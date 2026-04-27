//! Orchestrate the full decrypt + tag workflow for single files and
//! directories, with optional parallelism.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use console::style;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ncm_core::{AudioFormat, Cover, CoverMime, NcmDecoder, NcmHeaders};
use rayon::prelude::*;
use walkdir::WalkDir;

use crate::cli::{Cli, ConflictStrategy};
use crate::i18n::t;
use crate::{tagger, template};

/// Final per-run tallies surfaced to `main.rs` for the summary line.
#[derive(Debug)]
pub struct RunSummary {
    pub ok: usize,
    pub skipped: usize,
    pub failed: usize,
    /// Wall-clock time from the start of `run()` to the end of the parallel
    /// loop. Excludes argv parsing and summary formatting.
    pub elapsed: Duration,
    /// Per-format counts, keyed by the *effective* (sniffed or declared)
    /// audio format of each successfully written file.
    pub by_format: HashMap<AudioFormat, usize>,
}

pub fn run(args: &Cli) -> Result<RunSummary> {
    let started = Instant::now();
    let files = collect_inputs(args)?;
    if files.is_empty() {
        let hint_path = args
            .input
            .as_deref()
            .or(args.from_file.as_deref())
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        log::warn!("{}", t().msg_no_files.replace("{path}", &hint_path));
        return Ok(RunSummary {
            ok: 0,
            skipped: 0,
            failed: 0,
            elapsed: started.elapsed(),
            by_format: HashMap::new(),
        });
    }

    log::info!(
        "{}",
        t().msg_queued.replace("{count}", &files.len().to_string())
    );

    let ok = Arc::new(AtomicUsize::new(0));
    let skipped = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));
    let by_format: Arc<Mutex<HashMap<AudioFormat, usize>>> = Arc::new(Mutex::new(HashMap::new()));

    let pool = thread_pool(args.jobs)?;

    pool.install(|| {
        files
            .par_iter()
            .for_each(|input| match process_one(input, args) {
                Ok(Outcome::Written { path, format }) => {
                    ok.fetch_add(1, Ordering::Relaxed);
                    if let Ok(mut map) = by_format.lock() {
                        *map.entry(format).or_insert(0) += 1;
                    }
                    eprintln!(
                        "{} {} -> {}",
                        style("✓").green().bold(),
                        input.display(),
                        path.display()
                    );
                }
                Ok(Outcome::Skipped(reason)) => {
                    skipped.fetch_add(1, Ordering::Relaxed);
                    eprintln!("{} {}  ({reason})", style("·").yellow(), input.display());
                }
                Ok(Outcome::DryRun(path)) => {
                    ok.fetch_add(1, Ordering::Relaxed);
                    eprintln!(
                        "{} {} -> {}",
                        style(t().msg_dry_run_prefix).cyan(),
                        input.display(),
                        path.display()
                    );
                }
                Err(e) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    eprintln!("{} {}: {e:#}", style("✗").red().bold(), input.display());
                }
            });
    });

    let by_format_final = by_format.lock().map(|m| m.clone()).unwrap_or_default();

    Ok(RunSummary {
        ok: ok.load(Ordering::Relaxed),
        skipped: skipped.load(Ordering::Relaxed),
        failed: failed.load(Ordering::Relaxed),
        elapsed: started.elapsed(),
        by_format: by_format_final,
    })
}

fn thread_pool(jobs: Option<usize>) -> Result<rayon::ThreadPool> {
    let mut builder = rayon::ThreadPoolBuilder::new();
    if let Some(n) = jobs {
        if n == 0 {
            bail!("{}", t().err_jobs_zero);
        }
        builder = builder.num_threads(n);
    }
    builder.build().context("failed to build rayon thread pool")
}

/// Merge files from the positional INPUT (single file or directory) with
/// `--from-file <list>` entries, apply `--exclude` glob filters, dedupe,
/// then cap at `--limit` if set.
fn collect_inputs(args: &Cli) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();

    if let Some(input) = args.input.as_deref() {
        files.extend(collect_from_path(input, args.recursive)?);
    }

    if let Some(list) = args.from_file.as_deref() {
        for path in read_file_list(list)? {
            files.extend(collect_from_path(&path, args.recursive)?);
        }
    }

    files.sort();
    files.dedup();

    // Apply exclude patterns (if any) against the full path string.
    if !args.exclude.is_empty() {
        let excluder = build_excluder(&args.exclude)?;
        files.retain(|p| !excluder.is_match(p.to_string_lossy().as_ref()));
    }

    // Cap the list length per --limit after exclusion.
    if let Some(n) = args.limit {
        files.truncate(n);
    }

    Ok(files)
}

