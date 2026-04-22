//! Metadata deserialization for the NCM JSON payload.
//!
//! The NCM container embeds a JSON blob with song info (title, artist, album,
//! format hint, bitrate, duration, cover URL). This module defines two
//! layers:
//!
//! - [`RawMetadata`] mirrors the on-wire JSON field names for easy
//!   debugging / cross-referencing with other implementations.
//! - [`NcmMetadata`] is the cleaned-up, user-facing shape — flattened
//!   artist list, mapped [`AudioFormat`] enum, accessor helpers.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::format::AudioFormat;

/// Raw metadata as found inside the NCM file. Keeps the wire names so field
/// mapping stays obvious when comparing against reference implementations.
#[derive(Debug, Deserialize)]
pub struct RawMetadata {
    /// Song title (wire name: `musicName`).
    #[serde(rename = "musicName", default)]
    pub music_name: String,

    /// Flattened artist names (wire name: `artist`, shape
    /// `[["Name", id], ...]`; IDs are discarded).
    #[serde(default, deserialize_with = "deserialize_artists")]
    pub artist: Vec<String>,

    /// Album name.
    #[serde(default)]
    pub album: String,

    /// Declared audio format hint — `"mp3"`, `"flac"`, etc. Treat as advisory
    /// only; real format is determined from the decrypted audio's magic bytes.
    #[serde(default)]
    pub format: String,

    /// Bitrate in bits per second (not kbps).
    #[serde(default)]
    pub bitrate: Option<u64>,

    /// Duration in milliseconds.
    #[serde(default)]
    pub duration: Option<u64>,

    /// Album cover image URL (wire name: `albumPic`). Not fetched by this
    /// crate; the embedded binary cover inside the NCM container is used.
    #[serde(rename = "albumPic", default)]
    pub album_pic: Option<String>,

    /// Alias / alternate title list (sometimes populated, often empty).
    #[serde(default)]
    pub alias: Vec<String>,
}

/// The shape artists appear in: `[["Name", 12345], ["Other", 67890]]`.
/// IDs are ignored; we only need the name strings.
fn deserialize_artists<'de, D>(de: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(de)?;
    let Some(value) = value else {
        return Ok(Vec::new());
    };

    let mut names = Vec::new();
    if let Value::Array(rows) = value {
        for row in rows {
            match row {
                Value::Array(pair) => {
                    if let Some(Value::String(name)) = pair.into_iter().next() {
                        if !name.is_empty() {
                            names.push(name);
                        }
                    }
                }
                Value::String(name) if !name.is_empty() => names.push(name),
                _ => {}
            }
        }
    }
    Ok(names)
}

/// Cleaned, user-facing metadata. Produced from [`RawMetadata`] via
/// [`NcmMetadata::from_raw`]. This is what you interact with from
/// [`NcmHeaders::metadata`](crate::NcmHeaders).
#[derive(Debug, Clone)]
pub struct NcmMetadata {
    /// Song title. May be empty; use [`fallback_title`](Self::fallback_title)
    /// if you need a non-empty string.
    pub title: String,
    /// Artist names. Empty if none were present in the NCM.
    pub artists: Vec<String>,
    /// Album name. May be empty.
    pub album: String,
    /// Audio format declared in the metadata. Advisory only — may disagree
    /// with the format sniffed from the actual audio head bytes.
    pub declared_format: AudioFormat,
    /// Bitrate in bits per second. Divide by 1000 for kbps.
    pub bitrate: Option<u64>,
    /// Duration in milliseconds.
    pub duration: Option<u64>,
    /// Album cover URL, if the NCM carried one. Not fetched.
    pub album_pic_url: Option<String>,
}

impl NcmMetadata {
    /// Convert a wire-shaped [`RawMetadata`] into the cleaned-up form.
    pub fn from_raw(raw: RawMetadata) -> Self {
        Self {
            title: raw.music_name,
            artists: raw.artist,
            album: raw.album,
            declared_format: AudioFormat::from_hint(&raw.format),
            bitrate: raw.bitrate,
            duration: raw.duration,
            album_pic_url: raw.album_pic,
        }
    }

    /// Join all artist names with `sep`. Returns an empty string if
    /// [`artists`](Self::artists) is empty.
    pub fn artists_joined(&self, sep: &str) -> String {
        self.artists.join(sep)
    }

    /// Return the title, or the literal `"unknown"` if the title is empty.
    /// Useful when the title needs to land in a filesystem path.
    pub fn fallback_title(&self) -> &str {
        if self.title.is_empty() {
            "unknown"
        } else {
            &self.title
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_metadata() {
        let json = r#"{
            "musicName": "Shape of You",
            "artist": [["Ed Sheeran", 12345]],
            "album": "Divide",
            "format": "mp3",
            "bitrate": 320000,
            "duration": 234000
        }"#;

        let raw: RawMetadata = serde_json::from_str(json).unwrap();
        let meta = NcmMetadata::from_raw(raw);

        assert_eq!(meta.title, "Shape of You");
        assert_eq!(meta.artists, vec!["Ed Sheeran"]);
        assert_eq!(meta.album, "Divide");
        assert_eq!(meta.declared_format, AudioFormat::Mp3);
        assert_eq!(meta.bitrate, Some(320000));
    }

    #[test]
    fn parses_multiple_artists() {
        let json = r#"{
            "musicName": "Collab",
            "artist": [["A", 1], ["B", 2], ["C", 3]],
            "album": "X",
            "format": "flac"
        }"#;

        let raw: RawMetadata = serde_json::from_str(json).unwrap();
        let meta = NcmMetadata::from_raw(raw);

        assert_eq!(meta.artists, vec!["A", "B", "C"]);
        assert_eq!(meta.declared_format, AudioFormat::Flac);
    }

    #[test]
    fn tolerates_missing_fields() {
        let json = r#"{"musicName": "Lonely"}"#;

        let raw: RawMetadata = serde_json::from_str(json).unwrap();
        let meta = NcmMetadata::from_raw(raw);

        assert_eq!(meta.title, "Lonely");
        assert!(meta.artists.is_empty());
        assert_eq!(meta.album, "");
        assert_eq!(meta.declared_format, AudioFormat::Unknown);
    }

    #[test]
    fn tolerates_string_id_in_artist_pair() {
        let json = r#"{
            "musicName": "x",
            "artist": [["Name", "stringId"]]
        }"#;

        let raw: RawMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(raw.artist, vec!["Name"]);
    }
}
