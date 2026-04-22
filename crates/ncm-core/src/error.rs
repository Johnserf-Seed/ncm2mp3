//! Error type shared across the crate.

use thiserror::Error;

/// All ways NCM parsing and decryption can fail.
///
/// Variants divide into two buckets:
/// - **Format errors** (`InvalidMagic`, `InvalidKeyPrefix`, `InvalidMetaHeader`,
///   `InvalidMetaPrefix`, `LengthTooLarge`, `UnexpectedEof`) mean the input
///   wasn't a well-formed NCM file — either corrupted, truncated, or a
///   completely different format that happened to share the extension.
/// - **Transform errors** (`Aes`, `Base64`, `Json`, `Utf8`, `Io`) wrap the
///   underlying library's error type via `#[from]`; they usually indicate an
///   I/O failure or that a previous stage produced garbage because an
///   earlier format check slipped through.
#[derive(Error, Debug)]
pub enum NcmError {
    /// The file didn't start with the expected 8-byte magic `CTENFDAM`.
    #[error("invalid NCM magic header (expected CTENFDAM)")]
    InvalidMagic,

    /// The stream ended earlier than a declared length field required.
    /// The `&str` identifies which segment was being read.
    #[error("unexpected end of file while reading {0}")]
    UnexpectedEof(&'static str),

    /// AES-decrypted RC4-key blob didn't start with `neteasecloudmusic`.
    #[error("RC4 key missing 'neteasecloudmusic' prefix")]
    InvalidKeyPrefix,

    /// XOR-deobfuscated metadata segment didn't start with the expected
    /// `163 key(Don't modify):` ASCII header — the base64 payload can't be
    /// located without it.
    #[error("metadata segment missing '163 key(Don't modify):' header")]
    InvalidMetaHeader,

    /// AES-decrypted metadata payload didn't start with the `music:` prefix
    /// that precedes the JSON object.
    #[error("metadata missing 'music:' prefix")]
    InvalidMetaPrefix,

    /// A segment's declared length field exceeds
    /// [`format::MAX_SEGMENT_LEN`](crate::format::MAX_SEGMENT_LEN). Thrown as
    /// a defensive check against attacker-controlled or corrupt length fields
    /// that would otherwise request a multi-gigabyte allocation.
    #[error("declared length {declared} exceeds sane limit ({limit})")]
    LengthTooLarge {
        /// The length value read from the file.
        declared: u64,
        /// The hard limit this crate enforces.
        limit: u64,
    },

    /// AES-128-ECB decryption or padding removal failed. Wraps the original
    /// error message from the `cipher` crate as a string.
    #[error("AES decryption failed: {0}")]
    Aes(String),

    /// Base64 decoding of the metadata segment failed.
    #[error("base64 decode failed: {0}")]
    Base64(#[from] base64::DecodeError),

    /// Parsing the JSON inside the decrypted metadata failed.
    #[error("JSON decode failed: {0}")]
    Json(#[from] serde_json::Error),

    /// Converting decrypted metadata bytes to UTF-8 failed.
    #[error("UTF-8 decode failed: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    /// An underlying `std::io` operation failed (reading the source file,
    /// writing the decrypted output, …).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Shorthand for `std::result::Result<T, NcmError>`.
pub type Result<T> = std::result::Result<T, NcmError>;
