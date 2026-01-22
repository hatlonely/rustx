// List command implementation

use anyhow::Result;

use crate::cli::LsArgs;
use rustx::oss::LsOptions;
use crate::progress::{format_bytes, format_timestamp};
use rustx::oss::ObjectStoreManager;

/// Execute the ls command
pub async fn execute_ls(args: &LsArgs, manager: &mut ObjectStoreManager) -> Result<()> {
    let options = LsOptions {
        max_keys: args.max_keys.map(|k| k as usize),
    };

    let objects = manager.ls(&args.uri, options).await?;

    let mut total_count = 0;
    let mut total_size: u64 = 0;

    for obj in &objects {
        total_count += 1;
        total_size += obj.size;

        if args.long {
            // Long format: timestamp size key
            let timestamp = format_timestamp(&obj.last_modified);
            let size_str = if args.human_readable {
                format!("{:>10}", format_bytes(obj.size))
            } else {
                format!("{:>12}", obj.size)
            };
            println!("{} {} {}", timestamp, size_str, obj.key);
        } else {
            // Short format: just the key
            println!("{}", obj.key);
        }
    }

    // Print summary
    if args.long {
        println!();
        println!(
            "Total: {} objects, {}",
            total_count,
            format_bytes(total_size)
        );
    }

    Ok(())
}
