// Stat command implementation

use anyhow::Result;

use crate::cli::StatArgs;
use crate::progress::{format_bytes, format_timestamp};
use rustx::oss::ObjectStoreManager;
use rustx::oss::OssUri;

/// Execute the stat command
pub async fn execute_stat(args: &StatArgs, manager: &mut ObjectStoreManager) -> Result<()> {
    let uri = OssUri::parse(&args.uri)?;
    let meta = manager.stat(&args.uri).await?;

    println!(
        "URI:           {}://{}/{}",
        uri.provider.scheme(),
        uri.bucket,
        meta.key
    );
    println!("Size:          {} ({})", meta.size, format_bytes(meta.size));
    println!("Last Modified: {}", format_timestamp(&meta.last_modified));

    if let Some(etag) = &meta.etag {
        println!("ETag:          {}", etag);
    }

    if let Some(content_type) = &meta.content_type {
        println!("Content-Type:  {}", content_type);
    }

    Ok(())
}
