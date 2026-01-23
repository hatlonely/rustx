//! 对象存储管理器
//!
//! 提供对象存储实例的缓存、检索和高级文件操作功能。
//! 支持在本地和远程存储之间进行文件复制、列举、删除等操作。

use anyhow::{anyhow, Result};
use crate::cfg::{create_trait_from_type_options, ConfigReloader, TypeOptions};
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

/// 对象存储管理器配置
///
/// 定义了多个对象存储实例的配置和默认操作参数。
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ObjectStoreManagerConfig {
    /// 对象存储配置列表
    ///
    /// 每个配置描述一个对象存储实例（S3、OSS、GCS 等）。
    pub stores: Vec<TypeOptions>,

    /// 操作的默认选项
    ///
    /// 当操作选项中未指定具体值时，使用这些默认值。
    pub defaults: DefaultOptions,
}

/// 操作的默认选项
///
/// 定义了并发数、分片大小、分片上传阈值等默认参数。
#[derive(Debug, Clone, Deserialize, Serialize, SmartDefault)]
#[serde(default)]
pub struct DefaultOptions {
    /// 并发操作数量（默认：4）
    #[default = 4]
    pub concurrency: usize,

    /// 分片上传的分片大小（默认：8MB）
    #[default = 8388608]
    pub part_size: usize,

    /// 启用分片上传的阈值（默认：100MB）
    #[default = 104857600]
    pub multipart_threshold: u64,
}

/// 对象存储管理器
///
/// 负责管理和缓存多个 `ObjectStore` 实例，提供类似 Unix shell 的文件操作接口：
/// - `cp`: 在本地和远程之间复制文件/目录
/// - `ls`: 列举远程对象
/// - `rm`: 删除远程对象
/// - `stat`: 获取对象元数据
///
/// # 核心功能
///
/// - **实例缓存**：缓存已创建的 `ObjectStore` 实例，避免重复创建
/// - **智能路由**：根据 URI 自动选择合适的存储后端
/// - **高级操作**：支持本地↔远程、远程↔远程的文件传输
///
/// # 线程安全
///
/// 管理器本身不是线程安全的，但返回的 `ObjectStore` 实例是 `Arc` 包装的，可以跨线程共享。
pub struct ObjectStoreManager {
    /// 管理器配置
    config: ObjectStoreManagerConfig,

    /// 对象存储实例缓存
    ///
    /// 键为 `(Provider, Bucket)`，值为 `ObjectStore` 实例。
    cache: HashMap<(Provider, String), Arc<dyn ObjectStore>>,
}

impl ObjectStoreManager {
    /// 创建一个新的对象存储管理器
    ///
    /// # 参数
    ///
    /// - `config`: 管理器配置，包含存储实例列表和默认选项
    ///
    /// # 返回值
    ///
    /// 返回一个初始化完成的管理器实例。
    ///
    /// # 行为说明
    ///
    /// - 自动注册所有 `ObjectStore` 实现类型
    /// - 初始化空的实例缓存
    pub fn new(config: ObjectStoreManagerConfig) -> Self {
        // 注册所有 ObjectStore 实现类型
        register_object_store();

        Self {
            config,
            cache: HashMap::new(),
        }
    }

    /// 获取或创建指定 URI 的对象存储实例
    ///
    /// 根据 URI 中的 provider 和 bucket 查找对应的存储配置，
    /// 创建并缓存 `ObjectStore` 实例。
    ///
    /// # 参数
    ///
    /// - `uri`: 对象存储 URI，包含 provider、bucket 等信息
    ///
    /// # 返回值
    ///
    /// - `Ok(Arc<dyn ObjectStore>)`: 返回存储实例的共享引用
    /// - `Err(anyhow::Error)`: 未找到匹配的存储配置或创建失败
    ///
    /// # 行为说明
    ///
    /// - 首先检查缓存，如果已存在则直接返回
    /// - 缓存未命中时，查找匹配的存储配置并创建新实例
    /// - 新实例会被自动缓存，供后续使用
    pub fn get_store(&mut self, uri: &OssUri) -> Result<Arc<dyn ObjectStore>> {
        // 检查缓存
        let cache_key = (uri.provider.clone(), uri.bucket.clone());
        if let Some(store) = self.cache.get(&cache_key) {
            return Ok(Arc::clone(store));
        }

        // 查找匹配的存储配置
        let store_config = self
            .find_store_config(&uri.provider, &uri.bucket)
            .ok_or_else(|| anyhow!("No store configured for provider:{}, bucket:{}", uri.provider.scheme(), uri.bucket))?;

        // 创建存储实例
        let store = create_store_from_config(store_config)?;
        let store = Arc::from(store);

        // 缓存实例
        self.cache.insert(cache_key, Arc::clone(&store));

        Ok(store)
    }

