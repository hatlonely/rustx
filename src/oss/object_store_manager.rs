// Store matching and management

use anyhow::{anyhow, Result};
use crate::cfg::{create_trait_from_type_options, register, ConfigReloader, TypeOptions};
use super::object_store_types::{FailedFile, GetDirectoryOptions, GetFileOptions, ObjectMeta, PutDirectoryOptions, PutFileOptions};
use super::object_store::ObjectStore;
use crate::oss::register_object_store;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::object_store_manager_types::{CpOptions, CpResult, LsOptions, RmOptions, RmResult};
use super::uri::{Location, OssUri, Provider};

/// StoreManager configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ObjectStoreManagerConfig {
    /// ObjectStore configuration list
    pub stores: Vec<TypeOptions>,

    /// Default options
    pub defaults: DefaultOptions,
}

/// Default options for operations
#[derive(Debug, Clone, Deserialize, Serialize, SmartDefault)]
#[serde(default)]
pub struct DefaultOptions {
    /// Concurrent operations count (default: 4)
    #[default = 4]
    pub concurrency: usize,

    /// Part size for multipart upload (default: 8MB)
    #[default = 8388608]
    pub part_size: usize,

    /// Threshold for multipart upload (default: 100MB)
    #[default = 104857600]
    pub multipart_threshold: u64,
}

/// Register StoreManager to the type system
///
/// This function must be called before creating StoreManager instances
/// through the configuration system.
pub fn register_store_manager() -> Result<()> {
    register::<ObjectStoreManager, ObjectStoreManagerConfig>("StoreManager")
        .map_err(|e| anyhow!("Failed to register StoreManager: {}", e))
}

/// Store manager for caching and retrieving ObjectStore instances
///
/// Provides high-level file operations (cp, ls, rm, stat) on top of ObjectStore.
pub struct ObjectStoreManager {
    config: ObjectStoreManagerConfig,
    cache: HashMap<(Provider, String), Arc<dyn ObjectStore>>,
}

impl ObjectStoreManager {
    /// Create a new StoreManager with the given configuration
    pub fn new(config: ObjectStoreManagerConfig) -> Self {
        // Register ObjectStore implementations
        register_object_store();

        Self {
            config,
            cache: HashMap::new(),
        }
    }

    /// Get or create an ObjectStore for the given URI
    pub fn get_store(&mut self, uri: &OssUri) -> Result<Arc<dyn ObjectStore>> {
        // Check cache first
        let cache_key = (uri.provider.clone(), uri.bucket.clone());
        if let Some(store) = self.cache.get(&cache_key) {
            return Ok(Arc::clone(store));
        }

        // Find matching store configuration
        let store_config = self
            .find_store_config(&uri.bucket)
            .ok_or_else(|| anyhow!("No store configured for bucket: {}", uri.bucket))?;

        // Create store instance
        let store = create_store_from_config(store_config)?;
        let store = Arc::from(store);

        // Cache the store
        self.cache.insert(cache_key, Arc::clone(&store));

        Ok(store)
    }

    /// Find store configuration by bucket name
    fn find_store_config(&self, bucket: &str) -> Option<&TypeOptions> {
        self.config.stores.iter().find(|store| {
            store
                .options
                .get("bucket")
                .and_then(|v| v.as_str())
                .map(|b| b == bucket)
                .unwrap_or(false)
        })
    }

    /// Get default options from config
    pub fn defaults(&self) -> &DefaultOptions {
        &self.config.defaults
    }

    // ============ High-level File Operations ============

