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
use crate::output::OutputFile;
use crate::pipeline::has_ncm_extension;
use crate::term::{printable, printable_path};

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
            eprintln!(
                "{} {}: {}",
                style("✗").red().bold(),
                printable_path(ncm_path),
                printable(&format!("{e:#}"))
            );
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
                .replace("{path}", &printable_path(ncm_path))
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
                    .replace("{path}", &printable_path(ncm_path))
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

    // Never write through a symlink at the destination (see `output`):
    // without --overwrite, create the file only if nothing is there yet;
    // with it, replace whatever is there.
    let out_path = parent.join(format!("{stem}.{ext}"));
    let created = if args.overwrite {
        OutputFile::replace(&out_path).map(Some)
    } else {
        OutputFile::create_new(&out_path)
    };
    let Some(output) =
        created.with_context(|| format!("failed to create {}", out_path.display()))?
    else {
        eprintln!(
            "{} {}  ({})",
            style("·").yellow(),
            printable_path(ncm_path),
            t().msg_skipped_exists
                .replace("{path}", &printable_path(&out_path))
        );
        return Ok(());
    };

    output
        .write_and_commit(&cover.data)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    eprintln!(
        "{} {}",
        style("✓").green().bold(),
        t().msg_cover_written
            .replace("{path}", &printable_path(&out_path))
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::write_ncm;

    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F'];
    const MP3: &[u8] = b"ID3\x04\x00\x00\x00\x00\x00\x00 frames";

    fn song_with_cover() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("song.ncm");
        write_ncm(&input, MP3, Some(JPEG));
        (dir, input)
    }

    fn args(input: &Path, overwrite: bool) -> CoverArgs {
        CoverArgs {
            input: input.to_path_buf(),
            output: None,
            recursive: false,
            overwrite,
        }
    }

    #[test]
    fn writes_the_cover_next_to_the_input() {
        let (dir, input) = song_with_cover();
        extract_one(&input, &args(&input, false)).unwrap();
        assert_eq!(fs::read(dir.path().join("song.jpg")).unwrap(), JPEG);
    }

    #[test]
    fn keeps_an_existing_cover_without_overwrite() {
        let (dir, input) = song_with_cover();
        let out = dir.path().join("song.jpg");
        fs::write(&out, b"mine").unwrap();
        extract_one(&input, &args(&input, false)).unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"mine");
    }

    #[test]
    fn overwrite_replaces_an_existing_cover() {
        let (dir, input) = song_with_cover();
        let out = dir.path().join("song.jpg");
        fs::write(&out, b"mine").unwrap();
        extract_one(&input, &args(&input, true)).unwrap();
        assert_eq!(fs::read(&out).unwrap(), JPEG);
    }

    #[cfg(unix)]
    mod symlinks {
        use super::*;
        use std::os::unix::fs::symlink;

        fn plant(dir: &Path) -> (PathBuf, PathBuf) {
            let victim = dir.join("victim");
            fs::write(&victim, b"precious").unwrap();
            let link = dir.join("song.jpg");
            symlink(&victim, &link).unwrap();
            (link, victim)
        }

        #[test]
        fn does_not_write_through_a_planted_symlink() {
            let (dir, input) = song_with_cover();
            let (_link, victim) = plant(dir.path());
            extract_one(&input, &args(&input, false)).unwrap();
            assert_eq!(fs::read(&victim).unwrap(), b"precious");
        }

        #[test]
        fn does_not_create_a_dangling_symlinks_target() {
            let (dir, input) = song_with_cover();
            let target = dir.path().join("planted");
            symlink(&target, dir.path().join("song.jpg")).unwrap();
            extract_one(&input, &args(&input, false)).unwrap();
            assert!(!target.exists());
        }

        #[test]
        fn overwrite_replaces_a_planted_symlink_instead_of_following_it() {
            let (dir, input) = song_with_cover();
            let (link, victim) = plant(dir.path());
            extract_one(&input, &args(&input, true)).unwrap();
            assert_eq!(fs::read(&victim).unwrap(), b"precious");
            assert!(fs::symlink_metadata(&link).unwrap().file_type().is_file());
            assert_eq!(fs::read(&link).unwrap(), JPEG);
        }
    }
}