/// Compile a list of glob patterns into a single `GlobSet` for fast match.
fn build_excluder(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pat in patterns {
        let glob = Glob::new(pat).with_context(|| format!("invalid --exclude pattern: {pat}"))?;
        builder.add(glob);
    }
    builder.build().context("failed to build exclude glob set")
}

/// Expand a single path into `.ncm` files it represents. A file path is
/// returned as-is (regardless of extension, so the user can force a specific
/// file even without `.ncm` suffix); a directory is walked with optional
/// recursion, keeping only `.ncm` files.
fn collect_from_path(input: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
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

/// Read a text file as a newline-separated list of paths. Blank lines and
/// `#`-prefixed comments are ignored. Whitespace is trimmed.
fn read_file_list(list_path: &Path) -> Result<Vec<PathBuf>> {
    let file = File::open(list_path)
        .with_context(|| format!("failed to open --from-file list: {}", list_path.display()))?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!("failed to read line {} of {}", idx + 1, list_path.display())
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        out.push(PathBuf::from(trimmed));
    }
    Ok(out)
}

/// Whether the given path has a (case-insensitive) `.ncm` extension.
/// Shared with other modules (info, cover, …) via `pub(crate)`.
pub(crate) fn has_ncm_extension(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|s| s.eq_ignore_ascii_case("ncm"))
        .unwrap_or(false)
}

/// Per-file pipeline outcome. Exposed to sibling modules (e.g. `watch`) so
/// they can drive the same engine without reimplementing the decrypt flow.
pub(crate) enum Outcome {
    /// Decryption produced the audio file at `path` with the sniffed or
    /// declared `format`. Used to build the per-format breakdown.
    Written {
        path: PathBuf,
        format: AudioFormat,
    },
    DryRun(PathBuf),
    Skipped(String),
}

pub(crate) fn process_one(input: &Path, args: &Cli) -> Result<Outcome> {
    let (mut decoder, headers) =
        NcmDecoder::open(input).with_context(|| format!("failed to parse {}", input.display()))?;

    let format = headers.effective_format();

    if !args.format.is_empty() && !format_matches(&args.format, format) {
        return Ok(Outcome::Skipped(
            t().msg_skipped_fmt
                .replace("{fmt}", format.extension())
                .to_string(),
        ));
    }

    let planned = build_output_path(input, args, &headers, format)?;

    // Resolve conflict strategy to a concrete target path — or bail out as
    // a "skipped" Outcome when policy says to leave the existing file alone.
    let out_path = match resolve_conflict(&planned, args.on_conflict)? {
        ConflictOutcome::Write(p) => p,
        ConflictOutcome::Skip => {
            return Ok(Outcome::Skipped(
                t().msg_skipped_exists
                    .replace("{path}", &planned.display().to_string())
                    .to_string(),
            ));
        }
    };

    if args.dry_run {
        return Ok(Outcome::DryRun(out_path));
    }

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    {
        let file = File::create(&out_path)
            .with_context(|| format!("failed to create {}", out_path.display()))?;
        let mut writer = BufWriter::with_capacity(64 * 1024, file);
        decoder
            .decode_to_writer(&mut writer)
            .with_context(|| format!("failed to decrypt {}", input.display()))?;
    }

    if !args.no_tag {
        if let Err(e) = tagger::write_tags(&out_path, &headers.metadata, headers.cover.as_ref()) {
            log::warn!(
                "{}: {e:#}",
                t().msg_tagging_failed
                    .replace("{path}", &out_path.display().to_string())
            );
        }
    }

    if args.folder {
        if let Some(cover) = headers.cover.as_ref() {
            if let Err(e) = write_cover_next_to(&out_path, cover) {
                log::warn!(
                    "failed to write external cover next to {}: {e:#}",
                    out_path.display()
                );
            }
        }
    }

    Ok(Outcome::Written {
        path: out_path,
        format,
    })
}

enum ConflictOutcome {
    /// Write to this (possibly-renamed) path.
    Write(PathBuf),
    /// Existing file means this input should be skipped entirely.
    Skip,
}

