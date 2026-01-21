// Copy command implementation

use anyhow::{anyhow, Result};
use rustx::oss::{
    GetDirectoryOptions, GetFileOptions, ObjectStore, ProgressCallback, PutDirectoryOptions,
    PutFileOptions,
};
use std::path::Path;
use std::sync::Arc;

use crate::cli::CpArgs;
use crate::config::DefaultOptions;
use crate::progress::{format_bytes, FileProgressBar};
use crate::store::StoreManager;
use crate::uri::{Location, OssUri};

/// Copy direction
#[derive(Debug)]
enum CopyDirection {
    Upload,   // local -> remote
    Download, // remote -> local
    Remote,   // remote -> remote
}

/// Execute the cp command
pub async fn execute_cp(args: &CpArgs, manager: &mut StoreManager) -> Result<()> {
    let src = Location::parse(&args.source)?;
    let dst = Location::parse(&args.destination)?;

    let direction = match (&src, &dst) {
        (Location::Local(_), Location::Remote(_)) => CopyDirection::Upload,
        (Location::Remote(_), Location::Local(_)) => CopyDirection::Download,
        (Location::Remote(_), Location::Remote(_)) => CopyDirection::Remote,
        (Location::Local(_), Location::Local(_)) => {
            return Err(anyhow!("At least one path must be remote"));
        }
    };

    // Clone defaults to avoid borrow issues
    let defaults = manager.defaults().clone();

    match direction {
        CopyDirection::Upload => {
            let local_path = src.as_local().unwrap();
            let remote_uri = dst.as_remote().unwrap();
            let store = manager.get_store(remote_uri)?;
            upload(local_path, remote_uri, args, store.as_ref(), &defaults).await
        }
        CopyDirection::Download => {
            let remote_uri = src.as_remote().unwrap();
            let local_path = dst.as_local().unwrap();
            let store = manager.get_store(remote_uri)?;
            download(remote_uri, local_path, args, store.as_ref()).await
        }
        CopyDirection::Remote => {
            let src_uri = src.as_remote().unwrap();
            let dst_uri = dst.as_remote().unwrap();
            let src_store = manager.get_store(src_uri)?;
            let dst_store = manager.get_store(dst_uri)?;
            copy_remote(src_uri, dst_uri, args, src_store.as_ref(), dst_store.as_ref()).await
        }
    }
}

/// Upload local file/directory to remote
async fn upload(
    local_path: &str,
    remote_uri: &OssUri,
    args: &CpArgs,
    store: &dyn ObjectStore,
    defaults: &DefaultOptions,
) -> Result<()> {
    let path = Path::new(local_path);

    if path.is_dir() {
        if !args.recursive {
            return Err(anyhow!(
                "Source is a directory, use -r/--recursive to copy directories"
            ));
        }
        upload_directory(path, remote_uri, args, store, defaults).await
    } else if path.is_file() {
        upload_file(path, remote_uri, args, store, defaults).await
    } else {
        Err(anyhow!("Source path does not exist: {}", local_path))
    }
}

/// Upload a single file
async fn upload_file(
    local_path: &Path,
    remote_uri: &OssUri,
    args: &CpArgs,
    store: &dyn ObjectStore,
    defaults: &DefaultOptions,
) -> Result<()> {
    // Determine the destination key
    let key = if remote_uri.is_directory() {
        // If destination is a directory, append the file name
        let file_name = local_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid file path"))?
            .to_string_lossy();
        format!("{}{}", remote_uri.key, file_name)
    } else {
        remote_uri.key.clone()
    };

    // Check if file exists
    if !args.overwrite {
        if let Some(_) = store.head_object(&key).await? {
            return Err(anyhow!("Destination already exists: {}", key));
        }
    }

    let file_size = local_path.metadata()?.len();
    let file_name = local_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // Create progress bar if requested
    let progress_bar: Option<Arc<FileProgressBar>> = if args.progress {
        Some(Arc::new(FileProgressBar::new(file_size, &file_name)))
    } else {
        None
    };

    let progress_callback: Option<Arc<dyn ProgressCallback>> =
        progress_bar.clone().map(|pb| pb as Arc<dyn ProgressCallback>);

    let options = PutFileOptions {
        content_type: None,
        metadata: None,
        multipart_threshold: defaults.multipart_threshold,
        part_size: args.part_size,
        multipart_concurrency: args.concurrency,
        progress_callback,
    };

    store.put_file(&key, local_path, options).await?;

    if let Some(pb) = progress_bar {
        pb.finish();
    }

    println!(
        "Uploaded: {} -> {}://{}/{}",
        local_path.display(),
        remote_uri.provider.scheme(),
        remote_uri.bucket,
        key
    );

    Ok(())
}

