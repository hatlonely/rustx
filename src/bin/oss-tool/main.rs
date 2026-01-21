// OSS tool - A unified object storage CLI tool

mod cli;
mod commands;
mod config;
mod progress;
mod store;
mod uri;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands};
use commands::{execute_cp, execute_ls, execute_rm, execute_stat};
use config::OssToolConfig;
use store::StoreManager;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config = OssToolConfig::load(cli.config.as_deref())?;

    // Create store manager
    let mut manager = StoreManager::new(config);

    // Execute command
    match &cli.command {
        Commands::Cp(args) => execute_cp(args, &mut manager).await?,
        Commands::Ls(args) => execute_ls(args, &mut manager).await?,
        Commands::Rm(args) => execute_rm(args, &mut manager).await?,
        Commands::Stat(args) => execute_stat(args, &mut manager).await?,
    }

    Ok(())
}
