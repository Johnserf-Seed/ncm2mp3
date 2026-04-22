use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::i18n::{Lang, Strings};

/// Top-level dispatch: either the default "decrypt" flow (no subcommand),
/// or a specialized mode invoked via subcommand.
pub enum CliCommand {
    Decrypt(Cli),
    Info(InfoArgs),
}

/// Decrypt-mode arguments, flat struct preserved across the pipeline.
#[derive(Debug, Clone)]
pub struct Cli {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    /// When `None`, the pipeline preserves the input file's stem verbatim and
    /// only swaps the extension. A template is only applied when the user
    /// explicitly passes `--template`.
    pub template: Option<String>,
    pub recursive: bool,
    pub format: Vec<String>,
    pub no_tag: bool,
    pub folder: bool,
    pub jobs: Option<usize>,
    pub overwrite: bool,
    pub dry_run: bool,
    pub verbose: u8,
    /// Retained for diagnostics and future locale-aware logic.
    #[allow(dead_code)]
    pub lang: Lang,
}

impl Cli {
    pub fn log_level(&self) -> log::LevelFilter {
        match self.verbose {
            0 => log::LevelFilter::Warn,
            1 => log::LevelFilter::Info,
            _ => log::LevelFilter::Debug,
        }
    }
}

/// `info` subcommand args: inspect headers without decrypting audio.
#[derive(Debug, Clone)]
pub struct InfoArgs {
    pub input: PathBuf,
    pub recursive: bool,
}

/// Build the clap `Command` with help text from the chosen translation table.
///
/// We use the builder API instead of `#[derive(Parser)]` because derive
/// attributes require `&'static str` literals, which means the help text is
/// baked in at compile time. The builder lets us swap tables at runtime based
/// on locale detection.
pub fn build_command(s: &'static Strings) -> Command {
    Command::new("ncm2mp3")
        .version(env!("CARGO_PKG_VERSION"))
        .about(s.cli_about)
        // Allow top-level positional INPUT to be skipped when a subcommand is
        // invoked (e.g. `ncm2mp3 info song.ncm` doesn't need top-level INPUT).
        .subcommand_negates_reqs(true)
        .arg(
            Arg::new("input")
                .value_name(s.val_input)
                .help(s.arg_input)
                .required(true)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name(s.val_dir)
                .help(s.arg_output)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("template")
                .short('t')
                .long("template")
                .value_name(s.val_template)
                .help(s.arg_template),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .help(s.arg_recursive)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .value_name(s.val_fmt)
                .value_delimiter(',')
                .help(s.arg_format)
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("no-tag")
                .long("no-tag")
                .help(s.arg_no_tag)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("folder")
                .short('F')
                .long("folder")
                .help(s.arg_folder)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("jobs")
                .short('j')
                .long("jobs")
                .value_name(s.val_n)
                .help(s.arg_jobs)
                .value_parser(clap::value_parser!(usize)),
        )
        .arg(
            Arg::new("overwrite")
                .long("overwrite")
                .help(s.arg_overwrite)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help(s.arg_dry_run)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help(s.arg_verbose)
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("lang")
                .short('L')
                .long("lang")
                .value_name(s.val_lang)
                .help(s.arg_lang)
                .env("NCM2MP3_LANG")
                .value_parser(["en", "zh"]),
        )
        .subcommand(
            Command::new("info")
                .about(s.cmd_info_about)
                .arg(
                    Arg::new("input")
                        .value_name(s.val_input)
                        .help(s.arg_info_input)
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf)),
                )
                .arg(
                    Arg::new("recursive")
                        .short('r')
                        .long("recursive")
                        .help(s.arg_recursive)
                        .action(ArgAction::SetTrue),
                ),
        )
}

/// Dispatch parsed matches into the concrete command variant.
pub fn parse(matches: &ArgMatches, lang: Lang) -> Result<CliCommand> {
    match matches.subcommand() {
        Some(("info", sub)) => Ok(CliCommand::Info(InfoArgs {
            input: sub
                .get_one::<PathBuf>("input")
                .cloned()
                .ok_or_else(|| anyhow!("info input is required"))?,
            recursive: sub.get_flag("recursive"),
        })),
        _ => Ok(CliCommand::Decrypt(cli_from_matches(matches, lang))),
    }
}

/// Convert matched args into the flat `Cli` struct used by the pipeline.
fn cli_from_matches(matches: &ArgMatches, lang: Lang) -> Cli {
    let format = matches
        .get_many::<String>("format")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    Cli {
        input: matches
            .get_one::<PathBuf>("input")
            .cloned()
            .expect("input is required"),
        output: matches.get_one::<PathBuf>("output").cloned(),
        template: matches.get_one::<String>("template").cloned(),
        recursive: matches.get_flag("recursive"),
        format,
        no_tag: matches.get_flag("no-tag"),
        folder: matches.get_flag("folder"),
        jobs: matches.get_one::<usize>("jobs").copied(),
        overwrite: matches.get_flag("overwrite"),
        dry_run: matches.get_flag("dry-run"),
        verbose: matches.get_count("verbose"),
        lang,
    }
}