    /// Copy files/directories between local and remote locations
    ///
    /// Automatically determines the copy direction:
    /// - local -> remote: upload
    /// - remote -> local: download
    /// - remote -> remote: remote copy
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Upload a file
    /// manager.cp("./file.txt", "s3://bucket/file.txt", CpOptions::default()).await?;
    ///
    /// // Download a file
    /// manager.cp("s3://bucket/file.txt", "./file.txt", CpOptions::default()).await?;
    ///
    /// // Upload a directory recursively
    /// manager.cp("./dir/", "s3://bucket/prefix/", CpOptions {
    ///     recursive: true,
    ///     concurrency: Some(8),
    ///     ..Default::default()
    /// }).await?;
    /// ```
    pub async fn cp(&mut self, from: &str, to: &str, options: CpOptions) -> Result<CpResult> {
        let src = Location::parse(from)?;
        let dst = Location::parse(to)?;

        // Resolve options with defaults
        let concurrency = options
            .concurrency
            .unwrap_or(self.config.defaults.concurrency);
        let part_size = options.part_size.unwrap_or(self.config.defaults.part_size);
        let multipart_threshold = options
            .multipart_threshold
            .unwrap_or(self.config.defaults.multipart_threshold);

        match (&src, &dst) {
            (Location::Local(local_path), Location::Remote(remote_uri)) => {
                let store = self.get_store(remote_uri)?;
                self.upload(
                    local_path,
                    remote_uri,
                    &options,
                    store.as_ref(),
                    concurrency,
                    part_size,
                    multipart_threshold,
                )
                .await
            }
            (Location::Remote(remote_uri), Location::Local(local_path)) => {
                let store = self.get_store(remote_uri)?;
                self.download(remote_uri, local_path, &options, store.as_ref(), concurrency)
                    .await
            }
            (Location::Remote(src_uri), Location::Remote(dst_uri)) => {
                let src_store = self.get_store(src_uri)?;
                let dst_store = self.get_store(dst_uri)?;
                self.copy_remote(
                    src_uri,
                    dst_uri,
                    &options,
                    src_store.as_ref(),
                    dst_store.as_ref(),
                    concurrency,
                )
                .await
            }
            (Location::Local(_), Location::Local(_)) => {
                Err(anyhow!("At least one path must be remote"))
            }
        }
    }

    /// List objects at a remote URI
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let objects = manager.ls("s3://bucket/prefix/", LsOptions::default()).await?;
    /// for obj in objects {
    ///     println!("{}: {} bytes", obj.key, obj.size);
    /// }
    /// ```
    pub async fn ls(&mut self, uri: &str, options: LsOptions) -> Result<Vec<ObjectMeta>> {
        let parsed_uri = OssUri::parse(uri)?;
        let store = self.get_store(&parsed_uri)?;

        let prefix = if parsed_uri.key.is_empty() {
            None
        } else {
            Some(parsed_uri.key.as_str())
        };

        let objects = store.list_objects(prefix, options.max_keys).await?;
        Ok(objects)
    }

    /// Delete objects at a remote URI
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Delete a single file
    /// manager.rm("s3://bucket/file.txt", RmOptions::default()).await?;
    ///
    /// // Delete a directory recursively
    /// manager.rm("s3://bucket/prefix/", RmOptions {
    ///     recursive: true,
    ///     ..Default::default()
    /// }).await?;
    /// ```
    pub async fn rm(&mut self, uri: &str, options: RmOptions) -> Result<RmResult> {
        let parsed_uri = OssUri::parse(uri)?;
        let store = self.get_store(&parsed_uri)?;

        if parsed_uri.is_directory() || options.recursive {
            if !options.recursive {
                return Err(anyhow!(
                    "Target appears to be a directory/prefix, use recursive: true to delete"
                ));
            }
            self.delete_recursive(&parsed_uri, &options, store.as_ref())
                .await
        } else {
            self.delete_single(&parsed_uri, store.as_ref()).await
        }
    }

    /// Get metadata for an object at a remote URI
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let meta = manager.stat("s3://bucket/file.txt").await?;
    /// println!("Size: {}, Last Modified: {:?}", meta.size, meta.last_modified);
    /// ```
    pub async fn stat(&mut self, uri: &str) -> Result<ObjectMeta> {
        let parsed_uri = OssUri::parse(uri)?;
        let store = self.get_store(&parsed_uri)?;

        store
            .head_object(&parsed_uri.key)
            .await?
            .ok_or_else(|| anyhow!("Object not found: {}", parsed_uri.key))
    }

    // ============ Private Implementation Methods ============

    /// Upload local file/directory to remote
    async fn upload(
        &self,
        local_path: &str,
        remote_uri: &OssUri,
        options: &CpOptions,
        store: &dyn ObjectStore,
        concurrency: usize,
        part_size: usize,
        multipart_threshold: u64,
    ) -> Result<CpResult> {
        let path = Path::new(local_path);

        if path.is_dir() {
            if !options.recursive {
                return Err(anyhow!(
                    "Source is a directory, use recursive: true to copy directories"
                ));
            }
            self.upload_directory(
                path,
                remote_uri,
                options,
                store,
                concurrency,
                part_size,
                multipart_threshold,
            )
            .await
        } else if path.is_file() {
            self.upload_file(
                path,
                remote_uri,
                options,
                store,
                part_size,
                multipart_threshold,
            )
            .await
        } else {
            Err(anyhow!("Source path does not exist: {}", local_path))
        }
    }