/// Apply the user's [`ConflictStrategy`] to a planned output path.
fn resolve_conflict(planned: &Path, strategy: ConflictStrategy) -> Result<ConflictOutcome> {
    if !planned.exists() {
        return Ok(ConflictOutcome::Write(planned.to_path_buf()));
    }
    match strategy {
        ConflictStrategy::Skip => Ok(ConflictOutcome::Skip),
        ConflictStrategy::Overwrite => Ok(ConflictOutcome::Write(planned.to_path_buf())),
        ConflictStrategy::Rename => find_free_rename(planned),
    }
}

/// Probe `foo-1.ext`, `foo-2.ext`, … up to a reasonable ceiling.
fn find_free_rename(planned: &Path) -> Result<ConflictOutcome> {
    let parent = planned.parent().unwrap_or_else(|| Path::new(""));
    let stem = planned
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("output");
    let ext = planned.extension().and_then(OsStr::to_str).unwrap_or("");

    for n in 1..=9999u32 {
        let candidate = if ext.is_empty() {
            parent.join(format!("{stem}-{n}"))
        } else {
            parent.join(format!("{stem}-{n}.{ext}"))
        };
        if !candidate.exists() {
            return Ok(ConflictOutcome::Write(candidate));
        }
    }
    Err(anyhow!(
        "{}",
        t().err_rename_exhausted
            .replace("{path}", &planned.display().to_string())
    ))
}

/// Write a `Cover` as `cover.jpg` / `cover.png` in the same directory as
/// `audio_path`. Exposed as `pub(crate)` so `cover.rs` can reuse the same
/// extension logic.
pub(crate) fn write_cover_next_to(audio_path: &Path, cover: &Cover) -> Result<()> {
    let ext = match cover.mime {
        CoverMime::Jpeg => "jpg",
        CoverMime::Png => "png",
        CoverMime::Unknown => return Ok(()),
    };
    let parent = audio_path
        .parent()
        .ok_or_else(|| anyhow!("audio output has no parent directory"))?;
    let cover_path = parent.join(format!("cover.{ext}"));
    fs::write(&cover_path, &cover.data)
        .with_context(|| format!("failed to write {}", cover_path.display()))?;
    Ok(())
}

fn format_matches(filter: &[String], format: AudioFormat) -> bool {
    filter.iter().any(|f| AudioFormat::from_hint(f) == format)
}

