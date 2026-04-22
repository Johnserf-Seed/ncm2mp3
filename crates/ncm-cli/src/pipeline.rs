//! Orchestrate the full decrypt + tag workflow for single files and
//! directories, with optional parallelism.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use ncm_core::{AudioFormat, NcmDecoder, NcmHeaders};
use rayon::prelude::*;
use walkdir::WalkDir;

use crate::cli::Cli;
use crate::progress::{build_bar, build_multi};
use crate::{tagger, template};

#[derive(Debug)]
pub struct RunSummary {
    pub ok: usize,
    pub skipped: usize,
    pub failed: usize,
}

pub fn run(args: &Cli) -> Result<RunSummary> {
    let files = collect_inputs(&args.input, args.recursive)?;
    if files.is_empty() {
        log::warn!("no .ncm files found under {}", args.input.display());
        return Ok(RunSummary {
            ok: 0,
            skipped: 0,
            failed: 0,
        });
    }

    log::info!("queued {} file(s) for processing", files.len());

    let multi = build_multi();
    let pb = build_bar(&multi, files.len() as u64);

    let ok = Arc::new(AtomicUsize::new(0));
    let skipped = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));

    let pool = thread_pool(args.jobs)?;

    pool.install(|| {
        files.par_iter().for_each(|input| {
            pb.set_message(input.display().to_string());
            match process_one(input, args) {
                Ok(Outcome::Written(path)) => {
                    ok.fetch_add(1, Ordering::Relaxed);
                    log::info!("{} -> {}", input.display(), path.display());
                }
                Ok(Outcome::Skipped(reason)) => {
                    skipped.fetch_add(1, Ordering::Relaxed);
                    log::info!("skipped {}: {reason}", input.display());
                }
                Ok(Outcome::DryRun(path)) => {
                    ok.fetch_add(1, Ordering::Relaxed);
                    println!("[dry-run] {} -> {}", input.display(), path.display());
                }
                Err(e) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    log::error!("{}: {e:#}", input.display());
                }
            }
            pb.inc(1);
        });
    });

    pb.finish_with_message("done");

    Ok(RunSummary {
        ok: ok.load(Ordering::Relaxed),
        skipped: skipped.load(Ordering::Relaxed),
        failed: failed.load(Ordering::Relaxed),
    })
}

fn thread_pool(jobs: Option<usize>) -> Result<rayon::ThreadPool> {
    let mut builder = rayon::ThreadPoolBuilder::new();
    if let Some(n) = jobs {
        if n == 0 {
            bail!("--jobs must be >= 1");
        }
        builder = builder.num_threads(n);
    }
    builder
        .build()
        .context("failed to build rayon thread pool")
}

fn collect_inputs(input: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    if !input.exists() {
        bail!("input path does not exist: {}", input.display());
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

fn has_ncm_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("ncm"))
        .unwrap_or(false)
}

enum Outcome {
    Written(PathBuf),
    DryRun(PathBuf),
    Skipped(String),
}

fn process_one(input: &Path, args: &Cli) -> Result<Outcome> {
    let (mut decoder, headers) = NcmDecoder::open(input)
        .with_context(|| format!("failed to parse {}", input.display()))?;

    let format = headers.effective_format();

    if !args.format.is_empty() && !format_matches(&args.format, format) {
        return Ok(Outcome::Skipped(format!(
            "internal format {:?} not in --format filter",
            format
        )));
    }

    let out_path = build_output_path(input, args, &headers, format)?;

    if out_path.exists() && !args.overwrite {
        return Ok(Outcome::Skipped(format!(
            "output exists: {}",
            out_path.display()
        )));
    }

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
                "tagging failed for {} (file still decrypted): {e:#}",
                out_path.display()
            );
        }
    }

    Ok(Outcome::Written(out_path))
}

fn format_matches(filter: &[String], format: AudioFormat) -> bool {
    filter
        .iter()
        .any(|f| AudioFormat::from_hint(f) == format)
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
            .ok_or_else(|| anyhow!("input has no parent directory"))?
            .to_path_buf(),
    };

    let stem = template::render_stem(&args.template, &headers.metadata, format)?;
    let ext = format.extension();

    // Append the template stem as relative segments so embedded `/` in the
    // template becomes real subdirectories.
    let mut out = base;
    for seg in stem.split(['/', '\\']).filter(|s| !s.is_empty()) {
        out.push(seg);
    }
    out.set_extension(ext);
    Ok(out)
}
