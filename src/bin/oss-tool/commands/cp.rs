// Copy command implementation

use anyhow::Result;
use std::sync::Arc;

use crate::cli::CpArgs;
use rustx::oss::CpOptions;
use crate::progress::{format_bytes, LazyDirectoryProgressBar};
use rustx::oss::ObjectStoreManager;

/// Execute the cp command
pub async fn execute_cp(args: &CpArgs, manager: &mut ObjectStoreManager) -> Result<()> {
    // Create progress callback
    let progress_callback = Arc::new(LazyDirectoryProgressBar::new());

    // Convert CLI args to options
    let options = CpOptions {
        recursive: args.recursive,
        overwrite: args.overwrite,
        concurrency: Some(args.concurrency),
        part_size: Some(args.part_size),
        multipart_threshold: None, // Use default
        include: args.include.clone(),
        exclude: args.exclude.clone(),
        directory_progress_callback: Some(progress_callback.clone()),
    };

    let result = manager.cp(&args.source, &args.destination, options).await?;

    // Finish progress bar
    progress_callback.finish();

    // Print result
    if result.success_count == 1 && result.failed_count == 0 {
        println!(
            "Copied: {} -> {} ({})",
            args.source,
            args.destination,
            format_bytes(result.total_bytes)
        );
    } else {
        println!(
            "Copied {} files ({})",
            result.success_count,
            format_bytes(result.total_bytes)
        );
    }

    if result.failed_count > 0 {
        eprintln!("Failed to copy {} files:", result.failed_count);
        for failed in &result.failed_files {
            eprintln!("  - {}: {}", failed.path, failed.error);
        }
    }

    Ok(())
}
