// Copy command implementation

use anyhow::Result;
use std::sync::Arc;

use crate::cli::CpArgs;
use rustx::oss::CpOptions;
use crate::progress::{format_bytes, FileProgressBar};
use rustx::oss::ObjectStoreManager;
use rustx::oss::Location;

/// Execute the cp command
pub async fn execute_cp(args: &CpArgs, manager: &mut ObjectStoreManager) -> Result<()> {
    // Build progress callback if requested
    let progress_callback = if args.progress {
        // For single file, we create a progress bar
        // For directory, we'll let the output handle it
        let src = Location::parse(&args.source)?;
        let dst = Location::parse(&args.destination)?;

        match (&src, &dst) {
            // Single file upload
            (Location::Local(path), Location::Remote(_)) => {
                let path = std::path::Path::new(path);
                if path.is_file() {
                    let file_size = path.metadata()?.len();
                    let file_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    Some(Arc::new(FileProgressBar::new(file_size, &file_name))
                        as Arc<dyn rustx::oss::ProgressCallback>)
                } else {
                    None
                }
            }
            // Single file download - we need to get size from remote first
            (Location::Remote(_), Location::Local(_)) => {
                // Progress callback will be created after we know the file size
                None
            }
            _ => None,
        }
    } else {
        None
    };

    // Convert CLI args to options
    let options = CpOptions {
        recursive: args.recursive,
        overwrite: args.overwrite,
        concurrency: Some(args.concurrency),
        part_size: Some(args.part_size),
        multipart_threshold: None, // Use default
        include: args.include.clone(),
        exclude: args.exclude.clone(),
        progress_callback,
        directory_progress_callback: None,
    };

    let result = manager.cp(&args.source, &args.destination, options).await?;

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
