// Remove command implementation

use anyhow::{anyhow, Result};
use std::io::{self, Write};

use crate::cli::RmArgs;
use crate::store::StoreManager;
use crate::uri::OssUri;

/// Execute the rm command
pub async fn execute_rm(args: &RmArgs, manager: &mut StoreManager) -> Result<()> {
    let uri = OssUri::parse(&args.uri)?;

    if uri.is_directory() || args.recursive {
        if !args.recursive {
            return Err(anyhow!(
                "Target appears to be a directory/prefix, use -r/--recursive to delete recursively"
            ));
        }
        delete_recursive(&uri, args, manager).await
    } else {
        delete_single(&uri, args, manager).await
    }
}

/// Delete a single object
async fn delete_single(uri: &OssUri, args: &RmArgs, manager: &mut StoreManager) -> Result<()> {
    let store = manager.get_store(uri)?;

    // Check if the object exists
    let meta = store.head_object(&uri.key).await?;
    if meta.is_none() {
        return Err(anyhow!("Object not found: {}", uri.key));
    }

    // Confirm deletion if not forced
    if !args.force {
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

    store.delete_object(&uri.key).await?;
    println!(
        "Deleted: {}://{}/{}",
        uri.provider.scheme(),
        uri.bucket,
        uri.key
    );

    Ok(())
}

/// Delete objects recursively
async fn delete_recursive(
    uri: &OssUri,
    args: &RmArgs,
    manager: &mut StoreManager,
) -> Result<()> {
    let store = manager.get_store(uri)?;

    // First, list all objects to be deleted
    let prefix = if uri.key.is_empty() {
        None
    } else {
        Some(uri.key.as_str())
    };

    let all_objects = store.list_objects(prefix, None).await?;

    let mut objects_to_delete = Vec::new();
    for obj in all_objects {
        // Apply include/exclude filters
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

        objects_to_delete.push(obj.key.clone());
    }

    if objects_to_delete.is_empty() {
        println!("No objects found to delete.");
        return Ok(());
    }

    // Confirm deletion if not forced
    if !args.force {
        println!(
            "The following {} objects will be deleted:",
            objects_to_delete.len()
        );
        for (i, key) in objects_to_delete.iter().enumerate() {
            if i < 10 {
                println!("  {}", key);
            } else if i == 10 {
                println!("  ... and {} more", objects_to_delete.len() - 10);
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
    }

    // Delete all objects
    let mut success_count = 0;
    let mut failed_count = 0;

    for key in &objects_to_delete {
        match store.delete_object(key).await {
            Ok(_) => {
                success_count += 1;
            }
            Err(e) => {
                failed_count += 1;
                eprintln!("Failed to delete {}: {}", key, e);
            }
        }
    }

    println!(
        "Deleted {} objects from {}://{}/{}",
        success_count,
        uri.provider.scheme(),
        uri.bucket,
        uri.key
    );

    if failed_count > 0 {
        eprintln!("Failed to delete {} objects", failed_count);
    }

    Ok(())
}
