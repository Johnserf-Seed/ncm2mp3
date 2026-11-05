//! `cover` subcommand: extract the embedded cover image from one or more
//! NCM files without decrypting the audio stream.
//!
//! Output naming follows the input stem: `song.ncm` -> `song.jpg` (or
//! `song.png`, depending on the sniffed image MIME). For directory input,
//! every `.ncm` under the tree yields one cover file written alongside it
//! (or into `--output <dir>` if provided).

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use console::style;
use ncm_core::{CoverMime, NcmDecoder};
use walkdir::WalkDir;

use crate::cli::CoverArgs;
use crate::i18n::t;
use crate::pipeline::has_ncm_extension;

pub fn run(args: &CoverArgs) -> Result<()> {
    let files = collect(&args.input, args.recursive)?;
    if files.is_empty() {
        log::warn!(
            "{}",
            t().msg_no_files
                .replace("{path}", &args.input.display().to_string())
        );
        return Ok(());
    }

    for ncm_path in &files {
        if let Err(e) = extract_one(ncm_path, args) {
            eprintln!("{} {}: {e:#}", style("✗").red().bold(), ncm_path.display());
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

/// The exported cover's file name: the input's stem with `ext`
/// (`song.ncm` -> `song.jpg`), or `cover.<ext>` if there is no stem. The
/// stem stays an `OsStr`: on Linux a file name needn't be valid UTF-8
/// (e.g. GBK-encoded), and converting it to `str` would fail.
fn cover_file_name(ncm_path: &Path, ext: &str) -> OsString {
    let stem = ncm_path
        .file_stem()
        .filter(|s| !s.is_empty())
        .unwrap_or(OsStr::new("cover"));
    let mut name = stem.to_os_string();
    name.push(".");
    name.push(ext);
    name
}

fn extract_one(ncm_path: &Path, args: &CoverArgs) -> Result<()> {
    // `NcmDecoder::open` parses the preamble including the cover segment —
    // we discard the decoder (audio stream untouched).
    let (_decoder, headers) = NcmDecoder::open(ncm_path)
        .with_context(|| format!("failed to parse {}", ncm_path.display()))?;

    let Some(cover) = headers.cover.as_ref() else {
        eprintln!(
            "{} {}",
            style("·").yellow(),
            t().msg_cover_no_cover
                .replace("{path}", &ncm_path.display().to_string())
        );
        return Ok(());
    };

    let ext = match cover.mime {
        CoverMime::Jpeg => "jpg",
        CoverMime::Png => "png",
        CoverMime::Unknown => {
            eprintln!(
                "{} {}",
                style("·").yellow(),
                t().msg_cover_unknown_mime
                    .replace("{path}", &ncm_path.display().to_string())
            );
            return Ok(());
        }
    };

    // Destination: <output dir or input's parent> / <input stem>.<ext>
    let parent = match &args.output {
        Some(dir) => dir.clone(),
        None => ncm_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    fs::create_dir_all(&parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;

    let out_path = parent.join(cover_file_name(ncm_path, ext));
    if out_path.exists() && !args.overwrite {
        eprintln!(
            "{} {}  ({})",
            style("·").yellow(),
            ncm_path.display(),
            t().msg_skipped_exists
                .replace("{path}", &out_path.display().to_string())
        );
        return Ok(());
    }

    fs::write(&out_path, &cover.data)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    eprintln!(
        "{} {}",
        style("✓").green().bold(),
        t().msg_cover_written
            .replace("{path}", &out_path.display().to_string())
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn cover_file_name_swaps_only_the_extension() {
        let name = cover_file_name(Path::new("/in/song.ncm"), "jpg");
        assert_eq!(name, "song.jpg");
        let name = cover_file_name(Path::new("/in/artist - song . live.ncm"), "png");
        assert_eq!(name, "artist - song . live.png");
    }

    #[test]
    fn cover_file_name_falls_back_to_cover_without_a_file_name() {
        assert_eq!(cover_file_name(Path::new("/in/.."), "jpg"), "cover.jpg");
    }

    // A Linux file name needn't be valid UTF-8: `\xb8\xe8` is "歌" in GBK.
    // Such names used to fall back to `cover.<ext>`, so a batch of them
    // collided. macOS refuses them on disk, so this only builds paths and
    // never creates the files.
    #[cfg(unix)]
    #[test]
    fn cover_file_name_keeps_non_utf8_stem() {
        let input = Path::new(OsStr::from_bytes(b"/in/\xb8\xe8.ncm"));
        assert_eq!(cover_file_name(input, "jpg").as_bytes(), b"\xb8\xe8.jpg");
        assert_eq!(cover_file_name(input, "png").as_bytes(), b"\xb8\xe8.png");
    }
}
