// Stat command implementation

use anyhow::{anyhow, Result};

use crate::cli::StatArgs;
use crate::progress::{format_bytes, format_timestamp};
use crate::store::StoreManager;
use crate::uri::OssUri;

/// Execute the stat command
pub async fn execute_stat(args: &StatArgs, manager: &mut StoreManager) -> Result<()> {
    let uri = OssUri::parse(&args.uri)?;
    let store = manager.get_store(&uri)?;

    let meta = store
        .head_object(&uri.key)
        .await?
        .ok_or_else(|| anyhow!("Object not found: {}", uri.key))?;

    println!("URI:           {}://{}/{}", uri.provider.scheme(), uri.bucket, meta.key);
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
