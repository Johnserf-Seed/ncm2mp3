use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use clap_complete::Shell;

use crate::i18n::{Lang, Strings};

/// Top-level dispatch: either the default "decrypt" flow (no subcommand),
/// or a specialized mode invoked via subcommand.
pub enum CliCommand {
    Decrypt(Cli),
    Info(InfoArgs),
    Cover(CoverArgs),
    Watch(WatchArgs),
    Completion(Shell),
}

/// Whether to emit ANSI color codes in per-file status lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    /// Emit color when stderr is a TTY; skip it when redirected (default).
    Auto,
    /// Always emit color, even if stderr is redirected.
    Always,
    /// Never emit color.
    Never,
}

impl ColorChoice {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

/// What to do when the output file already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    /// Leave the existing file alone and count it as skipped (default).
    Skip,
    /// Overwrite the existing file.
    Overwrite,
    /// Append `-1`, `-2`, … to the stem until a free path is found.
    Rename,
}

impl ConflictStrategy {
    /// Parse the string coming from `--on-conflict`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "skip" => Some(Self::Skip),
            "overwrite" => Some(Self::Overwrite),
            "rename" => Some(Self::Rename),
            _ => None,
        }
    }
}

/// Decrypt-mode arguments, flat struct preserved across the pipeline.
#[derive(Debug, Clone)]
pub struct Cli {
    /// Positional `<INPUT>`. May be `None` when the user passes only
    /// `--from-file <list>`.
    pub input: Option<PathBuf>,
    /// Additional input sources read from a text file, one path per line
    /// (lines starting with `#` and blank lines are ignored).
    pub from_file: Option<PathBuf>,
    pub output: Option<PathBuf>,
    /// When `None`, the pipeline preserves the input file's stem verbatim and
    /// only swaps the extension. A template is only applied when the user
    /// explicitly passes `--template`.
    pub template: Option<String>,
    pub recursive: bool,
    pub format: Vec<String>,
    /// Glob patterns for files to skip during directory collection
    /// (matched against the absolute path as a string, using `globset`).
    pub exclude: Vec<String>,
    /// Optional cap on how many files actually get processed. Useful for
    /// smoke-testing a large library with a handful of files first.
    pub limit: Option<usize>,
    pub no_tag: bool,
    pub folder: bool,
    pub jobs: Option<usize>,
    /// What to do when an output file already exists.
    pub on_conflict: ConflictStrategy,
    pub dry_run: bool,
    pub verbose: u8,
    pub color: ColorChoice,
    /// Explicit override of config file location; when `None`, the default
    /// platform path is consulted.
    pub config_path: Option<PathBuf>,
    /// `--no-config` skips config-file loading entirely.
    pub no_config: bool,
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

/// `cover` subcommand args: extract the embedded cover image without
/// decrypting the audio.
#[derive(Debug, Clone)]
pub struct CoverArgs {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub recursive: bool,
    pub overwrite: bool,
}

/// `watch` subcommand args: watch a directory and auto-decrypt new NCM
/// files as they appear. Reuses most of the decrypt pipeline options.
#[derive(Debug, Clone)]
pub struct WatchArgs {
    /// The directory to watch.
    pub dir: PathBuf,
    pub output: Option<PathBuf>,
    pub template: Option<String>,
    pub format: Vec<String>,
    pub no_tag: bool,
    pub folder: bool,
    pub on_conflict: ConflictStrategy,
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
                // INPUT becomes optional when --from-file supplies the list;
                // clap's `required_unless_present` handles "either-or" cleanly.
                .required_unless_present("from-file")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("from-file")
                .long("from-file")
                .value_name(s.val_path)
                .help(s.arg_from_file)
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
            Arg::new("exclude")
                .long("exclude")
                .value_name(s.val_pattern)
                .help(s.arg_exclude)
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("limit")
                .long("limit")
                .value_name(s.val_n)
                .help(s.arg_limit)
                .value_parser(clap::value_parser!(usize)),
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
            Arg::new("on-conflict")
                .long("on-conflict")
                .value_name(s.val_conflict)
                .help(s.arg_on_conflict)
                .value_parser(["skip", "overwrite", "rename"])
                .conflicts_with("overwrite"),
        )
        .arg(
            Arg::new("overwrite")
                .long("overwrite")
                .help(s.arg_overwrite)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .value_name(s.val_path)
                .help(s.arg_config)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("no-config")
                .long("no-config")
                .help(s.arg_no_config)
                .action(ArgAction::SetTrue)
                .conflicts_with("config"),
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
            Arg::new("color")
                .long("color")
                .value_name(s.val_color)
                .help(s.arg_color)
                .value_parser(["auto", "always", "never"])
                .default_value("auto"),
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
        .subcommand(
            Command::new("cover")
                .about(s.cmd_cover_about)
                .arg(
                    Arg::new("input")
                        .value_name(s.val_input)
                        .help(s.arg_cover_input)
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
                    Arg::new("recursive")
                        .short('r')
                        .long("recursive")
                        .help(s.arg_recursive)
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("overwrite")
                        .long("overwrite")
                        .help(s.arg_overwrite)
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("watch")
                .about(s.cmd_watch_about)
                .arg(
                    Arg::new("dir")
                        .value_name(s.val_dir)
                        .help(s.arg_watch_dir)
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
                    Arg::new("on-conflict")
                        .long("on-conflict")
                        .value_name(s.val_conflict)
                        .help(s.arg_on_conflict)
                        .value_parser(["skip", "overwrite", "rename"]),
                ),
        )
        .subcommand(
            Command::new("completion")
                .about(s.cmd_completion_about)
                .arg(
                    Arg::new("shell")
                        .value_name(s.val_shell)
                        .help(s.arg_completion_shell)
                        .required(true)
                        .value_parser(clap::builder::EnumValueParser::<Shell>::new()),
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
        Some(("cover", sub)) => Ok(CliCommand::Cover(CoverArgs {
            input: sub
                .get_one::<PathBuf>("input")
                .cloned()
                .ok_or_else(|| anyhow!("cover input is required"))?,
            output: sub.get_one::<PathBuf>("output").cloned(),
            recursive: sub.get_flag("recursive"),
            overwrite: sub.get_flag("overwrite"),
        })),
        Some(("watch", sub)) => {
            let format = sub
                .get_many::<String>("format")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();
            let on_conflict = sub
                .get_one::<String>("on-conflict")
                .and_then(|s| ConflictStrategy::parse(s))
                .unwrap_or(ConflictStrategy::Skip);
            Ok(CliCommand::Watch(WatchArgs {
                dir: sub
                    .get_one::<PathBuf>("dir")
                    .cloned()
                    .ok_or_else(|| anyhow!("watch directory is required"))?,
                output: sub.get_one::<PathBuf>("output").cloned(),
                template: sub.get_one::<String>("template").cloned(),
                format,
                no_tag: sub.get_flag("no-tag"),
                folder: sub.get_flag("folder"),
                on_conflict,
            }))
        }
        Some(("completion", sub)) => {
            let shell = sub
                .get_one::<Shell>("shell")
                .copied()
                .ok_or_else(|| anyhow!("completion shell is required"))?;
            Ok(CliCommand::Completion(shell))
        }
        _ => Ok(CliCommand::Decrypt(cli_from_matches(matches, lang))),
    }
}

/// Convert matched args into the flat `Cli` struct used by the pipeline.
fn cli_from_matches(matches: &ArgMatches, lang: Lang) -> Cli {
    let format = matches
        .get_many::<String>("format")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    // Conflict strategy: `--on-conflict` wins when set; otherwise the legacy
    // `--overwrite` flag maps to `Overwrite`; otherwise default `Skip`.
    let on_conflict = matches
        .get_one::<String>("on-conflict")
        .and_then(|s| ConflictStrategy::parse(s))
        .unwrap_or_else(|| {
            if matches.get_flag("overwrite") {
                ConflictStrategy::Overwrite
            } else {
                ConflictStrategy::Skip
            }
        });

    let exclude = matches
        .get_many::<String>("exclude")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    Cli {
        input: matches.get_one::<PathBuf>("input").cloned(),
        from_file: matches.get_one::<PathBuf>("from-file").cloned(),
        output: matches.get_one::<PathBuf>("output").cloned(),
        template: matches.get_one::<String>("template").cloned(),
        recursive: matches.get_flag("recursive"),
        format,
        exclude,
        limit: matches.get_one::<usize>("limit").copied(),
        no_tag: matches.get_flag("no-tag"),
        folder: matches.get_flag("folder"),
        jobs: matches.get_one::<usize>("jobs").copied(),
        on_conflict,
        dry_run: matches.get_flag("dry-run"),
        verbose: matches.get_count("verbose"),
        color: matches
            .get_one::<String>("color")
            .and_then(|s| ColorChoice::parse(s))
            .unwrap_or(ColorChoice::Auto),
        config_path: matches.get_one::<PathBuf>("config").cloned(),
        no_config: matches.get_flag("no-config"),
        lang,
    }
}

/// Merge a [`FileConfig`] into an existing [`Cli`], respecting the
/// **command line > config file > default** precedence. Only fields that
/// weren't already set on the command line are filled in.
///
/// For clap boolean flags we can't tell "omitted" from "explicitly set to
/// false" — we use [`ArgMatches::value_source`] to distinguish. When a flag
/// is [`clap::parser::ValueSource::DefaultValue`] (i.e. the user did NOT
/// pass it), the config value wins.
pub fn merge_config(cli: &mut Cli, matches: &ArgMatches, config: &crate::config::FileConfig) {
    use clap::parser::ValueSource::CommandLine;

    fn cli_provided(matches: &ArgMatches, id: &str) -> bool {
        matches
            .value_source(id)
            .map(|s| s == CommandLine)
            .unwrap_or(false)
    }

    if !cli_provided(matches, "template") {
        if let Some(t) = &config.template {
            cli.template = Some(t.clone());
        }
    }
    if !cli_provided(matches, "output") {
        if let Some(o) = &config.output {
            cli.output = Some(o.clone());
        }
    }
    if !cli_provided(matches, "jobs") {
        if let Some(j) = config.jobs {
            cli.jobs = Some(j);
        }
    }
    if !cli_provided(matches, "folder") {
        if let Some(true) = config.folder {
            cli.folder = true;
        }
    }
    if !cli_provided(matches, "no-tag") {
        if let Some(true) = config.no_tag {
            cli.no_tag = true;
        }
    }
    if !cli_provided(matches, "recursive") {
        if let Some(true) = config.recursive {
            cli.recursive = true;
        }
    }
    if !cli_provided(matches, "on-conflict") && !cli_provided(matches, "overwrite") {
        if let Some(raw) = &config.on_conflict {
            if let Some(strategy) = ConflictStrategy::parse(raw) {
                cli.on_conflict = strategy;
            }
        }
    }
    if !cli_provided(matches, "format") {
        if let Some(fmts) = &config.format {
            cli.format = fmts.clone();
        }
    }
}
