// CLI argument definitions using clap

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "oss")]
#[command(author = "hatlonely <hatlonely@foxmail.com>")]
#[command(version = "0.1.0")]
#[command(about = "A unified object storage CLI tool", long_about = None)]
pub struct Cli {
    /// Path to config file (default: ~/.oss-tool/config.yaml)
    #[arg(short, long, global = true)]
    pub config: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Copy files (supports upload/download/cross-storage copy)
    Cp(CpArgs),
    /// List objects
    Ls(LsArgs),
    /// Remove objects
    Rm(RmArgs),
    /// Show object information
    Stat(StatArgs),
}

#[derive(Args, Debug)]
pub struct CpArgs {
    /// Source path (local path or remote URI like s3://bucket/key)
    pub source: String,

    /// Destination path (local path or remote URI like s3://bucket/key)
    pub destination: String,

    /// Recursive copy for directories
    #[arg(short, long)]
    pub recursive: bool,

    /// Only include files matching pattern
    #[arg(long)]
    pub include: Option<String>,

    /// Exclude files matching pattern
    #[arg(long)]
    pub exclude: Option<String>,

    /// Number of concurrent operations (default: 4)
    #[arg(long, default_value = "4")]
    pub concurrency: usize,

    /// Part size for multipart upload (default: 8MB)
    #[arg(long, default_value = "8388608")]
    pub part_size: usize,

    /// Overwrite existing files
    #[arg(long)]
    pub overwrite: bool,

    /// Show progress bar
    #[arg(long)]
    pub progress: bool,
}

#[derive(Args, Debug)]
pub struct LsArgs {
    /// Remote URI (e.g., s3://bucket/prefix/)
    pub uri: String,

    /// List recursively
    #[arg(short, long)]
    pub recursive: bool,

    /// Long listing format (show details)
    #[arg(short, long)]
    pub long: bool,

    /// Human-readable sizes
    #[arg(short = 'H', long)]
    pub human_readable: bool,

    /// Maximum number of objects to list
    #[arg(long)]
    pub max_keys: Option<u32>,
}

#[derive(Args, Debug)]
pub struct RmArgs {
    /// Remote URI (e.g., s3://bucket/path/to/key)
    pub uri: String,

    /// Remove recursively (for prefix/directory)
    #[arg(short, long)]
    pub recursive: bool,

    /// Force removal without confirmation
    #[arg(short, long)]
    pub force: bool,

    /// Only include files matching pattern
    #[arg(long)]
    pub include: Option<String>,

    /// Exclude files matching pattern
    #[arg(long)]
    pub exclude: Option<String>,
}

#[derive(Args, Debug)]
pub struct StatArgs {
    /// Remote URI (e.g., s3://bucket/path/to/key)
    pub uri: String,
}
