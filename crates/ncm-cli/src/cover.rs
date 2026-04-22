//! `cover` subcommand: extract the embedded cover image from one or more
//! NCM files without decrypting the audio stream.
//!
//! Output naming follows the input stem: `song.ncm` -> `song.jpg` (or
//! `song.png`, depending on the sniffed image MIME). For directory input,
//! every `.ncm` under the tree yields one cover file written alongside it
//! (or into `--output <dir>` if provided).

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
    let stem = ncm_path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("cover");

    let parent = match &args.output {
        Some(dir) => dir.clone(),
        None => ncm_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    fs::create_dir_all(&parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;

    let out_path = parent.join(format!("{stem}.{ext}"));
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
