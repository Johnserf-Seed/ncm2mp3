use thiserror::Error;

#[derive(Error, Debug)]
pub enum NcmError {
    #[error("invalid NCM magic header (expected CTENFDAM)")]
    InvalidMagic,

    #[error("unexpected end of file while reading {0}")]
    UnexpectedEof(&'static str),

    #[error("RC4 key missing 'neteasecloudmusic' prefix")]
    InvalidKeyPrefix,

    #[error("metadata missing 'music:' prefix")]
    InvalidMetaPrefix,

    #[error("declared length {declared} exceeds sane limit ({limit})")]
    LengthTooLarge { declared: u64, limit: u64 },

    #[error("AES decryption failed: {0}")]
    Aes(String),

    #[error("base64 decode failed: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("JSON decode failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("UTF-8 decode failed: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, NcmError>;
