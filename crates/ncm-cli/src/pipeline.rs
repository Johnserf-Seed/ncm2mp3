//! Orchestrate the full decrypt + tag workflow for single files and
//! directories, with optional parallelism.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
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
use crate::output::OutputFile;
use crate::term::{printable, printable_path};
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
                        printable_path(input),
                        printable_path(&path)
                    );
                }
                Ok(Outcome::Skipped(reason)) => {
                    skipped.fetch_add(1, Ordering::Relaxed);
                    eprintln!(
                        "{} {}  ({})",
                        style("·").yellow(),
                        printable_path(input),
                        printable(&reason)
                    );
                }
                Ok(Outcome::DryRun(path)) => {
                    ok.fetch_add(1, Ordering::Relaxed);
                    eprintln!(
                        "{} {} -> {}",
                        style(t().msg_dry_run_prefix).cyan(),
                        printable_path(input),
                        printable_path(&path)
                    );
                }
                Err(e) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    eprintln!(
                        "{} {}: {}",
                        style("✗").red().bold(),
                        printable_path(input),
                        printable(&format!("{e:#}"))
                    );
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

    // `effective_format` would fall back to the metadata's hint and happily
    // name noise `.mp3`. If the decrypted head matches no audio signature,
    // fail before writing anything rather than reporting a bogus success.
    if headers.detected_format == AudioFormat::Unknown {
        bail!("{}", t().err_unrecognized_audio);
    }

    let format = headers.effective_format();

    if !args.format.is_empty() && !format_matches(&args.format, format) {
        return Ok(Outcome::Skipped(
            t().msg_skipped_fmt
                .replace("{fmt}", format.extension())
                .to_string(),
        ));
    }

    let planned = build_output_path(input, args, &headers, format)?;

    if args.dry_run {
        return Ok(match resolve_conflict(&planned, args.on_conflict)? {
            ConflictOutcome::Write(path) => Outcome::DryRun(path),
            ConflictOutcome::Skip => skipped_exists(&planned),
        });
    }

    if let Some(parent) = planned.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    // Claim the output name and open it in one step (see `output`): checking
    // first and creating later would let a symlink planted at that name, or
    // a parallel worker heading for the same name, slip in between.
    let Some(mut output) = open_output(&planned, args.on_conflict)? else {
        return Ok(skipped_exists(&planned));
    };
    let out_path = output.path().to_path_buf();

    {
        let mut writer = BufWriter::with_capacity(64 * 1024, output.file());
        decoder
            .decode_to_writer(&mut writer)
            .with_context(|| format!("failed to decrypt {}", input.display()))?;
        // Flush here: on drop, a failed final write would go unreported.
        writer
            .flush()
            .with_context(|| format!("failed to write {}", out_path.display()))?;
    }

    if !args.no_tag {
        let tagged = tagger::write_tags(
            output.file(),
            format,
            &headers.metadata,
            headers.cover.as_ref(),
        );
        if let Err(e) = tagged {
            log::warn!(
                "{}: {}",
                t().msg_tagging_failed
                    .replace("{path}", &printable_path(&out_path)),
                printable(&format!("{e:#}"))
            );
        }
    }

    output
        .commit()
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    if args.folder {
        if let Some(cover) = headers.cover.as_ref() {
            if let Err(e) = write_cover_next_to(&out_path, cover) {
                log::warn!(
                    "failed to write external cover next to {}: {}",
                    printable_path(&out_path),
                    printable(&format!("{e:#}"))
                );
            }
        }
    }

    Ok(Outcome::Written {
        path: out_path,
        format,
    })
}

fn skipped_exists(planned: &Path) -> Outcome {
    Outcome::Skipped(
        t().msg_skipped_exists
            .replace("{path}", &planned.display().to_string()),
    )
}

/// Create the output file for `planned` as the user's [`ConflictStrategy`]
/// says. `Ok(None)` means skip this input: the name is taken.
fn open_output(planned: &Path, strategy: ConflictStrategy) -> Result<Option<OutputFile>> {
    let create_new = |path: &Path| {
        OutputFile::create_new(path).with_context(|| format!("failed to create {}", path.display()))
    };
    match strategy {
        ConflictStrategy::Skip => create_new(planned),
        ConflictStrategy::Overwrite => OutputFile::replace(planned)
            .map(Some)
            .with_context(|| format!("failed to create {}", planned.display())),
        ConflictStrategy::Rename => {
            for candidate in
                std::iter::once(planned.to_path_buf()).chain(rename_candidates(planned))
            {
                if let Some(output) = create_new(&candidate)? {
                    return Ok(Some(output));
                }
            }
            Err(rename_exhausted(planned))
        }
    }
}

