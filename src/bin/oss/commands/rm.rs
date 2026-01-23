// Remove command implementation

use anyhow::Result;
use std::io::{self, Write};

use crate::cli::RmArgs;
use rustx::oss::RmOptions;
use rustx::oss::ObjectStoreManager;
use rustx::oss::OssUri;

/// Execute the rm command
pub async fn execute_rm(args: &RmArgs, manager: &mut ObjectStoreManager) -> Result<()> {
    let uri = OssUri::parse(&args.uri)?;

    // Confirm deletion if not forced
    if !args.force {
        if uri.is_directory() || args.recursive {
            // For recursive delete, list objects first to show what will be deleted
            let options = rustx::oss::LsOptions {
                max_keys: None,
            };
            let objects = manager.ls(&args.uri, options).await?;

            // Apply filters to get actual count
            let mut filtered_keys = Vec::new();
            for obj in &objects {
                let relative_key = obj.key.strip_prefix(&uri.key).unwrap_or(&obj.key);

                if let Some(ref include) = args.include {
                    if !glob::Pattern::new(include)
                        .map(|p| p.matches(relative_key))
                        .unwrap_or(false)
                    {
                        continue;
                    }
                }
                if let Some(ref exclude) = args.exclude {
                    if glob::Pattern::new(exclude)
                        .map(|p| p.matches(relative_key))
                        .unwrap_or(false)
                    {
                        continue;
                    }
                }

                filtered_keys.push(&obj.key);
            }

            if filtered_keys.is_empty() {
                println!("No objects found to delete.");
                return Ok(());
            }

            println!(
                "The following {} objects will be deleted:",
                filtered_keys.len()
            );
            for (i, key) in filtered_keys.iter().enumerate() {
                if i < 10 {
                    println!("  {}", key);
                } else if i == 10 {
                    println!("  ... and {} more", filtered_keys.len() - 10);
                    break;
                }
            }

            print!("Continue? [y/N] ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborted.");
                return Ok(());
            }
        } else {
            // Single file delete
            print!(
                "Delete {}://{}/{}? [y/N] ",
                uri.provider.scheme(),
                uri.bucket,
                uri.key
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborted.");
                return Ok(());
            }
        }
    }

    let options = RmOptions {
        recursive: args.recursive,
        include: args.include.clone(),
        exclude: args.exclude.clone(),
    };

    let result = manager.rm(&args.uri, options).await?;

    // Print result
    if result.deleted_count == 1 && result.failed_count == 0 {
        println!(
            "Deleted: {}://{}/{}",
            uri.provider.scheme(),
            uri.bucket,
            uri.key
        );
    } else {
        println!(
            "Deleted {} objects from {}://{}/{}",
            result.deleted_count,
            uri.provider.scheme(),
            uri.bucket,
            uri.key
        );
    }

    if result.failed_count > 0 {
        eprintln!("Failed to delete {} objects:", result.failed_count);
        for failed in &result.failed_files {
            eprintln!("  - {}: {}", failed.path, failed.error);
        }
    }

    Ok(())
}