    /// 根据 provider 和 bucket 名称查找存储配置
    ///
    /// # 参数
    ///
    /// - `provider`: 存储提供商（S3、OSS、GCS）
    /// - `bucket`: 存储桶名称
    ///
    /// # 返回值
    ///
    /// - `Some(&TypeOptions)`: 找到匹配的配置
    /// - `None`: 未找到匹配的配置
    ///
    /// # 匹配规则
    ///
    /// 配置的 `type_name` 必须能映射到指定的 `provider`
    /// 配置的 `bucket` 选项必须与指定的 `bucket` 相同
    fn find_store_config(&self, provider: &Provider, bucket: &str) -> Option<&TypeOptions> {
        self.config.stores.iter().find(|store| {
            // 检查 provider 是否匹配
            let type_matches = type_name_to_provider(&store.type_name) == Some(*provider);

            // 检查 bucket 是否匹配
            let bucket_matches = store
                .options
                .get("bucket")
                .and_then(|v| v.as_str())
                .map(|b| b == bucket)
                .unwrap_or(false);

            type_matches && bucket_matches
        })
    }

    /// 获取配置的默认选项
    ///
    /// # 返回值
    ///
    /// 返回默认选项的引用，包含并发数、分片大小等参数。
    pub fn defaults(&self) -> &DefaultOptions {
        &self.config.defaults
    }

    // ============ 高级文件操作 ============

    /// 在本地和远程位置之间复制文件/目录
    ///
    /// 自动检测复制方向并执行相应操作：
    /// - **本地 → 远程**：上传
    /// - **远程 → 本地**：下载
    /// - **远程 → 远程**：远程复制
    ///
    /// # 参数
    ///
    /// - `from`: 源路径，可以是本地路径或远程 URI
    /// - `to`: 目标路径，可以是本地路径或远程 URI
    /// - `options`: 复制选项，控制并发、覆盖、递归等行为
    ///
    /// # 返回值
    ///
    /// - `Ok(CpResult)`: 复制完成，返回统计结果
    /// - `Err(anyhow::Error)`: 复制失败
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// // 上传单个文件
    /// manager.cp("./file.txt", "s3://bucket/file.txt", CpOptions::default()).await?;
    ///
    /// // 下载单个文件
    /// manager.cp("s3://bucket/file.txt", "./file.txt", CpOptions::default()).await?;
    ///
    /// // 递归上传目录
    /// manager.cp("./dir/", "s3://bucket/prefix/", CpOptions {
    ///     recursive: true,
    ///     concurrency: Some(8),
    ///     ..Default::default()
    /// }).await?;
    ///
    /// // 远程到远程复制
    /// manager.cp("s3://bucket1/file.txt", "s3://bucket2/file.txt", CpOptions::default()).await?;
    /// ```
    ///
    /// # 行为说明
    ///
    /// - 至少有一个路径必须是远程 URI（不能本地到本地）
    /// - 目录操作必须设置 `recursive: true`
    /// - 远程复制使用流式传输，避免中间文件
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

    /// 列举远程 URI 下的对象
    ///
    /// 返回指定前缀下的所有对象元数据列表。
    ///
    /// # 参数
    ///
    /// - `uri`: 远程 URI，格式如 `s3://bucket/prefix/`
    /// - `options`: 列举选项，可限制返回数量
    ///
    /// # 返回值
    ///
    /// - `Ok(Vec<ObjectMeta>)`: 对象元数据列表
    /// - `Err(anyhow::Error)`: 列举失败
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// // 列举所有对象
    /// let objects = manager.ls("s3://bucket/prefix/", LsOptions::default()).await?;
    /// for obj in objects {
    ///     println!("{}: {} bytes", obj.key, obj.size);
    /// }
    ///
    /// // 限制返回数量
    /// let objects = manager.ls("s3://bucket/", LsOptions { max_keys: Some(100) }).await?;
    /// ```
    ///
    /// # 行为说明
    ///
    /// - URI 中的 key 部分被用作前缀过滤
    /// - 如果 key 为空，列举整个 bucket
    /// - 返回结果按字典序排列（取决于后端实现）
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

    /// 删除远程 URI 处的对象
    ///
    /// 可以删除单个对象或递归删除前缀下的所有对象。
    ///
    /// # 参数
    ///
    /// - `uri`: 要删除的对象 URI
    /// - `options`: 删除选项，控制递归、过滤等行为
    ///
    /// # 返回值
    ///
    /// - `Ok(RmResult)`: 删除完成，返回统计结果
    /// - `Err(anyhow::Error)`: 删除失败
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// // 删除单个文件
    /// manager.rm("s3://bucket/file.txt", RmOptions::default()).await?;
    ///
    /// // 递归删除目录/前缀
    /// manager.rm("s3://bucket/prefix/", RmOptions {
    ///     recursive: true,
    ///     ..Default::default()
    /// }).await?;
    ///
    /// // 删除并过滤文件
    /// manager.rm("s3://bucket/logs/", RmOptions {
    ///     recursive: true,
    ///     include: Some("*.log".to_string()),
    /// }).await?;
    /// ```
    ///
    /// # 行为说明
    ///
    /// - URI 以 `/` 结尾时被视为目录/前缀
    /// - 删除目录/前缀必须设置 `recursive: true`
    /// - 支持基于 glob 模式的文件过滤
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

