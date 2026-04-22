mod cli;
mod i18n;
mod info;
mod pipeline;
mod tagger;
mod template;

use anyhow::Result;

use crate::cli::CliCommand;
use crate::i18n::{t, Lang};

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

    // Verbose level only affects the decrypt pipeline for now; info and
    // completion modes print their own structured output.
    let verbose = match &command {
        CliCommand::Decrypt(args) => args.log_level(),
        CliCommand::Info(_) | CliCommand::Completion(_) => log::LevelFilter::Warn,
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
        CliCommand::Decrypt(args) => {
            let summary = pipeline::run(&args)?;

            let s = t();
            eprintln!(
                "\n{}",
                s.msg_summary
                    .replace("{ok}", &summary.ok.to_string())
                    .replace("{skipped}", &summary.skipped.to_string())
                    .replace("{failed}", &summary.failed.to_string())
            );

            if summary.failed > 0 {
                std::process::exit(1);
            }
        }
    }

    Ok(())
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
