//! Core library for decrypting Netease Cloud Music NCM files.
//!
//! NCM files are a simple container format: a fixed-key AES layer over the
//! song metadata, and an NCM-specific RC4-like stream cipher over the audio
//! bytes. The audio inside is a plain MP3 / FLAC / M4A / etc. — this crate
//! strips the encryption layer; it does *not* transcode.
//!
//! # Quick start
//!
//! Open an NCM file and stream the decrypted audio into any `Write`:
//!
//! ```no_run
//! use ncm2mp3_core::NcmDecoder;
//!
//! let (mut decoder, headers) = NcmDecoder::open("song.ncm").unwrap();
//! let mut out = std::fs::File::create("song.mp3").unwrap();
//! decoder.decode_to_writer(&mut out).unwrap();
//! println!("title: {}", headers.metadata.title);
//! ```
//!
//! The returned [`NcmHeaders`] also contains the cover art bytes (if any) and
//! the detected audio format, both available *before* you start decrypting
//! the audio — useful for choosing an output extension or filtering files.
//!
//! # Format reference
//!
//! For the byte-level layout, the exact AES keys, XOR masks, and the math
//! behind the custom RC4-variant stream cipher, see
//! [`docs/ARCHITECTURE.md`](https://github.com/Johnserf-Seed/ncm2mp3/blob/main/docs/ARCHITECTURE.md)
//! in the repository.

#![warn(missing_docs)]

pub mod crypto;
pub mod decoder;
pub mod error;
pub mod format;
pub mod metadata;
pub mod parser;

pub use decoder::{NcmDecoder, NcmHeaders};
pub use error::{NcmError, Result};
pub use format::{AudioFormat, CoverMime};
pub use metadata::NcmMetadata;
pub use parser::Cover;