    /// 获取远程对象的元数据
    ///
    /// 查询对象的大小、最后修改时间、ETag 等信息，不下载对象内容。
    ///
    /// # 参数
    ///
    /// - `uri`: 对象的远程 URI
    ///
    /// # 返回值
    ///
    /// - `Ok(ObjectMeta)`: 对象的元数据，包含 key、size、last_modified 等字段
    /// - `Err(anyhow::Error)`: 对象不存在或查询失败
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let meta = manager.stat("s3://bucket/file.txt").await?;
    /// println!("Size: {} bytes", meta.size);
    /// println!("Last Modified: {:?}", meta.last_modified);
    /// println!("ETag: {}", meta.etag);
    /// ```
    ///
    /// # 行为说明
    ///
    /// - 这是一个轻量级操作，不消耗数据传输流量
    /// - 可用于检查对象是否存在
    /// - 如果对象不存在，返回错误
    pub async fn stat(&mut self, uri: &str) -> Result<ObjectMeta> {
        let parsed_uri = OssUri::parse(uri)?;
        let store = self.get_store(&parsed_uri)?;

        store
            .head_object(&parsed_uri.key)
            .await?
            .ok_or_else(|| anyhow!("Object not found: {}", parsed_uri.key))
    }

    // ============ 私有实现方法 ============

    /// 上传本地文件/目录到远程存储
    ///
    /// # 参数
    ///
    /// - `local_path`: 本地路径
    /// - `remote_uri`: 目标远程 URI
    /// - `options`: 上传选项
    /// - `store`: 对象存储实例
    /// - `concurrency`: 并发数
    /// - `part_size`: 分片大小
    /// - `multipart_threshold`: 分片上传阈值
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

