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

use crate::cli::{CliCommand, ColorChoice};
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

    // Subcommand-typo correction. clap's built-in `suggestions` feature
    // catches typo'd flags (e.g. `--formatt` -> `--format`) and invalid
    // enum values, but a typo'd subcommand like `ncm2mp3 inf song.ncm`
    // gets silently consumed into the optional positional INPUT slot,
    // so clap never gets a chance to suggest. We catch that case here.
    suggest_subcommand_on_typo(&command, &cmd);

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
            apply_color_choice(args.color);
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

/// Resolve the user's `--color` preference (plus the `NO_COLOR` env var
/// convention) into a hard on/off call on the `console` crate's global
/// state. Called once from `main` before any styled output is produced.
fn apply_color_choice(choice: ColorChoice) {
    // The well-known NO_COLOR env var (https://no-color.org) — when set to
    // any non-empty value, user wants no color regardless of --color.
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        console::set_colors_enabled_stderr(false);
        console::set_colors_enabled(false);
        return;
    }
    match choice {
        ColorChoice::Always => {
            console::set_colors_enabled_stderr(true);
            console::set_colors_enabled(true);
        }
        ColorChoice::Never => {
            console::set_colors_enabled_stderr(false);
            console::set_colors_enabled(false);
        }
        ColorChoice::Auto => {
            // `console` defaults to TTY-based auto-detection; no override.
        }
    }
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

/// Catch the case where the user mistyped a subcommand name. clap's
/// `suggestions` feature handles flags and enum values automatically,
/// but a typo'd subcommand (e.g. `ncm2mp3 inf song.ncm`) gets silently
/// consumed by the optional positional INPUT slot. We detect that here:
/// if INPUT was set, it doesn't exist on disk, looks like a bare word
/// (no extension, no path separator), AND is close enough to a known
/// subcommand name, print a friendly suggestion and exit non-zero.
fn suggest_subcommand_on_typo(command: &CliCommand, cmd: &clap::Command) {
    let CliCommand::Decrypt(args) = command else {
        return; // a real subcommand was matched; nothing to second-guess
    };
    let Some(input) = args.input.as_deref() else {
        return; // no input provided; --help / --version etc.
    };
    if input.exists() {
        return; // user gave a real path
    }
    let raw = input.to_string_lossy();
    // Heuristic: "looks like a bare word, not a path"
    if raw.contains('/')
        || raw.contains('\\')
        || raw.contains('.')
        || raw.is_empty()
        || raw.len() > 32
    {
        return;
    }

    let names: Vec<&str> = cmd.get_subcommands().map(|s| s.get_name()).collect();
    let Some(suggestion) = closest_name(&raw, &names) else {
        return;
    };

    let s = t();
    eprintln!(
        "{}",
        s.err_unknown_subcommand
            .replace("{value}", &raw)
            .replace("{suggestion}", suggestion)
    );
    eprintln!("  {}", s.tip_subcommands);
    std::process::exit(2);
}

/// Pick the subcommand name closest to `typed`, but only if "close
/// enough" — Levenshtein distance ≤ 2 OR `typed` is a prefix of a
/// candidate. Returns None when nothing's close.
fn closest_name<'a>(typed: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let typed_lc = typed.to_ascii_lowercase();
    let mut best: Option<(&'a str, usize)> = None;
    for &cand in candidates {
        if cand.to_ascii_lowercase().starts_with(&typed_lc) && typed.len() >= 2 {
            return Some(cand); // prefix match wins outright
        }
        let dist = strsim::levenshtein(&typed_lc, &cand.to_ascii_lowercase());
        if dist <= 2 && best.map_or(true, |(_, d)| dist < d) {
            best = Some((cand, dist));
        }
    }
    best.map(|(name, _)| name)
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

#[cfg(test)]
mod tests {
    use super::closest_name;

    const SUBCMDS: &[&str] = &["info", "cover", "watch", "completion"];

    #[test]
    fn one_letter_swap() {
        // "inf" is one substitution away from "info" — within distance 2.
        assert_eq!(closest_name("inf", SUBCMDS), Some("info"));
        // "covar" -> "cover" is one substitution.
        assert_eq!(closest_name("covar", SUBCMDS), Some("cover"));
        // "wath" -> "watch" is one substitution.
        assert_eq!(closest_name("wath", SUBCMDS), Some("watch"));
    }

    #[test]
    fn prefix_wins_outright() {
        // "comp" is a prefix of "completion" — direct match before
        // distance scoring kicks in.
        assert_eq!(closest_name("comp", SUBCMDS), Some("completion"));
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(closest_name("INFO", SUBCMDS), Some("info"));
        assert_eq!(closest_name("Inf", SUBCMDS), Some("info"));
    }

    #[test]
    fn nothing_close_returns_none() {
        // "xyzzy" vs any subcommand has Levenshtein > 2.
        assert_eq!(closest_name("xyzzy", SUBCMDS), None);
        // Single letter — too ambiguous, no prefix kicks in.
        assert_eq!(closest_name("z", SUBCMDS), None);
    }

    #[test]
    fn picks_closest_when_multiple_within_threshold() {
        // "covor" is distance 2 from "cover" (1 sub) — wins.
        // "covor" is distance 4 from "completion" — way over.
        assert_eq!(closest_name("covor", SUBCMDS), Some("cover"));
    }
}
