//! Core library for decrypting Netease Cloud Music NCM files.
//!
//! Open an NCM file and stream the decrypted audio into any `Write`:
//!
//! ```no_run
//! use ncm_core::NcmDecoder;
//!
//! let (mut decoder, headers) = NcmDecoder::open("song.ncm").unwrap();
//! let mut out = std::fs::File::create("song.mp3").unwrap();
//! decoder.decode_to_writer(&mut out).unwrap();
//! println!("title: {}", headers.metadata.title);
//! ```

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