/// Upload a directory
async fn upload_directory(
    local_path: &Path,
    remote_uri: &OssUri,
    args: &CpArgs,
    store: &dyn ObjectStore,
    defaults: &DefaultOptions,
) -> Result<()> {
    let include_patterns = args.include.as_ref().map(|p| vec![p.clone()]);
    let exclude_patterns = args.exclude.as_ref().map(|p| vec![p.clone()]);

    let options = PutDirectoryOptions {
        concurrency: args.concurrency,
        include_patterns,
        exclude_patterns,
        recursive: args.recursive,
        multipart_threshold: defaults.multipart_threshold,
        part_size: args.part_size,
        multipart_concurrency: args.concurrency,
        progress_callback: None,
    };

    let result = store
        .put_directory(&remote_uri.key, local_path, options)
        .await?;

    println!(
        "Uploaded {} files ({}) to {}://{}/{}",
        result.success_count,
        format_bytes(result.total_bytes),
        remote_uri.provider.scheme(),
        remote_uri.bucket,
        remote_uri.key
    );

    if result.failed_count > 0 {
        eprintln!("Failed to upload {} files:", result.failed_count);
        for failed in &result.failed_files {
            eprintln!("  - {}: {}", failed.path, failed.error);
        }
    }

    Ok(())
}

/// Download remote file/directory to local
async fn download(
    remote_uri: &OssUri,
    local_path: &str,
    args: &CpArgs,
    store: &dyn ObjectStore,
) -> Result<()> {
    let path = Path::new(local_path);

    // Check if source is a directory (prefix)
    if remote_uri.is_directory() || args.recursive {
        if !args.recursive {
            return Err(anyhow!(
                "Source appears to be a directory, use -r/--recursive to download directories"
            ));
        }
        download_directory(remote_uri, path, args, store).await
    } else {
        download_file(remote_uri, path, args, store).await
    }
}

/// Download a single file
async fn download_file(
    remote_uri: &OssUri,
    local_path: &Path,
    args: &CpArgs,
    store: &dyn ObjectStore,
) -> Result<()> {
    // Determine the destination path
    let dest_path = if local_path.is_dir() {
        // If destination is a directory, append the file name from the key
        let file_name = remote_uri
            .file_name()
            .ok_or_else(|| anyhow!("Cannot determine file name from URI"))?;
        local_path.join(file_name)
    } else {
        local_path.to_path_buf()
    };

    // Get file info for progress
    let meta = store
        .head_object(&remote_uri.key)
        .await?
        .ok_or_else(|| anyhow!("Object not found: {}", remote_uri.key))?;

    let file_name = remote_uri.file_name().unwrap_or(&remote_uri.key);

    // Create progress bar if requested
    let progress_bar: Option<Arc<FileProgressBar>> = if args.progress {
        Some(Arc::new(FileProgressBar::new(meta.size, file_name)))
    } else {
        None
    };

    let progress_callback: Option<Arc<dyn ProgressCallback>> =
        progress_bar.clone().map(|pb| pb as Arc<dyn ProgressCallback>);

    let options = GetFileOptions {
        overwrite: args.overwrite,
        progress_callback,
    };

    store.get_file(&remote_uri.key, &dest_path, options).await?;

    if let Some(pb) = progress_bar {
        pb.finish();
    }

    println!(
        "Downloaded: {}://{}/{} -> {}",
        remote_uri.provider.scheme(),
        remote_uri.bucket,
        remote_uri.key,
        dest_path.display()
    );

    Ok(())
}

/// Download a directory
async fn download_directory(
    remote_uri: &OssUri,
    local_path: &Path,
    args: &CpArgs,
    store: &dyn ObjectStore,
) -> Result<()> {
    let options = GetDirectoryOptions {
        concurrency: args.concurrency,
        overwrite: args.overwrite,
        progress_callback: None,
    };

    let result = store
        .get_directory(&remote_uri.key, local_path, options)
        .await?;

    println!(
        "Downloaded {} files ({}) from {}://{}/{} to {}",
        result.success_count,
        format_bytes(result.total_bytes),
        remote_uri.provider.scheme(),
        remote_uri.bucket,
        remote_uri.key,
        local_path.display()
    );

    if result.failed_count > 0 {
        eprintln!("Failed to download {} files:", result.failed_count);
        for failed in &result.failed_files {
            eprintln!("  - {}: {}", failed.path, failed.error);
        }
    }

    Ok(())
}

