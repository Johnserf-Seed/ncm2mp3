//! `info` subcommand: open an NCM file only as far as needed to print its
//! metadata — magic, RC4 key, metadata segment, cover segment, and a sniff
//! of the audio head. Avoids the full decrypt so it's near-instant even
//! for large files.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use console::style;
use ncm_core::{AudioFormat, CoverMime, NcmDecoder, NcmHeaders};
use walkdir::WalkDir;

use crate::cli::InfoArgs;
use crate::i18n::t;
use crate::pipeline::has_ncm_extension;

pub fn run(args: &InfoArgs) -> Result<()> {
    let files = collect(&args.input, args.recursive)?;
    if files.is_empty() {
        log::warn!(
            "{}",
            t().msg_no_files
                .replace("{path}", &args.input.display().to_string())
        );
        return Ok(());
    }

    let multi = files.len() > 1;
    for (idx, path) in files.iter().enumerate() {
        if multi && idx > 0 {
            println!();
        }
        match print_one(path) {
            Ok(()) => {}
            Err(e) => eprintln!("{} {}: {e:#}", style("✗").red().bold(), path.display()),
        }
    }

    Ok(())
}

fn collect(input: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    if !input.exists() {
        bail!(
            "{}",
            t().msg_input_missing
                .replace("{path}", &input.display().to_string())
        );
    }
    if input.is_file() {
        return Ok(vec![input.to_path_buf()]);
    }
    let max_depth = if recursive { usize::MAX } else { 1 };
    let mut out = Vec::new();
    for entry in WalkDir::new(input).max_depth(max_depth) {
        let entry = entry?;
        if entry.file_type().is_file() && has_ncm_extension(entry.path()) {
            out.push(entry.into_path());
        }
    }
    Ok(out)
}

fn print_one(path: &Path) -> Result<()> {
    let (_decoder, headers) =
        NcmDecoder::open(path).with_context(|| format!("failed to parse {}", path.display()))?;

    let file_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    for (label, value) in build_rows(path, file_size, &headers) {
        println!("{}: {}", style(label).bold().cyan(), value);
    }
    Ok(())
}

fn build_rows(path: &Path, file_size: u64, headers: &NcmHeaders) -> Vec<(&'static str, String)> {
    let s = t();
    let mut rows = Vec::new();

    rows.push((s.info_file, path.display().to_string()));
    rows.push((s.info_size, format_size(file_size)));
    rows.push((
        s.info_format,
        format_detected_format(headers.effective_format(), headers.metadata.declared_format),
    ));
    if let Some(b) = headers.metadata.bitrate {
        rows.push((s.info_bitrate, format!("{} kbps", b / 1000)));
    }
    if let Some(d) = headers.metadata.duration {
        rows.push((s.info_duration, format_duration(d)));
    }
    rows.push((s.info_title, or_none(&headers.metadata.title)));
    rows.push((
        s.info_artist,
        or_none(&headers.metadata.artists_joined(", ")),
    ));
    rows.push((s.info_album, or_none(&headers.metadata.album)));
    rows.push((
        s.info_cover,
        match headers.cover.as_ref() {
            Some(cover) => format!(
                "{}, {}",
                cover_mime_label(cover.mime),
                format_size(cover.data.len() as u64),
            ),
            None => s.info_none.to_string(),
        },
    ));

    rows
}

fn or_none(value: &str) -> String {
    if value.is_empty() {
        t().info_none.to_string()
    } else {
        value.to_string()
    }
}

fn cover_mime_label(mime: CoverMime) -> &'static str {
    match mime {
        CoverMime::Jpeg => "JPEG",
        CoverMime::Png => "PNG",
        CoverMime::Unknown => "unknown",
    }
}

fn format_detected_format(effective: AudioFormat, declared: AudioFormat) -> String {
    let eff_name = audio_format_name(effective);
    // Surface a mismatch between sniffed and declared only when the
    // declared hint is actually meaningful (not Unknown) and differs.
    if declared != AudioFormat::Unknown && declared != effective {
        format!("{} (declared: {})", eff_name, audio_format_name(declared))
    } else {
        eff_name.to_string()
    }
}

fn audio_format_name(f: AudioFormat) -> &'static str {
    match f {
        AudioFormat::Mp3 => "MP3",
        AudioFormat::Flac => "FLAC",
        AudioFormat::M4a => "M4A",
        AudioFormat::Wav => "WAV",
        AudioFormat::Ogg => "OGG",
        AudioFormat::Unknown => "unknown",
    }
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn format_duration_short_and_long() {
        assert_eq!(format_duration(0), "00:00");
        assert_eq!(format_duration(59_000), "00:59");
        assert_eq!(format_duration(234_000), "03:54");
        assert_eq!(format_duration(3_661_000), "01:01:01");
    }

    #[test]
    fn format_mismatch_shows_both() {
        assert_eq!(
            format_detected_format(AudioFormat::Flac, AudioFormat::Mp3),
            "FLAC (declared: MP3)"
        );
    }

    #[test]
    fn format_matching_shows_one() {
        assert_eq!(
            format_detected_format(AudioFormat::Mp3, AudioFormat::Mp3),
            "MP3"
        );
    }

    #[test]
    fn format_unknown_declared_shows_only_effective() {
        assert_eq!(
            format_detected_format(AudioFormat::Mp3, AudioFormat::Unknown),
            "MP3"
        );
    }
}
