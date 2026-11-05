//! Segment-by-segment NCM file parser.
//!
//! Each `read_*` function expects the reader to be positioned at the start of
//! its segment and leaves the reader positioned at the next one. Usually you
//! won't call these directly — use [`NcmDecoder`](crate::NcmDecoder), which
//! stitches the segments together and head-sniffs the audio format.

use std::io::Read;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

use crate::crypto::{aes128_ecb_decrypt, NcmStreamCipher};
use crate::error::{NcmError, Result};
use crate::format::{
    is_ncm_magic, CoverMime, CORE_KEY, KEY_PREFIX, KEY_XOR_MASK, MAGIC, MAX_KEY_LEN, MAX_META_LEN,
    MAX_SEGMENT_LEN, META_KEY, META_PLAIN_PREFIX, META_PREFIX, META_XOR_MASK,
};
use crate::metadata::{NcmMetadata, RawMetadata};

/// How much of a segment's declared length is reserved before reading it.
/// Key and metadata segments fit whole; a larger segment's buffer grows only
/// as its bytes actually arrive.
const PREALLOC_LIMIT: usize = 64 * 1024;

/// Read and verify the 8-byte magic (`CTENFDAM`) + 2-byte gap at the start
/// of the file. Errors with [`NcmError::InvalidMagic`] on mismatch.
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

fn read_segment<R: Read>(
    reader: &mut R,
    len: u32,
    limit: u32,
    field: &'static str,
) -> Result<Vec<u8>> {
    if len > limit {
        return Err(NcmError::LengthTooLarge {
            declared: len as u64,
            limit: limit as u64,
        });
    }
    // The length field is just a claim: allocating all of it up front would
    // let a few-byte file reserve the full cap. Grow the buffer as bytes
    // arrive instead, so a truncated segment costs only what it holds.
    let mut buf = Vec::with_capacity((len as usize).min(PREALLOC_LIMIT));
    reader.by_ref().take(u64::from(len)).read_to_end(&mut buf)?;
    if buf.len() < len as usize {
        return Err(NcmError::UnexpectedEof(field));
    }
    Ok(buf)
}

/// Read and decode the RC4 key segment, returning the key bytes used to
/// initialize the audio stream cipher. The returned key is never empty.
pub fn read_rc4_key<R: Read>(reader: &mut R) -> Result<Vec<u8>> {
    let len = read_u32_le(reader, "rc4 key length")?;
    let mut blob = read_segment(reader, len, MAX_KEY_LEN, "rc4 key")?;

    for byte in &mut blob {
        *byte ^= KEY_XOR_MASK;
    }

    let mut key = aes128_ecb_decrypt(&CORE_KEY, &blob)?;

    // A blob holding only the prefix leaves no key, and `NcmStreamCipher`
    // can't be keyed with zero bytes; reject it here rather than panic there.
    if key.len() <= KEY_PREFIX.len() || !key.starts_with(KEY_PREFIX) {
        return Err(NcmError::InvalidKeyPrefix);
    }
    key.drain(..KEY_PREFIX.len());
    Ok(key)
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

    let mut blob = read_segment(reader, len, MAX_META_LEN, "metadata")?;

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

fn skip_bytes<R: Read>(reader: &mut R, len: u32, field: &'static str) -> Result<()> {
    let skipped = std::io::copy(&mut reader.by_ref().take(len as u64), &mut std::io::sink())?;
    if skipped < len as u64 {
        return Err(NcmError::UnexpectedEof(field));
    }
    Ok(())
}

/// Skip the 4-byte CRC32 and the 1-byte field after it (other
/// implementations call it the "image version") that sit between the
/// metadata and the cover frame. We don't validate the CRC — its computation
/// isn't documented and skipping it hasn't caused problems on real files.
pub fn skip_crc<R: Read>(reader: &mut R) -> Result<()> {
    skip_bytes(reader, 5, "crc")
}

/// Skip 9 bytes between metadata and cover, treating the cover frame length
/// as part of an opaque gap.
#[deprecated(
    note = "loses the cover frame length, so audio decrypts to noise on files with a padded \
            cover frame (NetEase Cloud Music 3.x); use `skip_crc` + `read_cover_frame`"
)]
pub fn skip_crc_gap<R: Read>(reader: &mut R) -> Result<()> {
    let mut skip = [0u8; 9];
    reader
        .read_exact(&mut skip)
        .map_err(|_| NcmError::UnexpectedEof("crc/gap"))?;
    Ok(())
}

