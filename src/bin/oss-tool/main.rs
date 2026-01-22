// OSS tool - A unified object storage CLI tool

mod cli;
mod commands;
mod progress;

use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::PathBuf;

use cli::{Cli, Commands};
use commands::{execute_cp, execute_ls, execute_rm, execute_stat};
use rustx::oss::{ObjectStoreManager, ObjectStoreManagerConfig};

/// Get default config path (~/.oss-tool/config.yaml)
fn default_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    Ok(home.join(".oss-tool").join("config.yaml"))
}

/// Load configuration from file
fn load_config(path: &PathBuf) -> Result<ObjectStoreManagerConfig> {
    if !path.exists() {
        return Err(anyhow::anyhow!(
            "Config file not found: {}. Please create it first.",
            path.display()
        ));
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let config: ObjectStoreManagerConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

    Ok(config)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Determine config path
    let config_path = match cli.config {
        Some(ref path) => PathBuf::from(shellexpand::tilde(path).to_string()),
        None => default_config_path()?,
    };

    // Load configuration
    let config = load_config(&config_path)?;

    // Create store manager
    let mut manager = ObjectStoreManager::new(config);

    // Execute command
    match &cli.command {
        Commands::Cp(args) => execute_cp(args, &mut manager).await?,
        Commands::Ls(args) => execute_ls(args, &mut manager).await?,
        Commands::Rm(args) => execute_rm(args, &mut manager).await?,
        Commands::Stat(args) => execute_stat(args, &mut manager).await?,
    }

    Ok(())
}