/// Copy between remote locations
async fn copy_remote(
    src_uri: &OssUri,
    dst_uri: &OssUri,
    args: &CpArgs,
    src_store: &dyn ObjectStore,
    dst_store: &dyn ObjectStore,
) -> Result<()> {
    if src_uri.is_directory() || args.recursive {
        if !args.recursive {
            return Err(anyhow!(
                "Source appears to be a directory, use -r/--recursive to copy directories"
            ));
        }
        copy_remote_directory(src_uri, dst_uri, args, src_store, dst_store).await
    } else {
        copy_remote_file(src_uri, dst_uri, args, src_store, dst_store).await
    }
}

/// Copy a single file between remote locations
async fn copy_remote_file(
    src_uri: &OssUri,
    dst_uri: &OssUri,
    args: &CpArgs,
    src_store: &dyn ObjectStore,
    dst_store: &dyn ObjectStore,
) -> Result<()> {
    // Determine the destination key
    let dst_key = if dst_uri.is_directory() {
        let file_name = src_uri
            .file_name()
            .ok_or_else(|| anyhow!("Cannot determine file name from source URI"))?;
        format!("{}{}", dst_uri.key, file_name)
    } else {
        dst_uri.key.clone()
    };

    // Check if destination exists
    if !args.overwrite {
        if let Some(_) = dst_store.head_object(&dst_key).await? {
            return Err(anyhow!("Destination already exists: {}", dst_key));
        }
    }

    // Get file content from source
    let data = src_store.get_object(&src_uri.key).await?;

    // Upload to destination
    dst_store.put_object(&dst_key, data).await?;

    println!(
        "Copied: {}://{}/{} -> {}://{}/{}",
        src_uri.provider.scheme(),
        src_uri.bucket,
        src_uri.key,
        dst_uri.provider.scheme(),
        dst_uri.bucket,
        dst_key
    );

    Ok(())
}

/// Copy a directory between remote locations
async fn copy_remote_directory(
    src_uri: &OssUri,
    dst_uri: &OssUri,
    args: &CpArgs,
    src_store: &dyn ObjectStore,
    dst_store: &dyn ObjectStore,
) -> Result<()> {
    let mut success_count = 0;
    let mut failed_count = 0;
    let mut total_bytes: u64 = 0;

    // List all objects with the source prefix
    let prefix = if src_uri.key.is_empty() {
        None
    } else {
        Some(src_uri.key.as_str())
    };
    let objects = src_store.list_objects(prefix, None).await?;

    for obj in objects {
        // Calculate relative path from source prefix
        let relative_key = obj.key.strip_prefix(&src_uri.key).unwrap_or(&obj.key);

        // Apply include/exclude filters
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

        // Calculate destination key
        let dst_key = format!("{}{}", dst_uri.key, relative_key);

        // Check if destination exists
        if !args.overwrite {
            if let Some(_) = dst_store.head_object(&dst_key).await? {
                eprintln!("Skipping (exists): {}", dst_key);
                continue;
            }
        }

        // Copy the object
        match src_store.get_object(&obj.key).await {
            Ok(data) => {
                let size = data.len() as u64;
                match dst_store.put_object(&dst_key, data).await {
                    Ok(_) => {
                        success_count += 1;
                        total_bytes += size;
                        if args.progress {
                            println!("Copied: {} -> {}", obj.key, dst_key);
                        }
                    }
                    Err(e) => {
                        failed_count += 1;
                        eprintln!("Failed to upload {}: {}", dst_key, e);
                    }
                }
            }
            Err(e) => {
                failed_count += 1;
                eprintln!("Failed to download {}: {}", obj.key, e);
            }
        }
    }

    println!(
        "Copied {} files ({}) from {}://{}/{} to {}://{}/{}",
        success_count,
        format_bytes(total_bytes),
        src_uri.provider.scheme(),
        src_uri.bucket,
        src_uri.key,
        dst_uri.provider.scheme(),
        dst_uri.bucket,
        dst_uri.key
    );

    if failed_count > 0 {
        eprintln!("Failed to copy {} files", failed_count);
    }

    Ok(())
}