/// Embedded album cover image.
#[derive(Debug, Clone)]
pub struct Cover {
    /// Detected image MIME, inferred from the first bytes of [`data`](Self::data).
    pub mime: CoverMime,
    /// Raw image bytes (JPEG or PNG) — no decryption needed.
    pub data: Vec<u8>,
}

/// Read the cover frame and leave the reader at the first audio byte.
///
/// The frame is a `u32` frame length, a `u32` image length, the image, then
/// padding up to the frame length. Older clients wrote frames exactly as long
/// as the image, but NetEase Cloud Music 3.x reserves extra space (often with
/// an empty image), so stopping right after the image leaves the reader
/// inside the frame and the audio decrypts to noise.
///
/// Returns `None` when the image length is zero (the NCM carried no cover).
pub fn read_cover_frame<R: Read>(reader: &mut R) -> Result<Option<Cover>> {
    let frame_len = read_u32_le(reader, "cover frame length")?;
    if frame_len > MAX_SEGMENT_LEN {
        return Err(NcmError::LengthTooLarge {
            declared: frame_len as u64,
            limit: MAX_SEGMENT_LEN as u64,
        });
    }
    let image_len = read_u32_le(reader, "cover length")?;
    let cover = read_image(reader, image_len)?;
    // A frame shorter than its image is malformed; treat it as unpadded, like
    // the pre-3.x layout, and let the audio head sniff flag anything worse.
    skip_bytes(
        reader,
        frame_len.saturating_sub(image_len),
        "cover frame padding",
    )?;
    Ok(cover)
}

/// Read a length-prefixed cover image, ignoring any cover frame padding.
#[deprecated(
    note = "doesn't skip cover frame padding, so audio decrypts to noise on files from \
            NetEase Cloud Music 3.x; use `skip_crc` + `read_cover_frame`"
)]
pub fn read_cover<R: Read>(reader: &mut R) -> Result<Option<Cover>> {
    let len = read_u32_le(reader, "cover length")?;
    read_image(reader, len)
}

fn read_image<R: Read>(reader: &mut R, len: u32) -> Result<Option<Cover>> {
    if len == 0 {
        return Ok(None);
    }
    let data = read_segment(reader, len, MAX_SEGMENT_LEN, "cover")?;
    let mime = CoverMime::detect(&data);
    Ok(Some(Cover { mime, data }))
}

