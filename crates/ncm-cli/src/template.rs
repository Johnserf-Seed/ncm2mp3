//! Filename template rendering with per-placeholder sanitization.

use std::collections::HashMap;

use anyhow::{Context, Result};
use ncm_core::{AudioFormat, NcmMetadata};
use strfmt::strfmt;

/// Characters disallowed in Windows filenames. Unix accepts some of these but
/// we clean aggressively for cross-platform portability.
const INVALID_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Clean a single placeholder value. `/` is scrubbed here because the template
/// itself is what introduces directory separators — values that happen to
/// contain slashes must not create new subdirectories.
fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if INVALID_CHARS.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();

    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Render the filename stem (no extension) from the template and metadata.
pub fn render_stem(
    template: &str,
    metadata: &NcmMetadata,
    format: AudioFormat,
) -> Result<String> {
    let mut vars: HashMap<String, String> = HashMap::new();

    let artist = if metadata.artists.is_empty() {
        "unknown".to_string()
    } else {
        metadata.artists_joined(", ")
    };

    vars.insert("artist".into(), sanitize(&artist));
    vars.insert("album".into(), sanitize(&metadata.album));
    vars.insert("title".into(), sanitize(metadata.fallback_title()));
    vars.insert("format".into(), format.extension().to_string());
    vars.insert(
        "bitrate".into(),
        metadata
            .bitrate
            .map(|b| (b / 1000).to_string())
            .unwrap_or_else(|| "0".into()),
    );

    let rendered = strfmt(template, &vars)
        .with_context(|| format!("invalid filename template: {template}"))?;

    // Strip any leading/trailing slashes to prevent accidental absolute paths.
    Ok(rendered.trim_matches(['/', '\\']).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(title: &str, artists: &[&str], album: &str) -> NcmMetadata {
        NcmMetadata {
            title: title.to_string(),
            artists: artists.iter().map(|s| s.to_string()).collect(),
            album: album.to_string(),
            declared_format: AudioFormat::Mp3,
            bitrate: Some(320_000),
            duration: None,
            album_pic_url: None,
        }
    }

    #[test]
    fn basic_template() {
        let m = meta("Hello", &["Adele"], "25");
        let out = render_stem("{artist} - {title}", &m, AudioFormat::Mp3).unwrap();
        assert_eq!(out, "Adele - Hello");
    }

    #[test]
    fn nested_path_template() {
        let m = meta("Song", &["Artist"], "Album");
        let out = render_stem("{artist}/{album}/{title}", &m, AudioFormat::Flac).unwrap();
        assert_eq!(out, "Artist/Album/Song");
    }

    #[test]
    fn sanitizes_invalid_chars_in_values() {
        let m = meta("a:b/c", &["x?y"], "hello*world");
        let out = render_stem("{artist}/{album}/{title}", &m, AudioFormat::Mp3).unwrap();
        assert_eq!(out, "x_y/hello_world/a_b_c");
    }

    #[test]
    fn empty_fields_fallback() {
        let m = meta("", &[], "");
        let out = render_stem("{artist}/{title}", &m, AudioFormat::Mp3).unwrap();
        assert_eq!(out, "unknown/unknown");
    }

    #[test]
    fn bitrate_in_template() {
        let m = meta("t", &["a"], "b");
        let out = render_stem("{title}-{bitrate}k", &m, AudioFormat::Mp3).unwrap();
        assert_eq!(out, "t-320k");
    }

    #[test]
    fn multiple_artists_joined() {
        let m = meta("Collab", &["A", "B"], "X");
        let out = render_stem("{artist} - {title}", &m, AudioFormat::Mp3).unwrap();
        assert_eq!(out, "A, B - Collab");
    }

    #[test]
    fn unknown_placeholder_errors() {
        let m = meta("t", &["a"], "b");
        assert!(render_stem("{nope}", &m, AudioFormat::Mp3).is_err());
    }

    #[test]
    fn strips_leading_slash() {
        let m = meta("t", &["a"], "b");
        let out = render_stem("/{title}", &m, AudioFormat::Mp3).unwrap();
        assert_eq!(out, "t");
    }
}
