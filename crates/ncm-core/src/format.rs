//! NCM binary format constants and audio format detection.

pub const MAGIC: [u8; 8] = [0x43, 0x54, 0x45, 0x4E, 0x46, 0x44, 0x41, 0x4D];

pub const CORE_KEY: [u8; 16] = *b"hzHRAmso5kInbaxW";
pub const META_KEY: [u8; 16] = *b"#14ljk_!\\]&0U<'(";

pub const KEY_XOR_MASK: u8 = 0x64;
pub const META_XOR_MASK: u8 = 0x63;

pub const KEY_PREFIX: &[u8] = b"neteasecloudmusic";
pub const META_PREFIX: &[u8] = b"music:";

/// Fixed ASCII header that precedes the base64 payload inside the metadata
/// segment, visible only after XORing the segment with `META_XOR_MASK`.
pub const META_PLAIN_PREFIX: &[u8] = b"163 key(Don't modify):";

/// Reject any length field larger than 64 MiB. Legit keys and metadata are
/// only a few kilobytes; covers are at most a few MiB. Anything larger is
/// either a corrupt file or an attacker-controlled length field.
pub const MAX_SEGMENT_LEN: u32 = 64 * 1024 * 1024;

/// Recommended read chunk for streaming the audio segment.
pub const STREAM_CHUNK_SIZE: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Mp3,
    Flac,
    M4a,
    Wav,
    Ogg,
    Unknown,
}

impl AudioFormat {
    pub fn extension(self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Flac => "flac",
            AudioFormat::M4a => "m4a",
            AudioFormat::Wav => "wav",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Unknown => "bin",
        }
    }

    pub fn from_hint(hint: &str) -> Self {
        match hint.to_ascii_lowercase().as_str() {
            "mp3" => AudioFormat::Mp3,
            "flac" => AudioFormat::Flac,
            "m4a" | "aac" | "mp4" => AudioFormat::M4a,
            "wav" => AudioFormat::Wav,
            "ogg" | "oga" | "vorbis" => AudioFormat::Ogg,
            _ => AudioFormat::Unknown,
        }
    }

    /// Detect audio format from the first bytes of the decrypted stream.
    pub fn detect(head: &[u8]) -> Self {
        if head.len() >= 3 && &head[..3] == b"ID3" {
            return AudioFormat::Mp3;
        }
        if head.len() >= 2 && head[0] == 0xFF && (head[1] & 0xE0) == 0xE0 {
            return AudioFormat::Mp3;
        }
        if head.len() >= 4 && &head[..4] == b"fLaC" {
            return AudioFormat::Flac;
        }
        if head.len() >= 8 && &head[4..8] == b"ftyp" {
            return AudioFormat::M4a;
        }
        if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WAVE" {
            return AudioFormat::Wav;
        }
        if head.len() >= 4 && &head[..4] == b"OggS" {
            return AudioFormat::Ogg;
        }
        AudioFormat::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverMime {
    Jpeg,
    Png,
    Unknown,
}

impl CoverMime {
    pub fn detect(head: &[u8]) -> Self {
        if head.len() >= 3 && head[0] == 0xFF && head[1] == 0xD8 && head[2] == 0xFF {
            return CoverMime::Jpeg;
        }
        if head.len() >= 8 && head[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
            return CoverMime::Png;
        }
        CoverMime::Unknown
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CoverMime::Jpeg => "image/jpeg",
            CoverMime::Png => "image/png",
            CoverMime::Unknown => "application/octet-stream",
        }
    }
}

pub fn is_ncm_magic(bytes: &[u8]) -> bool {
    bytes.len() >= MAGIC.len() && bytes[..MAGIC.len()] == MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_matches() {
        assert!(is_ncm_magic(b"CTENFDAM"));
        assert!(is_ncm_magic(b"CTENFDAMextra"));
        assert!(!is_ncm_magic(b"CTENFDA"));
        assert!(!is_ncm_magic(b"nope1234"));
    }

    #[test]
    fn detect_mp3_id3() {
        assert_eq!(AudioFormat::detect(b"ID3\x03\x00"), AudioFormat::Mp3);
    }

    #[test]
    fn detect_mp3_sync() {
        assert_eq!(AudioFormat::detect(&[0xFF, 0xFB, 0x90]), AudioFormat::Mp3);
        assert_eq!(AudioFormat::detect(&[0xFF, 0xFA, 0x90]), AudioFormat::Mp3);
        assert_eq!(AudioFormat::detect(&[0xFF, 0xE0]), AudioFormat::Mp3);
    }

    #[test]
    fn detect_flac() {
        assert_eq!(AudioFormat::detect(b"fLaC\x00\x00"), AudioFormat::Flac);
    }

    #[test]
    fn detect_m4a() {
        assert_eq!(
            AudioFormat::detect(&[0, 0, 0, 0x20, b'f', b't', b'y', b'p', b'M', b'4', b'A', 0x20]),
            AudioFormat::M4a
        );
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(AudioFormat::detect(b"random"), AudioFormat::Unknown);
    }

    #[test]
    fn detect_jpeg() {
        assert_eq!(CoverMime::detect(&[0xFF, 0xD8, 0xFF, 0xE0]), CoverMime::Jpeg);
    }

    #[test]
    fn detect_png() {
        assert_eq!(
            CoverMime::detect(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0]),
            CoverMime::Png
        );
    }
}