enum ConflictOutcome {
    /// Write to this (possibly-renamed) path.
    Write(PathBuf),
    /// Existing file means this input should be skipped entirely.
    Skip,
}

/// Predict where the user's [`ConflictStrategy`] would put `planned`, for
/// `--dry-run`. A real run doesn't use this: [`open_output`] claims the
/// name and creates the file in one step instead.
fn resolve_conflict(planned: &Path, strategy: ConflictStrategy) -> Result<ConflictOutcome> {
    if !is_taken(planned) {
        return Ok(ConflictOutcome::Write(planned.to_path_buf()));
    }
    match strategy {
        ConflictStrategy::Skip => Ok(ConflictOutcome::Skip),
        ConflictStrategy::Overwrite => Ok(ConflictOutcome::Write(planned.to_path_buf())),
        ConflictStrategy::Rename => rename_candidates(planned)
            .find(|candidate| !is_taken(candidate))
            .map(ConflictOutcome::Write)
            .ok_or_else(|| rename_exhausted(planned)),
    }
}

/// Whether anything is at `path`, a dangling symlink included: the same
/// test [`OutputFile::create_new`] applies.
fn is_taken(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// `foo-1.ext`, `foo-2.ext`, … up to a reasonable ceiling: the names
/// `--on-conflict rename` tries after `foo.ext` itself.
fn rename_candidates(planned: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    let parent = planned.parent().unwrap_or_else(|| Path::new(""));
    let stem = planned
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("output");
    let ext = planned.extension().and_then(OsStr::to_str).unwrap_or("");
    (1..=9999u32).map(move |n| {
        if ext.is_empty() {
            parent.join(format!("{stem}-{n}"))
        } else {
            parent.join(format!("{stem}-{n}.{ext}"))
        }
    })
}

fn rename_exhausted(planned: &Path) -> anyhow::Error {
    anyhow!(
        "{}",
        t().err_rename_exhausted
            .replace("{path}", &planned.display().to_string())
    )
}

/// Write a `Cover` as `cover.jpg` / `cover.png` in the same directory as
/// `audio_path`, replacing any cover already there.
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
    // Replace rather than write in place, so a symlink planted at
    // `cover.jpg` gets swapped out instead of written through.
    OutputFile::replace(&cover_path)
        .and_then(|output| output.write_and_commit(&cover.data))
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

    append_extension(&mut out, ext);
    Ok(out)
}