/// Build an [`NcmStreamCipher`] from raw key bytes. Thin convenience over
/// [`NcmStreamCipher::new`](crate::crypto::NcmStreamCipher::new).
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

    /// A length-prefixed key segment whose AES plaintext is `plain`.
    fn key_segment(plain: &[u8]) -> Cursor<Vec<u8>> {
        use aes::Aes128;
        use cipher::{block_padding::Pkcs7, BlockModeEncrypt, KeyInit};

        let mut blob =
            ecb::Encryptor::<Aes128>::new((&CORE_KEY).into()).encrypt_padded_vec::<Pkcs7>(plain);
        for byte in &mut blob {
            *byte ^= KEY_XOR_MASK;
        }
        let mut bytes = (blob.len() as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(&blob);
        Cursor::new(bytes)
    }

    #[test]
    fn rc4_key_is_what_follows_the_prefix() {
        let mut plain = KEY_PREFIX.to_vec();
        plain.extend_from_slice(b"key-material");
        assert_eq!(
            read_rc4_key(&mut key_segment(&plain)).unwrap(),
            b"key-material"
        );
    }

    #[test]
    fn rc4_key_rejects_blob_holding_only_the_prefix() {
        // Used to come back as an empty key, which panicked (and, with
        // `panic = "abort"`, killed the whole process) in NcmStreamCipher.
        assert!(matches!(
            read_rc4_key(&mut key_segment(KEY_PREFIX)),
            Err(NcmError::InvalidKeyPrefix)
        ));
    }

    #[test]
    fn rc4_key_rejects_blob_without_prefix() {
        assert!(matches!(
            read_rc4_key(&mut key_segment(b"not-the-prefix-at-all")),
            Err(NcmError::InvalidKeyPrefix)
        ));
    }

    /// A length field with nothing after it.
    fn length_only(len: u32) -> Cursor<Vec<u8>> {
        Cursor::new(len.to_le_bytes().to_vec())
    }

    #[test]
    fn rc4_key_rejects_length_over_its_cap() {
        let result = read_rc4_key(&mut length_only(MAX_KEY_LEN + 1));
        assert!(matches!(
            result,
            Err(NcmError::LengthTooLarge { limit, .. }) if limit == MAX_KEY_LEN as u64
        ));
    }

    #[test]
    fn metadata_rejects_length_over_its_cap() {
        let result = read_metadata(&mut length_only(MAX_META_LEN + 1));
        assert!(matches!(
            result,
            Err(NcmError::LengthTooLarge { limit, .. }) if limit == MAX_META_LEN as u64
        ));
    }

    #[test]
    fn truncated_rc4_key_errors() {
        let mut bytes = 144u32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[0u8; 10]);
        assert!(matches!(
            read_rc4_key(&mut Cursor::new(bytes)),
            Err(NcmError::UnexpectedEof("rc4 key"))
        ));
    }

    #[test]
    fn cover_claiming_more_than_the_file_holds_errors() {
        // A cover length at the cap, followed by four bytes and EOF.
        let mut bytes = MAX_SEGMENT_LEN.to_le_bytes().to_vec();
        bytes.extend_from_slice(&MAX_SEGMENT_LEN.to_le_bytes());
        bytes.extend_from_slice(&JPEG_HEAD);
        assert!(matches!(
            read_cover_frame(&mut Cursor::new(bytes)),
            Err(NcmError::UnexpectedEof("cover"))
        ));
    }

    const JPEG_HEAD: [u8; 4] = [0xFF, 0xD8, 0xFF, 0xE0];

    fn cover_frame(frame_len: u32, image: &[u8], padding: &[u8]) -> Cursor<Vec<u8>> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&frame_len.to_le_bytes());
        bytes.extend_from_slice(&(image.len() as u32).to_le_bytes());
        bytes.extend_from_slice(image);
        bytes.extend_from_slice(padding);
        bytes.extend_from_slice(b"AUDIO");
        Cursor::new(bytes)
    }

    fn rest(cur: &mut Cursor<Vec<u8>>) -> Vec<u8> {
        let mut out = Vec::new();
        cur.read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn cover_frame_without_padding() {
        let mut cur = cover_frame(4, &JPEG_HEAD, &[]);
        let cover = read_cover_frame(&mut cur).unwrap().unwrap();
        assert_eq!(cover.mime, CoverMime::Jpeg);
        assert_eq!(cover.data, JPEG_HEAD);
        assert_eq!(rest(&mut cur), b"AUDIO");
    }

    #[test]
    fn cover_frame_skips_padding_after_image() {
        let mut cur = cover_frame(10, &JPEG_HEAD, &[0xAA; 6]);
        let cover = read_cover_frame(&mut cur).unwrap().unwrap();
        assert_eq!(cover.data, JPEG_HEAD);
        assert_eq!(rest(&mut cur), b"AUDIO");
    }

    #[test]
    fn cover_frame_with_empty_image_still_skips_frame() {
        // NetEase Cloud Music 3.x: image length 0, frame length non-zero.
        let mut cur = cover_frame(8, &[], &[0xAA; 8]);
        assert!(read_cover_frame(&mut cur).unwrap().is_none());
        assert_eq!(rest(&mut cur), b"AUDIO");
    }

    #[test]
    fn cover_frame_shorter_than_image_is_treated_as_unpadded() {
        let mut cur = cover_frame(0, &JPEG_HEAD, &[]);
        assert!(read_cover_frame(&mut cur).unwrap().is_some());
        assert_eq!(rest(&mut cur), b"AUDIO");
    }

    #[test]
    fn cover_frame_padding_past_eof_errors() {
        let mut cur = cover_frame(1_000, &[], &[]);
        assert!(matches!(
            read_cover_frame(&mut cur),
            Err(NcmError::UnexpectedEof("cover frame padding"))
        ));
    }

    #[test]
    fn cover_frame_rejects_oversized_frame() {
        let mut cur = cover_frame(MAX_SEGMENT_LEN + 1, &[], &[]);
        assert!(matches!(
            read_cover_frame(&mut cur),
            Err(NcmError::LengthTooLarge { .. })
        ));
    }
}