    /// 上传单个文件
    ///
    /// # 参数
    ///
    /// - `local_path`: 本地文件路径
    /// - `remote_uri`: 目标远程 URI
    /// - `options`: 上传选项
    /// - `store`: 对象存储实例
    /// - `part_size`: 分片大小
    /// - `multipart_threshold`: 分片上传阈值
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
        };

        store.put_file(&key, local_path, put_options).await?;

        Ok(CpResult::single_success(file_size))
    }

    /// 上传目录
    ///
    /// 递归上传目录中的所有文件到远程存储。
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

    /// 下载远程文件/目录到本地
    ///
    /// # 参数
    ///
    /// - `remote_uri`: 源远程 URI
    /// - `local_path`: 本地目标路径
    /// - `options`: 下载选项
    /// - `store`: 对象存储实例
    /// - `concurrency`: 并发数
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

    /// 下载单个文件
    ///
    /// # 参数
    ///
    /// - `remote_uri`: 源远程 URI
    /// - `local_path`: 本地目标路径
    /// - `options`: 下载选项
    /// - `store`: 对象存储实例
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
        };

        store
            .get_file(&remote_uri.key, &dest_path, get_options)
            .await?;

        Ok(CpResult::single_success(meta.size))
    }

    /// 下载目录
    ///
    /// 批量下载前缀下的所有对象到本地目录。支持文件过滤。
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

    /// 在远程位置之间复制
    ///
    /// 使用流式传输在不同存储桶或提供商之间复制文件。
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

    /// 使用流式传输在远程位置之间复制单个文件
    ///
    /// 通过管道连接源存储的下载和目标存储的上传，实现零中间文件的远程复制。
    async fn copy_remote_file(
        &self,
        src_uri: &OssUri,
        dst_uri: &OssUri,
        options: &CpOptions,
        src_store: &dyn ObjectStore,
        dst_store: &dyn ObjectStore,
    ) -> Result<CpResult> {
        use super::object_store_types::{GetStreamOptions, PutStreamOptions};

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

        // Get source file size
        let src_meta = src_store
            .head_object(&src_uri.key)
            .await?
            .ok_or_else(|| anyhow!("Source file not found: {}", src_uri.key))?;
        let size = src_meta.size;

        // Use a pipe to connect get_stream and put_stream
        // Buffer size: 16MB to allow some buffering between download and upload
        let (reader, writer) = tokio::io::duplex(16 * 1024 * 1024);

        // Prepare options
        let get_options = GetStreamOptions::default();
        let put_options = PutStreamOptions {
            content_type: src_meta.content_type,
            ..Default::default()
        };

        // Run download and upload concurrently
        // Using try_join to run both operations in parallel on the same task
        let src_key = src_uri.key.clone();
        let download_future = src_store.get_stream(&src_key, Box::new(writer), get_options);
        let upload_future = dst_store.put_stream(&dst_key, Box::new(reader), Some(size), put_options);

        let (download_result, upload_result) = tokio::try_join!(
            async { download_future.await.map_err(|e| anyhow!("Download failed: {}", e)) },
            async { upload_future.await.map_err(|e| anyhow!("Upload failed: {}", e)) }
        )?;

        // Both operations succeeded
        let _ = download_result; // bytes downloaded
        let _ = upload_result;   // unit

        Ok(CpResult::single_success(size))
    }

    /// 在远程位置之间复制目录
    ///
    /// 递归复制前缀下的所有对象。
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

            // Copy the object using streaming
            match self
                .copy_single_object(src_store, dst_store, &obj.key, &dst_key, obj.size)
                .await
            {
                Ok(size) => {
                    result.success_count += 1;
                    result.total_bytes += size;
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

    /// 使用流式传输在远程位置之间复制单个对象
    ///
    /// # 参数
    ///
    /// - `src_store`: 源存储实例
    /// - `dst_store`: 目标存储实例
    /// - `src_key`: 源对象键
    /// - `dst_key`: 目标对象键
    /// - `size`: 对象大小（字节）
    async fn copy_single_object(
        &self,
        src_store: &dyn ObjectStore,
        dst_store: &dyn ObjectStore,
        src_key: &str,
        dst_key: &str,
        size: u64,
    ) -> Result<u64> {
        use super::object_store_types::{GetStreamOptions, PutStreamOptions};

        // Use a pipe to connect get_stream and put_stream
        let (reader, writer) = tokio::io::duplex(16 * 1024 * 1024);

        let get_options = GetStreamOptions::default();
        let put_options = PutStreamOptions::default();

        let src_key = src_key.to_string();
        let download_future = src_store.get_stream(&src_key, Box::new(writer), get_options);
        let upload_future = dst_store.put_stream(dst_key, Box::new(reader), Some(size), put_options);

        tokio::try_join!(
            async { download_future.await.map_err(|e| anyhow!("Download failed: {}", e)) },
            async { upload_future.await.map_err(|e| anyhow!("Upload failed: {}", e)) }
        )?;

        Ok(size)
    }

    /// 删除单个对象
    ///
    /// # 参数
    ///
    /// - `uri`: 要删除的对象 URI
    /// - `store`: 对象存储实例
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

    /// 递归删除对象
    ///
    /// 列举前缀下的所有对象，应用过滤规则后批量删除。
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

/// 从 TypeOptions 配置创建 ObjectStore 实例
///
/// # 参数
///
/// - `config`: 类型配置，包含类型名称和选项
///
/// # 返回值
///
/// - `Ok(Box<dyn ObjectStore>)`: 创建的存储实例
/// - `Err(anyhow::Error)`: 创建失败
fn create_store_from_config(config: &TypeOptions) -> Result<Box<dyn ObjectStore>> {
    create_trait_from_type_options::<dyn ObjectStore>(config)
        .map_err(|e| anyhow!("Failed to create ObjectStore: {}", e))
}

/// 将类型名称映射到 Provider
///
/// # 参数
///
/// - `type_name`: ObjectStore 实现的类型名称
///
/// # 返回值
///
/// - `Some(Provider)`: 对应的存储提供商
/// - `None`: 未知的类型名称
///
/// # 映射关系
///
/// - `"AwsS3ObjectStore"` → `Provider::S3`
/// - `"AliOssObjectStore"` → `Provider::Oss`
/// - `"GcpGcsObjectStore"` → `Provider::Gcs`
fn type_name_to_provider(type_name: &str) -> Option<Provider> {
    match type_name {
        "AwsS3ObjectStore" => Some(Provider::S3),
        "AliOssObjectStore" => Some(Provider::Oss),
        "GcpGcsObjectStore" => Some(Provider::Gcs),
        _ => None,
    }
}

// 为配置系统集成实现 From trait
impl From<ObjectStoreManagerConfig> for ObjectStoreManager {
    fn from(config: ObjectStoreManagerConfig) -> Self {
        ObjectStoreManager::new(config)
    }
}

// 为热重载支持实现 ConfigReloader
impl ConfigReloader<ObjectStoreManagerConfig> for ObjectStoreManager {
    fn reload_config(&mut self, config: ObjectStoreManagerConfig) -> Result<()> {
        // 更新配置
        self.config = config;

        // 清空缓存，强制使用新配置重新创建实例
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