/// Append `.ext` to the last path component. `PathBuf::set_extension` would
/// treat a dot inside the stem as an existing extension and replace it,
/// turning `Mr. Brightside` into `Mr.mp3`.
fn append_extension(path: &mut PathBuf, ext: &str) {
    if let Some(name) = path.file_name() {
        let mut name = name.to_os_string();
        name.push(".");
        name.push(ext);
        path.set_file_name(name);
    }
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

    #[test]
    fn build_output_default_keeps_dots_in_input_stem() {
        // Issue #4: a dot inside the stem must not be mistaken for an
        // extension and replaced.
        let mut args = cli_default();
        args.output = Some(PathBuf::from("/out"));
        let h = fake_headers("flac");
        let p = build_output_path(
            Path::new("/in/artist - song name . xxx.ncm"),
            &args,
            &h,
            AudioFormat::Flac,
        )
        .unwrap();
        assert_eq!(p, PathBuf::from("/out/artist - song name . xxx.flac"));
    }

    #[test]
    fn build_output_template_keeps_dots_in_values() {
        let mut args = cli_default();
        args.output = Some(PathBuf::from("/out"));
        args.template = Some("{artist} - {title}".into());
        let mut h = fake_headers("mp3");
        h.metadata.title = "Mr. Brightside".into();
        let p =
            build_output_path(Path::new("/in/whatever.ncm"), &args, &h, AudioFormat::Mp3).unwrap();
        assert_eq!(p, PathBuf::from("/out/Artist - Mr. Brightside.mp3"));
    }

    #[test]
    fn build_output_folder_mode_keeps_dots() {
        let mut args = cli_default();
        args.output = Some(PathBuf::from("/out"));
        args.folder = true;
        let h = fake_headers("mp3");
        let p =
            build_output_path(Path::new("/in/Vol. 1.ncm"), &args, &h, AudioFormat::Mp3).unwrap();
        assert_eq!(p, PathBuf::from("/out/Vol. 1/Vol. 1.mp3"));
    }

    // ---- process_one -----------------------------------------------------

    use crate::testutil::write_ncm;

    const MP3: &[u8] = b"ID3\x04\x00\x00\x00\x00\x00\x00 some frames";

    #[test]
    fn process_one_writes_decrypted_audio_keeping_dotted_name() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("artist - song name . xxx.ncm");
        let audio = MP3;
        write_ncm(&input, audio, None);
        let mut args = cli_default();
        args.no_tag = true;

        match process_one(&input, &args).unwrap() {
            Outcome::Written { path, format } => {
                assert_eq!(path, dir.path().join("artist - song name . xxx.mp3"));
                assert_eq!(format, AudioFormat::Mp3);
                assert_eq!(std::fs::read(&path).unwrap(), audio);
            }
            _ => panic!("expected the file to be written"),
        }
    }

    #[test]
    fn process_one_fails_on_unrecognized_audio_without_writing() {
        // Issue #3: audio that decrypts to noise used to be written out and
        // counted as a success.
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("noise.ncm");
        write_ncm(
            &input,
            &[0x0b, 0x86, 0x8c, 0xed, 0x8e, 0x3a, 0x75, 0x45],
            None,
        );
        let out_dir = dir.path().join("out");
        let mut args = cli_default();
        args.output = Some(out_dir.clone());

        let err = process_one(&input, &args)
            .err()
            .expect("unrecognized audio must be an error");
        assert_eq!(err.to_string(), t().err_unrecognized_audio);
        assert!(!out_dir.exists(), "nothing should be written");
    }

    /// `song.ncm` in a temp dir, with `audio` inside, plus args that write
    /// to `<dir>/out` under `strategy`, untagged.
    fn song_and_args(
        audio: &[u8],
        strategy: ConflictStrategy,
    ) -> (tempfile::TempDir, PathBuf, PathBuf, Cli) {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("song.ncm");
        write_ncm(&input, audio, None);
        let out_dir = dir.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();
        let mut args = cli_default();
        args.output = Some(out_dir.clone());
        args.on_conflict = strategy;
        args.no_tag = true;
        (dir, input, out_dir, args)
    }

    #[test]
    fn process_one_skips_a_name_another_worker_is_still_writing() {
        // The race behind parallel inputs that map to one output name: the
        // first worker's file exists but isn't finished. An `exists()` check
        // made before it appeared would let both write.
        let (_dir, input, out_dir, args) = song_and_args(MP3, ConflictStrategy::Skip);
        let planned = out_dir.join("song.mp3");
        let _other_worker = OutputFile::create_new(&planned).unwrap().unwrap();

        assert!(matches!(
            process_one(&input, &args).unwrap(),
            Outcome::Skipped(_)
        ));
        assert_eq!(std::fs::read(&planned).unwrap(), b"");
    }

    #[test]
    fn process_one_renames_around_a_name_another_worker_is_still_writing() {
        let (_dir, input, out_dir, args) = song_and_args(MP3, ConflictStrategy::Rename);
        let _other_worker = OutputFile::create_new(&out_dir.join("song.mp3"))
            .unwrap()
            .unwrap();

        match process_one(&input, &args).unwrap() {
            Outcome::Written { path, .. } => {
                assert_eq!(path, out_dir.join("song-1.mp3"));
                assert_eq!(std::fs::read(&path).unwrap(), MP3);
            }
            _ => panic!("expected song-1.mp3 to be written"),
        }
    }

    #[test]
    fn process_one_overwrite_replaces_the_existing_output() {
        let (_dir, input, out_dir, args) = song_and_args(MP3, ConflictStrategy::Overwrite);
        let planned = out_dir.join("song.mp3");
        std::fs::write(&planned, b"stale").unwrap();

        assert!(matches!(
            process_one(&input, &args).unwrap(),
            Outcome::Written { .. }
        ));
        assert_eq!(std::fs::read(&planned).unwrap(), MP3);
        assert_eq!(
            std::fs::read_dir(&out_dir).unwrap().count(),
            1,
            "no temporary file left behind"
        );
    }

    #[test]
    fn process_one_tags_the_output_through_its_open_handle() {
        // Smallest FLAC lofty accepts: the magic plus a STREAMINFO block
        // (44.1 kHz, stereo, 16-bit).
        let mut flac = b"fLaC\x80\x00\x00\x22".to_vec();
        flac.extend_from_slice(&[0x10, 0x00, 0x10, 0x00, 0, 0, 0, 0, 0, 0]);
        flac.extend_from_slice(&[0x0A, 0xC4, 0x42, 0xF0, 0, 0, 0, 0]);
        flac.extend_from_slice(&[0u8; 16]);

        let (_dir, input, out_dir, mut args) = song_and_args(&flac, ConflictStrategy::Skip);
        args.no_tag = false;
        // Same input again, now with a cover for the tagger to embed.
        let cover = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F'];
        write_ncm(&input, &flac, Some(&cover));

        let Outcome::Written { path, .. } = process_one(&input, &args).unwrap() else {
            panic!("expected the file to be written");
        };
        assert_eq!(path, out_dir.join("song.flac"));

        use lofty::file::TaggedFileExt;
        let tagged = lofty::read_from_path(&path).unwrap();
        let tag = tagged.primary_tag().expect("tags were written");
        assert_eq!(tag.pictures().len(), 1);
        assert_eq!(tag.pictures()[0].data(), &cover[..]);
    }

    #[cfg(unix)]
    mod symlinks {
        use super::*;
        use std::os::unix::fs::symlink;

        /// Plant `out/song.mp3` -> `victim` holding "precious".
        fn plant(out_dir: &Path) -> (PathBuf, PathBuf) {
            let victim = out_dir.parent().unwrap().join("victim");
            std::fs::write(&victim, b"precious").unwrap();
            let link = out_dir.join("song.mp3");
            symlink(&victim, &link).unwrap();
            (link, victim)
        }

        #[test]
        fn skip_does_not_write_through_a_planted_symlink() {
            let (_dir, input, out_dir, args) = song_and_args(MP3, ConflictStrategy::Skip);
            let (_link, victim) = plant(&out_dir);

            assert!(matches!(
                process_one(&input, &args).unwrap(),
                Outcome::Skipped(_)
            ));
            assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
        }

        #[test]
        fn skip_does_not_create_a_dangling_symlinks_target() {
            let (dir, input, out_dir, args) = song_and_args(MP3, ConflictStrategy::Skip);
            let target = dir.path().join("planted");
            symlink(&target, out_dir.join("song.mp3")).unwrap();

            assert!(matches!(
                process_one(&input, &args).unwrap(),
                Outcome::Skipped(_)
            ));
            assert!(!target.exists());
        }

        #[test]
        fn rename_does_not_write_through_a_planted_symlink() {
            let (_dir, input, out_dir, args) = song_and_args(MP3, ConflictStrategy::Rename);
            let (_link, victim) = plant(&out_dir);

            let Outcome::Written { path, .. } = process_one(&input, &args).unwrap() else {
                panic!("expected song-1.mp3 to be written");
            };
            assert_eq!(path, out_dir.join("song-1.mp3"));
            assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
        }

        #[test]
        fn overwrite_replaces_a_planted_symlink_instead_of_following_it() {
            let (_dir, input, out_dir, args) = song_and_args(MP3, ConflictStrategy::Overwrite);
            let (link, victim) = plant(&out_dir);

            assert!(matches!(
                process_one(&input, &args).unwrap(),
                Outcome::Written { .. }
            ));
            assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
            assert!(std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_file());
            assert_eq!(std::fs::read(&link).unwrap(), MP3);
        }

        #[test]
        fn folder_cover_does_not_write_through_a_planted_symlink() {
            let dir = tempfile::tempdir().unwrap();
            let audio = dir.path().join("song.mp3");
            std::fs::write(&audio, b"fake mp3").unwrap();
            let victim = dir.path().join("victim");
            std::fs::write(&victim, b"precious").unwrap();
            let link = dir.path().join("cover.jpg");
            symlink(&victim, &link).unwrap();

            let cover = Cover {
                mime: CoverMime::Jpeg,
                data: b"jpeg-bytes".to_vec(),
            };
            write_cover_next_to(&audio, &cover).unwrap();

            assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
            assert_eq!(std::fs::read(&link).unwrap(), b"jpeg-bytes");
        }

        #[test]
        fn dry_run_treats_a_dangling_symlink_as_taken() {
            let dir = tempfile::tempdir().unwrap();
            let link = dir.path().join("song.mp3");
            symlink(dir.path().join("nowhere"), &link).unwrap();
            assert!(matches!(
                resolve_conflict(&link, ConflictStrategy::Skip).unwrap(),
                ConflictOutcome::Skip
            ));
        }
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
