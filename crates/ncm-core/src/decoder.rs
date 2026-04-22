//! Top-level NCM decoder: parses headers, metadata, cover, then streams the
//! decrypted audio into a writer.

use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;

use crate::crypto::NcmStreamCipher;
use crate::error::{NcmError, Result};
use crate::format::{AudioFormat, STREAM_CHUNK_SIZE};
use crate::metadata::NcmMetadata;
use crate::parser::{
    read_and_verify_magic, read_cover, read_metadata, read_rc4_key, skip_crc_gap, Cover,
};

/// Headers and context available after parsing an NCM file's preamble.
#[derive(Debug, Clone)]
pub struct NcmHeaders {
    pub metadata: NcmMetadata,
    pub cover: Option<Cover>,
    pub detected_format: AudioFormat,
}

impl NcmHeaders {
    /// Prefer the sniffed magic over the declared format; fall back when the
    /// sniff is inconclusive.
    pub fn effective_format(&self) -> AudioFormat {
        if self.detected_format != AudioFormat::Unknown {
            self.detected_format
        } else {
            self.metadata.declared_format
        }
    }
}

pub struct NcmDecoder<R: Read> {
    reader: R,
    cipher: NcmStreamCipher,
    buffered_head: Vec<u8>,
    total_audio_read: usize,
}

impl NcmDecoder<BufReader<File>> {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<(Self, NcmHeaders)> {
        let file = File::open(path)?;
        let reader = BufReader::with_capacity(64 * 1024, file);
        Self::from_reader(reader)
    }
}

impl<R: Read> NcmDecoder<R> {
    /// Parse all header/metadata/cover segments and sniff the first chunk of
    /// audio to determine the real format. Returns the decoder (positioned at
    /// the start of audio with pre-decrypted head buffered) and the headers.
    pub fn from_reader(mut reader: R) -> Result<(Self, NcmHeaders)> {
        read_and_verify_magic(&mut reader)?;
        let key = read_rc4_key(&mut reader)?;
        let metadata = read_metadata(&mut reader)?;
        skip_crc_gap(&mut reader)?;
        let cover = read_cover(&mut reader)?;

        let cipher = NcmStreamCipher::new(&key);

        // Sniff the first chunk of audio so callers can know the real format
        // before any bytes are written out.
        let mut head = vec![0u8; 16];
        let read_n = fill_buf(&mut reader, &mut head)?;
        head.truncate(read_n);
        cipher.apply(&mut head, 0);

        let detected_format = AudioFormat::detect(&head);

        let headers = NcmHeaders {
            metadata,
            cover,
            detected_format,
        };

        let decoder = Self {
            reader,
            cipher,
            buffered_head: head,
            total_audio_read: 0,
        };

        Ok((decoder, headers))
    }

    /// Write the fully decrypted audio stream to `writer`.
    pub fn decode_to_writer<W: Write>(&mut self, writer: &mut W) -> Result<u64> {
        let mut total: u64 = 0;

        if !self.buffered_head.is_empty() {
            writer.write_all(&self.buffered_head)?;
            total += self.buffered_head.len() as u64;
            self.total_audio_read = self.buffered_head.len();
            self.buffered_head.clear();
        }

        let mut buf = vec![0u8; STREAM_CHUNK_SIZE];
        loop {
            let n = fill_buf(&mut self.reader, &mut buf)?;
            if n == 0 {
                break;
            }
            self.cipher.apply(&mut buf[..n], self.total_audio_read);
            writer.write_all(&buf[..n])?;
            self.total_audio_read += n;
            total += n as u64;
        }

        Ok(total)
    }
}

/// `read_exact` rejects short reads at EOF; we want "read as much as possible
/// up to buf.len()". Mirrors the old pre-`read_exact` stdlib idiom.
fn fill_buf<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(NcmError::Io(e)),
        }
    }
    Ok(filled)
}
