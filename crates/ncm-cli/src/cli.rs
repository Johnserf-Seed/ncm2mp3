use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "ncm2mp3",
    version,
    about = "Decrypt Netease Cloud Music NCM files to their original audio format",
    long_about = None,
)]
pub struct Cli {
    /// Input .ncm file or a directory of .ncm files
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    /// Output directory (defaults to the input's parent directory)
    #[arg(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Filename template. Available placeholders:
    /// {artist} {album} {title} {format} {bitrate}.
    /// Forward slashes create subdirectories; the extension is appended
    /// automatically based on the detected audio format.
    #[arg(short, long, default_value = "{title}", value_name = "TEMPLATE")]
    pub template: String,

    /// Recurse into subdirectories when INPUT is a directory
    #[arg(short, long)]
    pub recursive: bool,

    /// Only process files whose internal format matches (e.g. mp3,flac).
    /// Can be repeated or comma-separated.
    #[arg(long, value_delimiter = ',', value_name = "FMT")]
    pub format: Vec<String>,

    /// Do not write ID3/Vorbis tags or embed cover art
    #[arg(long)]
    pub no_tag: bool,

    /// Number of parallel workers (default: number of CPU cores)
    #[arg(short = 'j', long, value_name = "N")]
    pub jobs: Option<usize>,

    /// Overwrite existing output files instead of skipping them
    #[arg(long)]
    pub overwrite: bool,

    /// Print what would be done without writing any output files
    #[arg(long)]
    pub dry_run: bool,

    /// Increase log verbosity (-v for info, -vv for debug)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
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
