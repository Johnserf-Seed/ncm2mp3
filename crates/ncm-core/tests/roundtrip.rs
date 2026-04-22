//! End-to-end test: build a synthetic NCM file from known inputs, then run
//! it through the real `NcmDecoder` and verify every layer reconstructs.

use std::io::Cursor;

use aes::Aes128;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use cipher::{block_padding::Pkcs7, BlockModeEncrypt, KeyInit};
use ecb::Encryptor;

use ncm_core::crypto::NcmStreamCipher;
use ncm_core::format::{
    AudioFormat, CORE_KEY, KEY_PREFIX, KEY_XOR_MASK, MAGIC, META_KEY, META_PLAIN_PREFIX,
    META_PREFIX, META_XOR_MASK,
};
use ncm_core::NcmDecoder;

type Aes128EcbEnc = Encryptor<Aes128>;

fn aes_encrypt(key: &[u8; 16], plaintext: &[u8]) -> Vec<u8> {
    Aes128EcbEnc::new(key.into()).encrypt_padded_vec::<Pkcs7>(plaintext)
}

fn build_synthetic_ncm(audio: &[u8], metadata_json: &str, cover: Option<&[u8]>) -> Vec<u8> {
    let rc4_key = b"test-rc4-key-material";

    let mut key_plain = Vec::new();
    key_plain.extend_from_slice(KEY_PREFIX);
    key_plain.extend_from_slice(rc4_key);
    let mut key_segment = aes_encrypt(&CORE_KEY, &key_plain);
    for byte in &mut key_segment {
        *byte ^= KEY_XOR_MASK;
    }

    let mut meta_plain = Vec::new();
    meta_plain.extend_from_slice(META_PREFIX);
    meta_plain.extend_from_slice(metadata_json.as_bytes());
    let meta_aes = aes_encrypt(&META_KEY, &meta_plain);
    let meta_b64 = BASE64.encode(&meta_aes);

    // Pre-base64, the real NCM format prepends a 22-byte ASCII tag so we
    // mirror that here before the XOR obfuscation step.
    let mut meta_segment: Vec<u8> = Vec::new();
    meta_segment.extend_from_slice(META_PLAIN_PREFIX);
    meta_segment.extend_from_slice(meta_b64.as_bytes());
    for byte in &mut meta_segment {
        *byte ^= META_XOR_MASK;
    }

    let cipher = NcmStreamCipher::new(rc4_key);
    let mut audio_enc = audio.to_vec();
    cipher.apply(&mut audio_enc, 0);

    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&[0, 0]); // gap
    out.extend_from_slice(&(key_segment.len() as u32).to_le_bytes());
    out.extend_from_slice(&key_segment);
    out.extend_from_slice(&(meta_segment.len() as u32).to_le_bytes());
    out.extend_from_slice(&meta_segment);
    out.extend_from_slice(&[0u8; 9]); // crc + gap

    if let Some(cover) = cover {
        out.extend_from_slice(&(cover.len() as u32).to_le_bytes());
        out.extend_from_slice(cover);
    } else {
        out.extend_from_slice(&0u32.to_le_bytes());
    }

    out.extend_from_slice(&audio_enc);
    out
}

#[test]
fn roundtrip_mp3_payload() {
    // Fake audio: ID3v2 header + some bytes. Large enough to cross chunk boundary.
    let mut audio = Vec::from(&b"ID3\x04\x00\x00\x00\x00\x00\x0A"[..]);
    audio.extend((0..100_000u32).map(|i| (i & 0xff) as u8));

    let metadata_json = r#"{
        "musicName": "Test Track",
        "artist": [["Tester", 42]],
        "album": "Roundtrip",
        "format": "mp3",
        "bitrate": 320000
    }"#;

    let cover_bytes = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F'];
    let ncm = build_synthetic_ncm(&audio, metadata_json, Some(cover_bytes));

    let (mut decoder, headers) = NcmDecoder::from_reader(Cursor::new(ncm)).unwrap();
    assert_eq!(headers.metadata.title, "Test Track");
    assert_eq!(headers.metadata.artists, vec!["Tester"]);
    assert_eq!(headers.metadata.album, "Roundtrip");
    assert_eq!(headers.effective_format(), AudioFormat::Mp3);
    assert!(headers.cover.is_some());
    let cover = headers.cover.as_ref().unwrap();
    assert_eq!(cover.mime, ncm_core::CoverMime::Jpeg);

    let mut out = Vec::new();
    let written = decoder.decode_to_writer(&mut out).unwrap();
    assert_eq!(written, audio.len() as u64);
    assert_eq!(out, audio);
}

#[test]
fn roundtrip_flac_without_cover() {
    let mut audio = Vec::from(&b"fLaC\x80\x00\x00"[..]);
    audio.extend((0..8192u32).map(|i| ((i * 13) & 0xff) as u8));

    let metadata_json = r#"{
        "musicName": "Flac Song",
        "artist": [["A", 1], ["B", 2]],
        "album": "Deluxe",
        "format": "flac"
    }"#;

    let ncm = build_synthetic_ncm(&audio, metadata_json, None);

    let (mut decoder, headers) = NcmDecoder::from_reader(Cursor::new(ncm)).unwrap();
    assert_eq!(headers.effective_format(), AudioFormat::Flac);
    assert_eq!(headers.metadata.artists, vec!["A", "B"]);
    assert!(headers.cover.is_none());

    let mut out = Vec::new();
    decoder.decode_to_writer(&mut out).unwrap();
    assert_eq!(out, audio);
}
