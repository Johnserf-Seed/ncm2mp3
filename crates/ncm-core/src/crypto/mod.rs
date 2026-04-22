//! Cryptographic primitives used by the NCM parser.
//!
//! - [`aes128_ecb_decrypt`] — AES-128-ECB + PKCS#7, used for the RC4 key
//!   blob and the metadata blob.
//! - [`NcmStreamCipher`] — the NCM-specific stream cipher used on the audio
//!   segment. Standard RC4 key scheduling, custom PRGA that allows random
//!   access by byte offset.

pub mod aes;
pub mod rc4;

pub use aes::aes128_ecb_decrypt;
pub use rc4::NcmStreamCipher;