    /// Upload a single file
    async fn upload_file(
        &self,
        local_path: &Path,
        remote_uri: &OssUri,
        options: &CpOptions,
        store: &dyn ObjectStore,
        part_size: usize,
        multipart_threshold: u64,
    ) -> Result<CpResult> {
        // Determine the destination key
        let key = if remote_uri.is_directory() {
            let file_name = local_path
                .file_name()
                .ok_or_else(|| anyhow!("Invalid file path"))?
                .to_string_lossy();
            format!("{}{}", remote_uri.key, file_name)
        } else {
            remote_uri.key.clone()
        };

        // Check if file exists
        if !options.overwrite {
            if store.head_object(&key).await?.is_some() {
                return Err(anyhow!("Destination already exists: {}", key));
            }
        }

        let file_size = local_path.metadata()?.len();

        let put_options = PutFileOptions {
            content_type: None,
            metadata: None,
            multipart_threshold,
            part_size,
            multipart_concurrency: options
                .concurrency
                .unwrap_or(self.config.defaults.concurrency),
            progress_callback: options.progress_callback.clone(),
        };

        store.put_file(&key, local_path, put_options).await?;

        Ok(CpResult::single_success(file_size))
    }

    /// Upload a directory
    async fn upload_directory(
        &self,
        local_path: &Path,
        remote_uri: &OssUri,
        options: &CpOptions,
        store: &dyn ObjectStore,
        concurrency: usize,
        part_size: usize,
        multipart_threshold: u64,
    ) -> Result<CpResult> {
        let include_patterns = options.include.as_ref().map(|p| vec![p.clone()]);
        let exclude_patterns = options.exclude.as_ref().map(|p| vec![p.clone()]);

        let put_options = PutDirectoryOptions {
            concurrency,
            include_patterns,
            exclude_patterns,
            recursive: options.recursive,
            multipart_threshold,
            part_size,
            multipart_concurrency: concurrency,
            progress_callback: options.directory_progress_callback.clone(),
        };

        let result = store
            .put_directory(&remote_uri.key, local_path, put_options)
            .await?;

        Ok(result.into())
    }

    /// Download remote file/directory to local
    async fn download(
        &self,
        remote_uri: &OssUri,
        local_path: &str,
        options: &CpOptions,
        store: &dyn ObjectStore,
        concurrency: usize,
    ) -> Result<CpResult> {
        let path = Path::new(local_path);

        if remote_uri.is_directory() || options.recursive {
            if !options.recursive {
                return Err(anyhow!(
                    "Source appears to be a directory, use recursive: true to download directories"
                ));
            }
            self.download_directory(remote_uri, path, options, store, concurrency)
                .await
        } else {
            self.download_file(remote_uri, path, options, store).await
        }
    }

    /// Download a single file
    async fn download_file(
        &self,
        remote_uri: &OssUri,
        local_path: &Path,
        options: &CpOptions,
        store: &dyn ObjectStore,
    ) -> Result<CpResult> {
        // Determine the destination path
        let dest_path = if local_path.is_dir() {
            let file_name = remote_uri
                .file_name()
                .ok_or_else(|| anyhow!("Cannot determine file name from URI"))?;
            local_path.join(file_name)
        } else {
            local_path.to_path_buf()
        };

        // Get file info
        let meta = store
            .head_object(&remote_uri.key)
            .await?
            .ok_or_else(|| anyhow!("Object not found: {}", remote_uri.key))?;

        let get_options = GetFileOptions {
            overwrite: options.overwrite,
            progress_callback: options.progress_callback.clone(),
        };

        store
            .get_file(&remote_uri.key, &dest_path, get_options)
            .await?;

        Ok(CpResult::single_success(meta.size))
    }

