mod cli;
mod config;
mod cover;
mod i18n;
mod info;
mod pipeline;
mod tagger;
mod template;
mod watch;

use std::time::Duration;

use anyhow::Result;
use ncm_core::AudioFormat;

use crate::cli::CliCommand;
use crate::i18n::{t, Lang};
use crate::pipeline::RunSummary;

fn main() -> Result<()> {
    // Pick the locale before clap runs so `--help` text is already localized.
    // Priority: explicit `--lang`/`-L` on argv > env vars > default English.
    let lang = prescan_lang().unwrap_or_else(Lang::detect_from_env);
    i18n::init(lang.strings());

    // Build the clap command once; we reuse the same instance both for
    // parsing argv and for generating shell completions.
    let mut cmd = cli::build_command(lang.strings());
    let matches = cmd.clone().get_matches();
    let command = cli::parse(&matches, lang)?;

    // Verbose level only affects the decrypt pipeline for now; info / cover
    // / completion modes print their own structured output.
    let verbose = match &command {
        CliCommand::Decrypt(args) => args.log_level(),
        CliCommand::Info(_)
        | CliCommand::Cover(_)
        | CliCommand::Watch(_)
        | CliCommand::Completion(_) => log::LevelFilter::Warn,
    };
    env_logger::Builder::new()
        .filter_level(verbose)
        .format_timestamp(None)
        .format_target(false)
        .init();

    match command {
        CliCommand::Completion(shell) => {
            clap_complete::generate(shell, &mut cmd, "ncm2mp3", &mut std::io::stdout());
        }
        CliCommand::Info(args) => info::run(&args)?,
        CliCommand::Cover(args) => cover::run(&args)?,
        CliCommand::Watch(args) => watch::run(&args)?,
        CliCommand::Decrypt(mut args) => {
            // Merge in config file settings. Command-line flags already in
            // `args` take priority; config fills in the gaps. `--no-config`
            // or `--config <path>` on the CLI decides which file to load.
            if !args.no_config {
                let file_cfg = if let Some(explicit) = args.config_path.clone() {
                    config::load_from(&explicit)?
                } else {
                    config::load_default_if_present()?
                };
                cli::merge_config(&mut args, &matches, &file_cfg);
            }

            let summary = pipeline::run(&args)?;
            print_summary(&summary);
            if summary.failed > 0 {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

/// Render the final summary line using current translation strings, with
/// elapsed wall-clock time and a per-format breakdown appended.
fn print_summary(summary: &RunSummary) {
    let s = t();
    let base = s
        .msg_summary
        .replace("{ok}", &summary.ok.to_string())
        .replace("{skipped}", &summary.skipped.to_string())
        .replace("{failed}", &summary.failed.to_string());

    let elapsed = s
        .msg_summary_elapsed
        .replace("{elapsed}", &format_elapsed(summary.elapsed));

    let breakdown = if summary.by_format.is_empty() {
        String::new()
    } else {
        s.msg_summary_breakdown
            .replace("{breakdown}", &format_breakdown(&summary.by_format))
    };

    eprintln!("\n{base}{elapsed}{breakdown}");
}

fn format_elapsed(d: Duration) -> String {
    let total = d.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}h{minutes:02}m{seconds:02}s")
    } else if minutes > 0 {
        format!("{minutes}m{seconds:02}s")
    } else {
        // For sub-minute runs, surface a hint of sub-second precision.
        let millis = d.subsec_millis();
        format!("{seconds}.{millis:03}s")
    }
}

fn format_breakdown(by_format: &std::collections::HashMap<AudioFormat, usize>) -> String {
    // Stable order: sort by format name so the line reads the same between
    // invocations for comparable outputs.
    let mut entries: Vec<(AudioFormat, usize)> = by_format.iter().map(|(f, n)| (*f, *n)).collect();
    entries.sort_by_key(|(f, _)| format_sort_key(*f));
    entries
        .into_iter()
        .map(|(f, n)| format!("{}:{}", format_display(f), n))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_sort_key(f: AudioFormat) -> u8 {
    match f {
        AudioFormat::Mp3 => 0,
        AudioFormat::Flac => 1,
        AudioFormat::M4a => 2,
        AudioFormat::Wav => 3,
        AudioFormat::Ogg => 4,
        AudioFormat::Unknown => 9,
    }
}

fn format_display(f: AudioFormat) -> &'static str {
    match f {
        AudioFormat::Mp3 => "MP3",
        AudioFormat::Flac => "FLAC",
        AudioFormat::M4a => "M4A",
        AudioFormat::Wav => "WAV",
        AudioFormat::Ogg => "OGG",
        AudioFormat::Unknown => "?",
    }
}

/// Minimal pre-clap scan of argv to pick up `--lang`/`-L`/`--lang=…` before
/// building the clap command. This lets us localize `--help` itself, which
/// clap otherwise freezes at the moment the command is constructed.
fn prescan_lang() -> Option<Lang> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(val) = arg.strip_prefix("--lang=") {
            return Lang::parse(val);
        }
        if arg == "--lang" || arg == "-L" {
            return args.next().and_then(|s| Lang::parse(&s));
        }
    }
    None
}
