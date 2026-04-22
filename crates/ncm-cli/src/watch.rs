//! `watch` subcommand: monitor a directory and auto-decrypt `.ncm` files as
//! they appear or get modified.
//!
//! Uses [`notify-debouncer-mini`] so that editing-in-progress files (where
//! the writer triggers many `Modify` events in rapid succession) don't kick
//! off multiple parallel decryptions of the same file.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use console::style;
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};

use crate::cli::{Cli, WatchArgs};
use crate::i18n::t;
use crate::pipeline::{has_ncm_extension, process_one, Outcome};

pub fn run(args: &WatchArgs) -> Result<()> {
    if !args.dir.exists() {
        anyhow::bail!(
            "{}",
            t().msg_input_missing
                .replace("{path}", &args.dir.display().to_string())
        );
    }
    if !args.dir.is_dir() {
        anyhow::bail!(
            "watch target must be a directory, got: {}",
            args.dir.display()
        );
    }

    let (tx, rx) = mpsc::channel();
    let mut debouncer =
        new_debouncer(Duration::from_secs(1), tx).context("failed to create file watcher")?;

    debouncer
        .watcher()
        .watch(&args.dir, RecursiveMode::Recursive)
        .with_context(|| format!("failed to watch {}", args.dir.display()))?;

    eprintln!(
        "{}",
        t().msg_watching
            .replace("{path}", &args.dir.display().to_string())
    );

    // Convert WatchArgs into a Cli once so process_one can be called with
    // the same pipeline engine. Watch mode implicitly works recursively over
    // the watched tree and should never dry-run.
    let cli = watch_args_to_cli(args);

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                for event in events {
                    if event.kind != DebouncedEventKind::Any {
                        continue;
                    }
                    if should_process(&event.path) {
                        handle_file(&event.path, &cli);
                    }
                }
            }
            Ok(Err(e)) => {
                log::warn!("watch error: {e}");
            }
            // Sender closed (shouldn't happen unless the debouncer drops) —
            // exit the loop cleanly.
            Err(_) => break,
        }
    }

    eprintln!("{}", t().msg_watch_stopped);
    Ok(())
}

fn should_process(path: &Path) -> bool {
    path.is_file() && has_ncm_extension(path)
}

fn handle_file(path: &Path, cli: &Cli) {
    match process_one(path, cli) {
        Ok(Outcome::Written { path: out, .. }) => {
            eprintln!(
                "{} {} -> {}",
                style("✓").green().bold(),
                path.display(),
                out.display()
            );
        }
        Ok(Outcome::Skipped(reason)) => {
            eprintln!("{} {}  ({reason})", style("·").yellow(), path.display());
        }
        Ok(Outcome::DryRun(_)) => {
            // Watch mode doesn't set dry_run, so this shouldn't fire —
            // but keep the match exhaustive.
        }
        Err(e) => {
            eprintln!("{} {}: {e:#}", style("✗").red().bold(), path.display());
        }
    }
}

fn watch_args_to_cli(args: &WatchArgs) -> Cli {
    use crate::i18n::Lang;
    Cli {
        input: Some(PathBuf::new()), // unused in process_one, but avoid Option plumbing
        from_file: None,
        output: args.output.clone(),
        template: args.template.clone(),
        recursive: true,
        format: args.format.clone(),
        no_tag: args.no_tag,
        folder: args.folder,
        jobs: None,
        on_conflict: args.on_conflict,
        dry_run: false,
        verbose: 0,
        config_path: None,
        no_config: true,
        lang: Lang::En, // unused
    }
}