    /// Download a directory
    async fn download_directory(
        &self,
        remote_uri: &OssUri,
        local_path: &Path,
        options: &CpOptions,
        store: &dyn ObjectStore,
        concurrency: usize,
    ) -> Result<CpResult> {
        // If no filters, use the efficient built-in method
        if options.include.is_none() && options.exclude.is_none() {
            let get_options = GetDirectoryOptions {
                concurrency,
                overwrite: options.overwrite,
                progress_callback: options.directory_progress_callback.clone(),
            };

            let result = store
                .get_directory(&remote_uri.key, local_path, get_options)
                .await?;

            return Ok(result.into());
        }

        // With filters, we need to list -> filter -> download manually
        let mut result = CpResult::default();

        // List all objects with the prefix
        let prefix = if remote_uri.key.is_empty() {
            None
        } else {
            Some(remote_uri.key.as_str())
        };
        let objects = store.list_objects(prefix, None).await?;

        // Create local directory if it doesn't exist
        if !local_path.exists() {
            tokio::fs::create_dir_all(local_path).await?;
        }

        for obj in objects {
            // Calculate relative path from prefix
            let relative_key = obj.key.strip_prefix(&remote_uri.key).unwrap_or(&obj.key);

            // Skip empty keys (the prefix itself)
            if relative_key.is_empty() {
                continue;
            }

            // Apply include/exclude filters
            if let Some(ref include) = options.include {
                if !glob::Pattern::new(include)
                    .map(|p| p.matches(relative_key))
                    .unwrap_or(false)
                {
                    continue;
                }
            }
            if let Some(ref exclude) = options.exclude {
                if glob::Pattern::new(exclude)
                    .map(|p| p.matches(relative_key))
                    .unwrap_or(false)
                {
                    continue;
                }
            }

            // Calculate local file path
            let file_path = local_path.join(relative_key);

            // Create parent directories if needed
            if let Some(parent) = file_path.parent() {
                if !parent.exists() {
                    tokio::fs::create_dir_all(parent).await?;
                }
            }

            // Check if file exists
            if !options.overwrite && file_path.exists() {
                continue;
            }

            // Download the file
            let get_options = GetFileOptions {
                overwrite: options.overwrite,
                progress_callback: None,
            };

            match store.get_file(&obj.key, &file_path, get_options).await {
                Ok(_) => {
                    result.success_count += 1;
                    result.total_bytes += obj.size;
                }
                Err(e) => {
                    result.failed_count += 1;
                    result.failed_files.push(FailedFile {
                        path: obj.key.clone(),
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok(result)
    }

    /// Copy between remote locations
    async fn copy_remote(
        &self,
        src_uri: &OssUri,
        dst_uri: &OssUri,
        options: &CpOptions,
        src_store: &dyn ObjectStore,
        dst_store: &dyn ObjectStore,
        _concurrency: usize,
    ) -> Result<CpResult> {
        if src_uri.is_directory() || options.recursive {
            if !options.recursive {
                return Err(anyhow!(
                    "Source appears to be a directory, use recursive: true to copy directories"
                ));
            }
            self.copy_remote_directory(src_uri, dst_uri, options, src_store, dst_store)
                .await
        } else {
            self.copy_remote_file(src_uri, dst_uri, options, src_store, dst_store)
                .await
        }
    }

    /// Copy a single file between remote locations
    async fn copy_remote_file(
        &self,
        src_uri: &OssUri,
        dst_uri: &OssUri,
        options: &CpOptions,
        src_store: &dyn ObjectStore,
        dst_store: &dyn ObjectStore,
    ) -> Result<CpResult> {
        let dst_key = if dst_uri.is_directory() {
            let file_name = src_uri
                .file_name()
                .ok_or_else(|| anyhow!("Cannot determine file name from source URI"))?;
            format!("{}{}", dst_uri.key, file_name)
        } else {
            dst_uri.key.clone()
        };

        // Check if destination exists
        if !options.overwrite {
            if dst_store.head_object(&dst_key).await?.is_some() {
                return Err(anyhow!("Destination already exists: {}", dst_key));
            }
        }

        // Get file content from source
        let data = src_store.get_object(&src_uri.key).await?;
        let size = data.len() as u64;

        // Upload to destination
        dst_store.put_object(&dst_key, data).await?;

        Ok(CpResult::single_success(size))
    }

    /// Copy a directory between remote locations
    async fn copy_remote_directory(
        &self,
        src_uri: &OssUri,
        dst_uri: &OssUri,
        options: &CpOptions,
        src_store: &dyn ObjectStore,
        dst_store: &dyn ObjectStore,
    ) -> Result<CpResult> {
        let mut result = CpResult::default();

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
            if let Some(ref include) = options.include {
                if !glob::Pattern::new(include)
                    .map(|p| p.matches(relative_key))
                    .unwrap_or(false)
                {
                    continue;
                }
            }
            if let Some(ref exclude) = options.exclude {
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
            if !options.overwrite {
                if dst_store.head_object(&dst_key).await?.is_some() {
                    continue;
                }
            }

            // Copy the object
            match src_store.get_object(&obj.key).await {
                Ok(data) => {
                    let size = data.len() as u64;
                    match dst_store.put_object(&dst_key, data).await {
                        Ok(_) => {
                            result.success_count += 1;
                            result.total_bytes += size;
                        }
                        Err(e) => {
                            result.failed_count += 1;
                            result.failed_files.push(FailedFile {
                                path: dst_key,
                                error: e.to_string(),
                            });
                        }
                    }
                }
                Err(e) => {
                    result.failed_count += 1;
                    result.failed_files.push(FailedFile {
                        path: obj.key.clone(),
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok(result)
    }

    /// Delete a single object
    async fn delete_single(&self, uri: &OssUri, store: &dyn ObjectStore) -> Result<RmResult> {
        // Check if the object exists
        let meta = store.head_object(&uri.key).await?;
        if meta.is_none() {
            return Err(anyhow!("Object not found: {}", uri.key));
        }

        store.delete_object(&uri.key).await?;

        Ok(RmResult {
            deleted_count: 1,
            failed_count: 0,
            failed_files: Vec::new(),
        })
    }

    /// Delete objects recursively
    async fn delete_recursive(
        &self,
        uri: &OssUri,
        options: &RmOptions,
        store: &dyn ObjectStore,
    ) -> Result<RmResult> {
        let mut result = RmResult::default();

        // List all objects to be deleted
        let prefix = if uri.key.is_empty() {
            None
        } else {
            Some(uri.key.as_str())
        };

        let all_objects = store.list_objects(prefix, None).await?;

        let mut objects_to_delete = Vec::new();
        for obj in all_objects {
            let relative_key = obj.key.strip_prefix(&uri.key).unwrap_or(&obj.key);

            // Apply include/exclude filters
            if let Some(ref include) = options.include {
                if !glob::Pattern::new(include)
                    .map(|p| p.matches(relative_key))
                    .unwrap_or(false)
                {
                    continue;
                }
            }
            if let Some(ref exclude) = options.exclude {
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
            return Ok(result);
        }

        // Delete all objects
        for key in &objects_to_delete {
            match store.delete_object(key).await {
                Ok(_) => {
                    result.deleted_count += 1;
                }
                Err(e) => {
                    result.failed_count += 1;
                    result.failed_files.push(FailedFile {
                        path: key.clone(),
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok(result)
    }
}

/// Create an ObjectStore from TypeOptions configuration
fn create_store_from_config(config: &TypeOptions) -> Result<Box<dyn ObjectStore>> {
    create_trait_from_type_options::<dyn ObjectStore>(config)
        .map_err(|e| anyhow!("Failed to create ObjectStore: {}", e))
}

// Implement From trait for configuration system integration
impl From<ObjectStoreManagerConfig> for ObjectStoreManager {
    fn from(config: ObjectStoreManagerConfig) -> Self {
        ObjectStoreManager::new(config)
    }
}

// Implement ConfigReloader for hot reload support
impl ConfigReloader<ObjectStoreManagerConfig> for ObjectStoreManager {
    fn reload_config(&mut self, config: ObjectStoreManagerConfig) -> Result<()> {
        // Update configuration
        self.config = config;

        // Clear cache to force recreation with new configuration
        self.cache.clear();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_config() -> ObjectStoreManagerConfig {
        ObjectStoreManagerConfig {
            stores: vec![TypeOptions {
                type_name: "AwsS3ObjectStore".to_string(),
                options: json!({
                    "bucket": "test-bucket",
                    "region": "us-east-1"
                }),
            }],
            defaults: Default::default(),
        }
    }

    #[test]
    fn test_store_manager_creation() {
        let config = create_test_config();
        let _manager = ObjectStoreManager::new(config);
    }

    #[test]
    fn test_default_options() {
        let defaults = DefaultOptions::default();
        assert_eq!(defaults.concurrency, 4);
        assert_eq!(defaults.part_size, 8 * 1024 * 1024);
        assert_eq!(defaults.multipart_threshold, 100 * 1024 * 1024);
    }
}
