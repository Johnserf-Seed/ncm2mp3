//! Parse each segment of an NCM file from a `Read` source.

use std::io::Read;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

use crate::crypto::{aes128_ecb_decrypt, NcmStreamCipher};
use crate::error::{NcmError, Result};
use crate::format::{
    is_ncm_magic, CoverMime, CORE_KEY, KEY_PREFIX, KEY_XOR_MASK, MAGIC, MAX_SEGMENT_LEN, META_KEY,
    META_PLAIN_PREFIX, META_PREFIX, META_XOR_MASK,
};
use crate::metadata::{NcmMetadata, RawMetadata};

pub fn read_and_verify_magic<R: Read>(reader: &mut R) -> Result<()> {
    let mut magic = [0u8; MAGIC.len()];
    reader
        .read_exact(&mut magic)
        .map_err(|_| NcmError::UnexpectedEof("magic"))?;
    if !is_ncm_magic(&magic) {
        return Err(NcmError::InvalidMagic);
    }
    let mut gap = [0u8; 2];
    reader
        .read_exact(&mut gap)
        .map_err(|_| NcmError::UnexpectedEof("post-magic gap"))?;
    Ok(())
}

fn read_u32_le<R: Read>(reader: &mut R, field: &'static str) -> Result<u32> {
    let mut buf = [0u8; 4];
    reader
        .read_exact(&mut buf)
        .map_err(|_| NcmError::UnexpectedEof(field))?;
    Ok(u32::from_le_bytes(buf))
}

fn read_segment<R: Read>(reader: &mut R, len: u32, field: &'static str) -> Result<Vec<u8>> {
    if len > MAX_SEGMENT_LEN {
        return Err(NcmError::LengthTooLarge {
            declared: len as u64,
            limit: MAX_SEGMENT_LEN as u64,
        });
    }
    let mut buf = vec![0u8; len as usize];
    reader
        .read_exact(&mut buf)
        .map_err(|_| NcmError::UnexpectedEof(field))?;
    Ok(buf)
}

/// Read and decode the RC4 key segment, returning the key bytes used to
/// initialize the audio stream cipher.
pub fn read_rc4_key<R: Read>(reader: &mut R) -> Result<Vec<u8>> {
    let len = read_u32_le(reader, "rc4 key length")?;
    let mut blob = read_segment(reader, len, "rc4 key")?;

    for byte in &mut blob {
        *byte ^= KEY_XOR_MASK;
    }

    let decrypted = aes128_ecb_decrypt(&CORE_KEY, &blob)?;

    if !decrypted.starts_with(KEY_PREFIX) {
        return Err(NcmError::InvalidKeyPrefix);
    }

    Ok(decrypted[KEY_PREFIX.len()..].to_vec())
}

/// Read and decode the metadata JSON segment. Empty metadata (length zero) is
/// allowed and produces an empty `NcmMetadata`.
pub fn read_metadata<R: Read>(reader: &mut R) -> Result<NcmMetadata> {
    let len = read_u32_le(reader, "metadata length")?;
    if len == 0 {
        return Ok(NcmMetadata::from_raw(RawMetadata {
            music_name: String::new(),
            artist: Vec::new(),
            album: String::new(),
            format: String::new(),
            bitrate: None,
            duration: None,
            album_pic: None,
            alias: Vec::new(),
        }));
    }

    let mut blob = read_segment(reader, len, "metadata")?;

    for byte in &mut blob {
        *byte ^= META_XOR_MASK;
    }

    // After XOR, the segment starts with the literal ASCII tag
    // "163 key(Don't modify):" before the base64 payload begins.
    if !blob.starts_with(META_PLAIN_PREFIX) {
        return Err(NcmError::InvalidMetaHeader);
    }
    let b64_section = &blob[META_PLAIN_PREFIX.len()..];

    let decoded = BASE64.decode(b64_section)?;
    let decrypted = aes128_ecb_decrypt(&META_KEY, &decoded)?;

    if !decrypted.starts_with(META_PREFIX) {
        return Err(NcmError::InvalidMetaPrefix);
    }

    let json_bytes = &decrypted[META_PREFIX.len()..];
    let raw: RawMetadata = serde_json::from_slice(json_bytes)?;
    Ok(NcmMetadata::from_raw(raw))
}

/// Skip the CRC32 + 5-byte gap that sits between metadata and cover.
pub fn skip_crc_gap<R: Read>(reader: &mut R) -> Result<()> {
    let mut skip = [0u8; 9];
    reader
        .read_exact(&mut skip)
        .map_err(|_| NcmError::UnexpectedEof("crc/gap"))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Cover {
    pub mime: CoverMime,
    pub data: Vec<u8>,
}

pub fn read_cover<R: Read>(reader: &mut R) -> Result<Option<Cover>> {
    let len = read_u32_le(reader, "cover length")?;
    if len == 0 {
        return Ok(None);
    }
    let data = read_segment(reader, len, "cover")?;
    let mime = CoverMime::detect(&data);
    Ok(Some(Cover { mime, data }))
}

pub fn stream_cipher_from_key(key: &[u8]) -> NcmStreamCipher {
    NcmStreamCipher::new(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn magic_rejects_bogus_header() {
        let mut data = Cursor::new(b"NOTCTENFDAM....".to_vec());
        assert!(matches!(
            read_and_verify_magic(&mut data),
            Err(NcmError::InvalidMagic)
        ));
    }

    #[test]
    fn magic_accepts_real_header() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&[0, 0]); // gap
        let mut cur = Cursor::new(bytes);
        assert!(read_and_verify_magic(&mut cur).is_ok());
    }

    #[test]
    fn rejects_oversized_segment() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(MAX_SEGMENT_LEN + 1).to_le_bytes());
        let mut cur = Cursor::new(bytes);
        let result = read_rc4_key(&mut cur);
        assert!(matches!(result, Err(NcmError::LengthTooLarge { .. })));
    }
}
