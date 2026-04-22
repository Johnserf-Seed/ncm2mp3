mod cli;
mod pipeline;
mod progress;
mod tagger;
mod template;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = cli::Cli::parse();

    env_logger::Builder::new()
        .filter_level(args.log_level())
        .format_timestamp(None)
        .format_target(false)
        .init();

    let summary = pipeline::run(&args)?;

    eprintln!(
        "\nDone: {} ok, {} skipped, {} failed",
        summary.ok, summary.skipped, summary.failed
    );

    if summary.failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}