fn build_output_path(
    input: &Path,
    args: &Cli,
    headers: &NcmHeaders,
    format: AudioFormat,
) -> Result<PathBuf> {
    let base = match &args.output {
        Some(dir) => dir.clone(),
        None => input
            .parent()
            .ok_or_else(|| anyhow!("{}", t().err_no_parent))?
            .to_path_buf(),
    };

    let ext = format.extension();
    let mut out = base;

    match &args.template {
        Some(tmpl) => {
            // Template drives naming: split on `/`\\` so the template can
            // carve subdirectories under the output base.
            let stem = template::render_stem(tmpl, &headers.metadata, format)?;
            for seg in stem.split(['/', '\\']).filter(|s| !s.is_empty()) {
                out.push(seg);
            }
        }
        None => {
            // Default: preserve the input file's stem verbatim, just switch
            // the extension to the detected audio format. No sanitization —
            // if the name is valid on disk going in, it's valid going out.
            let stem = input
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown");
            out.push(stem);
        }
    }

    // Folder mode: turn the last segment (so far acting as a filename stem)
    // into a directory and place the audio file with the same stem inside it.
    //   output/Foo.mp3            -> output/Foo/Foo.mp3
    //   output/Artist/Album/Song  -> output/Artist/Album/Song/Song
    if args.folder {
        if let Some(last) = out.file_name().map(|s| s.to_owned()) {
            out.push(&last);
        }
    }

    out.set_extension(ext);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{ColorChoice, ConflictStrategy};
    use crate::i18n::Lang;
    use ncm_core::{NcmHeaders, NcmMetadata};
    use std::io::Write;

    // ---- Test fixtures ---------------------------------------------------

    /// Build a minimal `Cli` for tests; callers tweak only the fields they
    /// care about, defaults match the CLI's "no flags passed" behavior.
    fn cli_default() -> Cli {
        Cli {
            input: None,
            from_file: None,
            output: None,
            template: None,
            recursive: false,
            format: Vec::new(),
            exclude: Vec::new(),
            limit: None,
            no_tag: false,
            folder: false,
            jobs: None,
            on_conflict: ConflictStrategy::Skip,
            dry_run: false,
            verbose: 0,
            color: ColorChoice::Auto,
            config_path: None,
            no_config: true,
            lang: Lang::En,
        }
    }

    fn fake_headers(format_hint: &str) -> NcmHeaders {
        // The decoder/parser code paths aren't exercised here; we just need
        // a populated `NcmHeaders` so `build_output_path` can rely on
        // `metadata.title` / `bitrate` / `duration` for template rendering.
        NcmHeaders {
            metadata: NcmMetadata {
                title: "song".into(),
                artists: vec!["Artist".into()],
                album: "Album".into(),
                declared_format: ncm_core::AudioFormat::from_hint(format_hint),
                bitrate: Some(320_000),
                duration: Some(234_000),
                album_pic_url: None,
            },
            cover: None,
            detected_format: ncm_core::AudioFormat::Unknown,
        }
    }

    // ---- has_ncm_extension -----------------------------------------------

    #[test]
    fn ncm_ext_lowercase() {
        assert!(has_ncm_extension(Path::new("song.ncm")));
    }

    #[test]
    fn ncm_ext_uppercase_and_mixed_case() {
        assert!(has_ncm_extension(Path::new("song.NCM")));
        assert!(has_ncm_extension(Path::new("song.NcM")));
    }

    #[test]
    fn ncm_ext_rejects_non_matching() {
        assert!(!has_ncm_extension(Path::new("song.mp3")));
        assert!(!has_ncm_extension(Path::new("song")));
        assert!(!has_ncm_extension(Path::new("song.ncm.bak")));
        assert!(!has_ncm_extension(Path::new(".ncm"))); // hidden file with no stem
    }

    // ---- format_matches --------------------------------------------------

    #[test]
    fn format_filter_basic_match() {
        assert!(format_matches(&["mp3".into()], AudioFormat::Mp3));
        assert!(!format_matches(&["mp3".into()], AudioFormat::Flac));
    }

    #[test]
    fn format_filter_case_insensitive() {
        assert!(format_matches(&["MP3".into()], AudioFormat::Mp3));
        assert!(format_matches(&["FlAc".into()], AudioFormat::Flac));
    }

    #[test]
    fn format_filter_aliases() {
        // Hint -> real format mapping is owned by AudioFormat::from_hint;
        // we check the user-facing aliases the CLI advertises.
        assert!(format_matches(&["aac".into()], AudioFormat::M4a));
        assert!(format_matches(&["mp4".into()], AudioFormat::M4a));
        assert!(format_matches(&["vorbis".into()], AudioFormat::Ogg));
    }

    #[test]
    fn format_filter_multiple() {
        let f: Vec<String> = vec!["mp3".into(), "flac".into()];
        assert!(format_matches(&f, AudioFormat::Mp3));
        assert!(format_matches(&f, AudioFormat::Flac));
        assert!(!format_matches(&f, AudioFormat::M4a));
    }

    // ---- build_excluder --------------------------------------------------

    #[test]
    fn excluder_single_pattern_matches_path() {
        let g = build_excluder(&["**/tmp/**".into()]).unwrap();
        assert!(g.is_match("/foo/tmp/file.ncm"));
        assert!(g.is_match("a/b/tmp/c.ncm"));
        assert!(!g.is_match("/foo/bar/file.ncm"));
    }

    #[test]
    fn excluder_multiple_patterns_combine_or() {
        let g = build_excluder(&["**/tmp/**".into(), "*.partial.ncm".into()]).unwrap();
        assert!(g.is_match("/x/tmp/y.ncm"));
        assert!(g.is_match("foo.partial.ncm"));
        assert!(!g.is_match("/x/normal.ncm"));
    }

    #[test]
    fn excluder_invalid_pattern_errors() {
        // Unmatched `[` is invalid glob syntax. Surface as Err with context.
        let r = build_excluder(&["[unclosed".into()]);
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("invalid --exclude"));
    }

    // ---- read_file_list --------------------------------------------------

    #[test]
    fn read_file_list_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("list.txt");
        std::fs::write(&p, "song1.ncm\nsong2.ncm\n").unwrap();
        let v = read_file_list(&p).unwrap();
        assert_eq!(
            v,
            vec![PathBuf::from("song1.ncm"), PathBuf::from("song2.ncm")]
        );
    }

    #[test]
    fn read_file_list_skips_comments_and_blank_lines() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("list.txt");
        let body = "\
# this is a comment
song1.ncm

   # indented comment
