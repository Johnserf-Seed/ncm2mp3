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
/// `--from-file <list>` entries, deduped.
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
    Ok(files)
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

enum Outcome {
    /// Decryption produced the audio file at `path` with the sniffed or
    /// declared `format`. Used to build the per-format breakdown.
    Written {
        path: PathBuf,
        format: AudioFormat,
    },
    DryRun(PathBuf),
    Skipped(String),
}

fn process_one(input: &Path, args: &Cli) -> Result<Outcome> {
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
