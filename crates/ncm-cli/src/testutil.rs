//! Fixtures shared by the unit tests of several modules.

use std::path::Path;

/// Write a minimal NCM: a real key segment, no metadata, `cover` (if any) in
/// an unpadded cover frame, and `audio` under the NCM stream cipher.
pub(crate) fn write_ncm(path: &Path, audio: &[u8], cover: Option<&[u8]>) {
    use aes::Aes128;
    use cipher::{block_padding::Pkcs7, BlockModeEncrypt, KeyInit};
    use ncm_core::crypto::NcmStreamCipher;
    use ncm_core::format::{CORE_KEY, KEY_PREFIX, KEY_XOR_MASK, MAGIC};

    let rc4_key = b"pipeline-test-key";
    let mut key_plain = KEY_PREFIX.to_vec();
    key_plain.extend_from_slice(rc4_key);
    let mut key_segment =
        ecb::Encryptor::<Aes128>::new((&CORE_KEY).into()).encrypt_padded_vec::<Pkcs7>(&key_plain);
    for byte in &mut key_segment {
        *byte ^= KEY_XOR_MASK;
    }

    let mut audio_enc = audio.to_vec();
    NcmStreamCipher::new(rc4_key).apply(&mut audio_enc, 0);

    let image = cover.unwrap_or_default();
    let mut ncm = MAGIC.to_vec();
    ncm.extend_from_slice(&[0, 0]); // gap
    ncm.extend_from_slice(&(key_segment.len() as u32).to_le_bytes());
    ncm.extend_from_slice(&key_segment);
    ncm.extend_from_slice(&0u32.to_le_bytes()); // no metadata
    ncm.extend_from_slice(&[0u8; 5]); // crc32 + image version
    ncm.extend_from_slice(&(image.len() as u32).to_le_bytes()); // cover frame length
    ncm.extend_from_slice(&(image.len() as u32).to_le_bytes()); // image length
    ncm.extend_from_slice(image);
    ncm.extend_from_slice(&audio_enc);
    std::fs::write(path, ncm).unwrap();
}