song2.ncm
";
        std::fs::write(&p, body).unwrap();
        let v = read_file_list(&p).unwrap();
        assert_eq!(
            v,
            vec![PathBuf::from("song1.ncm"), PathBuf::from("song2.ncm")]
        );
    }

    #[test]
    fn read_file_list_trims_whitespace() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("list.txt");
        std::fs::write(&p, "  song1.ncm  \n\tsong2.ncm\t\n").unwrap();
        let v = read_file_list(&p).unwrap();
        assert_eq!(
            v,
            vec![PathBuf::from("song1.ncm"), PathBuf::from("song2.ncm")]
        );
    }

    #[test]
    fn read_file_list_missing_file_errors() {
        let r = read_file_list(Path::new("/nonexistent/list.txt"));
        assert!(r.is_err());
    }

    // ---- resolve_conflict / find_free_rename ------------------------------

    #[test]
    fn resolve_conflict_writes_when_path_doesnt_exist() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("song.mp3");
        for strat in [
            ConflictStrategy::Skip,
            ConflictStrategy::Overwrite,
            ConflictStrategy::Rename,
        ] {
            let r = resolve_conflict(&p, strat).unwrap();
            assert!(matches!(r, ConflictOutcome::Write(ref w) if w == &p));
        }
    }

    #[test]
    fn resolve_conflict_skip_existing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("song.mp3");
        std::fs::write(&p, b"existing").unwrap();
        let r = resolve_conflict(&p, ConflictStrategy::Skip).unwrap();
        assert!(matches!(r, ConflictOutcome::Skip));
    }

    #[test]
    fn resolve_conflict_overwrite_returns_same_path() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("song.mp3");
        std::fs::write(&p, b"existing").unwrap();
        let r = resolve_conflict(&p, ConflictStrategy::Overwrite).unwrap();
        assert!(matches!(r, ConflictOutcome::Write(ref w) if w == &p));
    }

    #[test]
    fn resolve_conflict_rename_picks_first_free_index() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("song.mp3");
        std::fs::write(&p, b"existing").unwrap();
        let r = resolve_conflict(&p, ConflictStrategy::Rename).unwrap();
        match r {
            ConflictOutcome::Write(w) => {
                assert_eq!(w.file_name().unwrap().to_str().unwrap(), "song-1.mp3");
                assert_eq!(w.parent().unwrap(), dir.path());
            }
            ConflictOutcome::Skip => panic!("should have renamed, not skipped"),
        }
    }

    #[test]
    fn resolve_conflict_rename_skips_taken_indices() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("song.mp3");
        // Pre-occupy song.mp3, song-1.mp3, song-2.mp3. Expect song-3.mp3.
        for name in ["song.mp3", "song-1.mp3", "song-2.mp3"] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let r = resolve_conflict(&p, ConflictStrategy::Rename).unwrap();
        match r {
            ConflictOutcome::Write(w) => {
                assert_eq!(w.file_name().unwrap().to_str().unwrap(), "song-3.mp3");
            }
            ConflictOutcome::Skip => panic!("should have renamed"),
        }
    }

    // ---- build_output_path ----------------------------------------------
    //
    // The tricky function: template ON/OFF × folder mode ON/OFF, plus
    // the extension swap. The key invariants we want to hold:
    //   - default template = preserve input filename stem
    //   - explicit template = render against metadata
    //   - folder mode = wrap output in a same-named directory
    //   - the audio extension always matches the *passed-in* format,
    //     not whatever the input filename had

    #[test]
    fn build_output_default_keeps_input_stem_swaps_ext() {
        let mut args = cli_default();
        args.output = Some(PathBuf::from("/out"));
        let h = fake_headers("mp3");
        let p =
            build_output_path(Path::new("/in/My Song.ncm"), &args, &h, AudioFormat::Mp3).unwrap();
        assert_eq!(p, PathBuf::from("/out/My Song.mp3"));
    }

    #[test]
    fn build_output_default_no_output_uses_input_parent() {
        let args = cli_default();
        let h = fake_headers("flac");
        let p =
            build_output_path(Path::new("/music/foo.ncm"), &args, &h, AudioFormat::Flac).unwrap();
        assert_eq!(p, PathBuf::from("/music/foo.flac"));
    }

    #[test]
    fn build_output_template_drives_naming() {
        let mut args = cli_default();
        args.output = Some(PathBuf::from("/out"));
        args.template = Some("{artist}/{album}/{title}".into());
        let h = fake_headers("mp3");
        let p =
            build_output_path(Path::new("/in/whatever.ncm"), &args, &h, AudioFormat::Mp3).unwrap();
        assert_eq!(p, PathBuf::from("/out/Artist/Album/song.mp3"));
    }

    #[test]
    fn build_output_folder_mode_wraps_in_dir() {
        // No template + folder: input "Foo.ncm" -> /out/Foo/Foo.mp3
        let mut args = cli_default();
        args.output = Some(PathBuf::from("/out"));
        args.folder = true;
        let h = fake_headers("mp3");
        let p = build_output_path(Path::new("/in/Foo.ncm"), &args, &h, AudioFormat::Mp3).unwrap();
        assert_eq!(p, PathBuf::from("/out/Foo/Foo.mp3"));
    }

    #[test]
    fn build_output_template_plus_folder_combines() {
        // Template {artist}/{title} + folder:
        //   /out/Artist/song -> /out/Artist/song/song.mp3
        let mut args = cli_default();
        args.output = Some(PathBuf::from("/out"));
        args.template = Some("{artist}/{title}".into());
        args.folder = true;
        let h = fake_headers("mp3");
        let p =
            build_output_path(Path::new("/in/whatever.ncm"), &args, &h, AudioFormat::Mp3).unwrap();
        assert_eq!(p, PathBuf::from("/out/Artist/song/song.mp3"));
    }

    #[test]
    fn build_output_extension_follows_format_not_input() {
        // Even if the input is named .ncm, the output uses the audio format
        // we detected, not the input extension.
        let mut args = cli_default();
        args.output = Some(PathBuf::from("/out"));
        let h = fake_headers("mp3");
        let p = build_output_path(Path::new("/in/song.ncm"), &args, &h, AudioFormat::Flac).unwrap();
        assert_eq!(p, PathBuf::from("/out/song.flac"));
    }

    #[test]
    fn build_output_dot_ncm_pathological_input() {
        // Documents the (mildly quirky but harmless) behavior for inputs
        // named just `.ncm`: Rust's `Path::file_stem` returns the whole
        // string `.ncm` for hidden-file-style names, so the stem keeps
        // the leading dot and the extension swap appends rather than
        // replaces. Result is `.ncm.mp3` — weird but a valid file path.
        // The "unknown" fallback only triggers on truly empty stems
        // (rare in practice; mostly trailing-slash paths).
        let mut args = cli_default();
        args.output = Some(PathBuf::from("/out"));
        let h = fake_headers("mp3");
        let p = build_output_path(Path::new("/in/.ncm"), &args, &h, AudioFormat::Mp3).unwrap();
        assert_eq!(p.file_name().unwrap().to_str().unwrap(), ".ncm.mp3");
    }

    // ---- write_cover_next_to ---------------------------------------------

    #[test]
    fn write_cover_jpeg_lands_with_jpg_ext() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.mp3");
        std::fs::File::create(&audio)
            .unwrap()
            .write_all(b"fake mp3")
            .unwrap();
        let cover = Cover {
            mime: CoverMime::Jpeg,
            data: b"jpeg-bytes".to_vec(),
        };
        write_cover_next_to(&audio, &cover).unwrap();
        let cover_path = dir.path().join("cover.jpg");
        assert!(cover_path.exists());
        assert_eq!(std::fs::read(&cover_path).unwrap(), b"jpeg-bytes");
    }

    #[test]
    fn write_cover_unknown_mime_is_silently_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.mp3");
        std::fs::File::create(&audio).unwrap();
        let cover = Cover {
            mime: CoverMime::Unknown,
            data: b"bytes".to_vec(),
        };
        // Should NOT error, but should also NOT create a file.
        write_cover_next_to(&audio, &cover).unwrap();
        assert!(!dir.path().join("cover.bin").exists());
        assert!(!dir.path().join("cover.jpg").exists());
        assert!(!dir.path().join("cover.png").exists());
    }
}
