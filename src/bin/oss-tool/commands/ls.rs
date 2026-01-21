// List command implementation

use anyhow::Result;

use crate::cli::LsArgs;
use crate::progress::{format_bytes, format_timestamp};
use crate::store::StoreManager;
use crate::uri::OssUri;

/// Execute the ls command
pub async fn execute_ls(args: &LsArgs, manager: &mut StoreManager) -> Result<()> {
    let uri = OssUri::parse(&args.uri)?;
    let store = manager.get_store(&uri)?;

    let max_keys = args.max_keys.map(|k| k as usize);
    let mut total_count = 0;
    let mut total_size: u64 = 0;

    // Use prefix from URI key
    let prefix = if uri.key.is_empty() {
        None
    } else {
        Some(uri.key.as_str())
    };

    let objects = store.list_objects(prefix, max_keys).await?;

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
