use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::StreamExt;
use glob::Pattern;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::oss::{
    DirectoryTransferProgress, DirectoryTransferResult, FailedFile, GetDirectoryOptions,
    GetFileOptions, GetObjectOptions, GetStreamOptions, ObjectMeta, ObjectStoreError,
    PutDirectoryOptions, PutFileOptions, PutObjectOptions, PutStreamOptions,
};

/// 对象存储统一接口
///
/// 该 trait 定义了对象存储的标准操作接口，支持多种后端实现（如 S3、阿里云 OSS、本地文件系统等）。
/// 所有实现都必须是线程安全的（`Send + Sync`），可以在多线程环境中安全使用。
///
/// # 接口分层
///
/// - **基础 CRUD**：`put_object`、`get_object`、`delete_object`、`head_object`、`list_objects`
///   - 需要实现者提供具体实现
///   - 操作单个对象，数据以 `Bytes` 形式传递（适合小文件）
///
/// - **流式接口**：`put_stream`、`get_stream`
///   - 需要实现者提供具体实现
///   - 支持大文件的流式传输，避免将整个文件加载到内存
///
/// - **文件操作**：`put_file`、`get_file`
///   - 提供默认实现，基于流式接口
///   - 直接与本地文件系统交互
///
/// - **目录操作**：`put_directory`、`get_directory`
///   - 提供默认实现，基于文件操作接口
///   - 支持批量并发传输，带进度回调
///
/// # 线程安全
///
/// 所有方法都是 `async` 的，可以在异步运行时中安全调用。
/// trait 约束 `Send + Sync` 确保实现可以跨线程共享。
#[async_trait]
pub trait ObjectStore: Send + Sync {
    // === 基础 CRUD ===

    /// 上传对象到存储后端
    ///
    /// 将内存中的数据上传到指定的对象键。适用于小文件或已经在内存中的数据。
    /// 对于大文件，建议使用 `put_stream` 或 `put_file` 方法。
    ///
    /// # 参数
    ///
    /// - `key`: 对象的唯一标识符（键），通常是类似路径的字符串（如 `"folder/file.txt"`）
    /// - `value`: 要上传的数据，以 `Bytes` 形式提供
    /// - `options`: 上传选项，包括 `content_type`、自定义元数据等
    ///
    /// # 返回值
    ///
    /// - `Ok(())`: 上传成功
    /// - `Err(ObjectStoreError)`: 上传失败，可能的错误包括：
    ///   - 网络错误
    ///   - 权限不足
    ///   - 存储配额超限
    ///
    /// # 行为说明
    ///
    /// - 如果对象已存在，将被覆盖
    /// - 上传操作是原子的，要么完全成功，要么完全失败
    async fn put_object(
        &self,
        key: &str,
        value: Bytes,
        options: PutObjectOptions,
    ) -> Result<(), ObjectStoreError>;

    /// 从存储后端获取对象
    ///
    /// 下载指定对象的全部内容到内存中。适用于小文件或需要完整数据的场景。
    /// 对于大文件，建议使用 `get_stream` 或 `get_file` 方法以避免内存溢出。
    ///
    /// # 参数
    ///
    /// - `key`: 要获取的对象键
    /// - `options`: 获取选项，包括范围请求（`range`）等
    ///
    /// # 返回值
    ///
    /// - `Ok(Bytes)`: 对象的完整内容
    /// - `Err(ObjectStoreError)`: 获取失败，可能的错误包括：
    ///   - `ObjectStoreError::NotFound`: 对象不存在
    ///   - 网络错误
    ///   - 权限不足
    ///
    /// # 行为说明
    ///
    /// - 如果指定了 `range` 选项，只返回指定范围的数据
    /// - 整个对象内容会被加载到内存中，需注意内存使用
    async fn get_object(&self, key: &str, options: GetObjectOptions)
        -> Result<Bytes, ObjectStoreError>;

    /// 删除存储后端中的对象
    ///
    /// 从存储中永久删除指定的对象。
    ///
    /// # 参数
    ///
    /// - `key`: 要删除的对象键
    ///
    /// # 返回值
    ///
    /// - `Ok(())`: 删除成功
    /// - `Err(ObjectStoreError)`: 删除失败，可能的错误包括：
    ///   - 网络错误
    ///   - 权限不足
    ///
    /// # 行为说明
    ///
    /// - 如果对象不存在，大多数实现会返回 `Ok(())`（幂等操作）
    /// - 删除操作是不可逆的，请谨慎使用
    /// - 不支持删除"目录"，对象存储中的目录只是键前缀的逻辑概念
    async fn delete_object(&self, key: &str) -> Result<(), ObjectStoreError>;

    /// 获取对象元数据（不下载内容）
    ///
    /// 获取对象的元信息，包括大小、最后修改时间、ETag、Content-Type 等。
    /// 此操作不会下载对象内容，适合用于检查对象是否存在或获取对象属性。
    ///
    /// # 参数
    ///
    /// - `key`: 要查询的对象键
    ///
    /// # 返回值
    ///
    /// - `Ok(Some(ObjectMeta))`: 对象存在，返回元数据
    /// - `Ok(None)`: 对象不存在
    /// - `Err(ObjectStoreError)`: 查询失败，可能的错误包括：
    ///   - 网络错误
    ///   - 权限不足
    ///
    /// # 行为说明
    ///
    /// - 这是一个轻量级操作，不消耗数据传输流量
    /// - 可用于实现"存在性检查"而无需下载整个对象
    /// - 返回的 `ObjectMeta` 包含 `key`、`size`、`last_modified`、`etag` 等字段
    async fn head_object(&self, key: &str) -> Result<Option<ObjectMeta>, ObjectStoreError>;

    /// 列出存储桶中的对象
    ///
    /// 获取符合指定前缀的所有对象的元数据列表。内部会自动处理分页，
    /// 循环调用后端 API 直到获取所有结果或达到最大数量限制。
    ///
    /// # 参数
    ///
    /// - `prefix`: 可选的前缀过滤器
    ///   - `Some("folder/")`: 只列出以 `"folder/"` 开头的对象
    ///   - `None`: 列出所有对象
    /// - `max_keys`: 可选的最大返回数量
    ///   - `Some(100)`: 最多返回 100 个对象
    ///   - `None`: 返回所有匹配的对象
    ///
    /// # 返回值
    ///
    /// - `Ok(Vec<ObjectMeta>)`: 对象元数据列表
    /// - `Err(ObjectStoreError)`: 列举失败，可能的错误包括：
    ///   - 网络错误
    ///   - 权限不足
    ///
    /// # 行为说明
    ///
    /// - 返回结果的顺序取决于后端实现（通常是字典序）
    /// - 对象存储没有真正的目录概念，前缀过滤是基于字符串匹配
    /// - 当对象数量很大时，此操作可能耗时较长
    /// - 结果不包含"目录"本身，只包含实际的对象
    async fn list_objects(
        &self,
        prefix: Option<&str>,
        max_keys: Option<usize>,
    ) -> Result<Vec<ObjectMeta>, ObjectStoreError>;

    // === 流式接口 ===

    /// 流式上传对象
    ///
    /// 通过异步读取器将数据流式上传到存储后端。适用于大文件上传或数据来源于流的场景。
    /// 实现会根据数据大小和配置自动选择直接上传或分片上传策略。
    ///
    /// # 参数
    ///
    /// - `key`: 对象键，目标存储路径
    /// - `reader`: 异步读取器，提供要上传的数据流
    /// - `size`: 可选的数据大小（字节数）
    ///   - `Some(size)`: 提前知道大小，用于优化上传策略
    ///     - 小于阈值：使用单次上传，效率更高
    ///     - 大于阈值：使用分片上传
    ///   - `None`: 大小未知，强制使用分片上传策略
    /// - `options`: 上传选项，包括：
    ///   - `content_type`: 对象的 MIME 类型
    ///   - `metadata`: 自定义元数据
    ///   - `multipart_threshold`: 启用分片上传的阈值（字节）
    ///   - `part_size`: 分片大小（字节）
    ///   - `multipart_concurrency`: 分片上传的并发数
    ///
    /// # 返回值
    ///
    /// - `Ok(())`: 上传成功
    /// - `Err(ObjectStoreError)`: 上传失败
    ///
    /// # 行为说明
    ///
    /// - 如果 `size` 为 `None` 或超过 `multipart_threshold`，将使用分片上传
    /// - 分片上传允许并发上传多个分片，提高大文件传输效率
    /// - 如果上传失败，分片上传会尝试清理已上传的分片
    /// - 建议对于大小未知的流，始终传 `None` 以避免内存问题
    async fn put_stream(
        &self,
        key: &str,
        reader: Box<dyn AsyncRead + Send + Unpin>,
        size: Option<u64>,
        options: PutStreamOptions,
    ) -> Result<(), ObjectStoreError>;

    /// 流式下载对象
    ///
    /// 将对象内容流式写入到异步写入器中。适用于大文件下载或需要将数据写入流的场景。
    /// 数据会被分块传输，不会一次性加载到内存中。
    ///
    /// # 参数
    ///
    /// - `key`: 要下载的对象键
    /// - `writer`: 异步写入器，数据将被写入此目标
    /// - `options`: 下载选项，包括：
    ///   - `range`: 可选的范围请求，用于部分下载
    ///
    /// # 返回值
    ///
    /// - `Ok(u64)`: 下载成功，返回实际传输的字节数
    /// - `Err(ObjectStoreError)`: 下载失败，可能的错误包括：
    ///   - `ObjectStoreError::NotFound`: 对象不存在
    ///   - 网络错误
    ///   - 写入器错误
    ///
    /// # 行为说明
    ///
    /// - 数据以流的方式传输，内存占用恒定
    /// - 如果指定了 `range`，只下载指定范围的数据
    /// - 写入器会被自动刷新（flush）
    /// - 适合下载大文件或将数据流式传输到其他目的地
    async fn get_stream(
        &self,
        key: &str,
        writer: Box<dyn AsyncWrite + Send + Unpin>,
        options: GetStreamOptions,
    ) -> Result<u64, ObjectStoreError>;

    // === 文件操作（默认实现） ===

    /// 上传本地文件到存储后端
    ///
    /// 将本地文件系统中的文件上传到对象存储。提供默认实现，基于 `put_stream` 方法。
    ///
    /// # 参数
    ///
    /// - `key`: 目标对象键
    /// - `local_path`: 本地文件路径
    /// - `options`: 上传选项，包括：
    ///   - `content_type`: 对象的 MIME 类型（可选）
    ///   - `metadata`: 自定义元数据（可选）
    ///   - `multipart_threshold`: 启用分片上传的阈值
    ///   - `part_size`: 分片大小
    ///   - `multipart_concurrency`: 分片上传并发数
    ///
    /// # 返回值
    ///
    /// - `Ok(())`: 上传成功
    /// - `Err(ObjectStoreError)`: 上传失败，可能的错误包括：
    ///   - 本地文件不存在
    ///   - 文件读取权限不足
    ///   - 网络错误
    ///
    /// # 默认实现
    ///
    /// 默认实现会：
    /// 1. 获取文件大小
    /// 2. 打开文件并调用 `put_stream` 进行流式上传
    async fn put_file(
        &self,
        key: &str,
        local_path: &Path,
        options: PutFileOptions,
    ) -> Result<(), ObjectStoreError> {
        let metadata = tokio::fs::metadata(local_path).await?;
        let file_size = metadata.len();

        // 小文件：使用流式上传
        let file = tokio::fs::File::open(local_path).await?;
        let stream_options = PutStreamOptions {
            content_type: options.content_type.clone(),
            metadata: options.metadata.clone(),
            multipart_threshold: options.multipart_threshold,
            part_size: options.part_size,
            multipart_concurrency: options.multipart_concurrency,
        };
        self.put_stream(key, Box::new(file), Some(file_size), stream_options)
            .await
    }

    /// 下载对象到本地文件
    ///
    /// 将对象存储中的对象下载到本地文件系统。提供默认实现，基于 `get_stream` 方法。
    ///
    /// # 参数
    ///
    /// - `key`: 要下载的对象键
    /// - `local_path`: 本地目标文件路径
    /// - `options`: 下载选项，包括：
    ///   - `overwrite`: 是否覆盖已存在的文件（默认 `false`）
    ///
    /// # 返回值
    ///
    /// - `Ok(())`: 下载成功
    /// - `Err(ObjectStoreError)`: 下载失败，可能的错误包括：
    ///   - `ObjectStoreError::NotFound`: 对象不存在
    ///   - `ObjectStoreError::FileExists`: 本地文件已存在且 `overwrite` 为 `false`
    ///   - 网络错误
    ///   - 本地文件系统错误
    ///
    /// # 默认实现
    ///
    /// 默认实现会：
    /// 1. 检查本地文件是否存在（根据 `overwrite` 选项）
    /// 2. 创建父目录（如果不存在）
    /// 3. 创建目标文件并调用 `get_stream` 进行流式下载
    async fn get_file(
        &self,
        key: &str,
        local_path: &Path,
        options: GetFileOptions,
    ) -> Result<(), ObjectStoreError> {
        // 检查文件是否存在
        if !options.overwrite && local_path.exists() {
            return Err(ObjectStoreError::FileExists {
                path: local_path.display().to_string(),
            });
        }

        // 确保父目录存在
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 创建文件
        let file = tokio::fs::File::create(local_path).await?;

        let stream_options = GetStreamOptions {
            range: None,
        };

        self.get_stream(key, Box::new(file), stream_options).await?;
        Ok(())
    }

    // === 目录操作（默认实现） ===

    /// 上传本地目录到存储后端
    ///
    /// 将本地目录中的所有文件批量上传到对象存储。支持并发上传、进度回调、
    /// 文件过滤等功能。提供默认实现，基于 `put_file` 方法。
    ///
    /// # 参数
    ///
    /// - `prefix`: 目标前缀，所有文件将上传到此前缀下
    ///   - 例如：`prefix = "backup/2024/"` + 文件 `data/file.txt` -> `"backup/2024/data/file.txt"`
    /// - `local_dir`: 本地目录路径
    /// - `options`: 上传选项，包括：
    ///   - `recursive`: 是否递归上传子目录（默认 `true`）
    ///   - `concurrency`: 并发上传数量
    ///   - `include_patterns`: 包含的文件模式（glob 格式）
    ///   - `exclude_patterns`: 排除的文件模式（glob 格式）
    ///   - `progress_callback`: 进度回调函数
    ///   - `multipart_threshold`/`part_size`/`multipart_concurrency`: 分片上传参数
    ///
    /// # 返回值
    ///
    /// - `Ok(DirectoryTransferResult)`: 上传完成，返回统计结果：
    ///   - `success_count`: 成功上传的文件数
    ///   - `failed_count`: 上传失败的文件数
    ///   - `total_bytes`: 成功传输的总字节数
    ///   - `failed_files`: 失败文件的详细信息
    /// - `Err(ObjectStoreError)`: 上传失败（目录不存在等致命错误）
    ///
    /// # 行为说明
    ///
    /// - 使用 `buffer_unordered` 进行并发上传，顺序不保证
    /// - 单个文件失败不会中断整个目录上传
    /// - 进度回调会在每个文件完成时触发
    /// - 文件过滤：先应用 `exclude_patterns`，再应用 `include_patterns`
    async fn put_directory(
        &self,
        prefix: &str,
        local_dir: &Path,
        options: PutDirectoryOptions,
    ) -> Result<DirectoryTransferResult, ObjectStoreError> {
        // 检查目录是否存在
        let metadata = tokio::fs::metadata(local_dir).await.map_err(|_| {
            ObjectStoreError::DirectoryNotFound {
                path: local_dir.display().to_string(),
            }
        })?;

        if !metadata.is_dir() {
            return Err(ObjectStoreError::NotADirectory {
                path: local_dir.display().to_string(),
            });
        }

        // 收集所有要上传的文件
        let files = collect_files(local_dir, &options).await?;
        let total_files = files.len();
        let total_bytes: u64 = files.iter().map(|(_, size)| size).sum();

        // 进度跟踪
        let completed_files = Arc::new(AtomicUsize::new(0));
        let transferred_bytes = Arc::new(AtomicU64::new(0));
        let _failed_files: Arc<tokio::sync::Mutex<Vec<FailedFile>>> = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        // 并发上传
        let results: Vec<(String, Result<u64, String>)> =
            futures::stream::iter(files.into_iter().map(|(rel_path, file_size)| {
                let local_dir = local_dir.to_path_buf();
                let prefix = prefix.to_string();
                let completed_files = completed_files.clone();
                let transferred_bytes = transferred_bytes.clone();
                let progress_callback = options.progress_callback.clone();
                let file_options = PutFileOptions {
                    content_type: None,
                    metadata: None,
                    multipart_threshold: options.multipart_threshold,
                    part_size: options.part_size,
                    multipart_concurrency: options.multipart_concurrency,
                };

                async move {
                    let local_path = local_dir.join(&rel_path);
                    let key = if prefix.is_empty() {
                        rel_path.clone()
                    } else if prefix.ends_with('/') {
                        format!("{}{}", prefix, rel_path)
                    } else {
                        format!("{}/{}", prefix, rel_path)
                    };

                    let result = match self.put_file(&key, &local_path, file_options).await {
                        Ok(()) => {
                            transferred_bytes.fetch_add(file_size, Ordering::SeqCst);
                            Ok(file_size)
                        }
                        Err(e) => Err(e.to_string()),
                    };

                    let completed = completed_files.fetch_add(1, Ordering::SeqCst) + 1;

                    // 更新进度
                    if let Some(ref callback) = progress_callback {
                        callback.on_progress(&DirectoryTransferProgress {
                            current_file: rel_path.clone(),
                            completed_files: completed,
                            total_files,
                            transferred_bytes: transferred_bytes.load(Ordering::SeqCst),
                            total_bytes,
                        });
                        callback.on_file_complete(
                            &rel_path,
                            result.is_ok(),
                            result.as_ref().err().map(|s| s.as_str()),
                        );
                    }

                    (rel_path, result)
                }
            }))
            .buffer_unordered(options.concurrency)
            .collect()
            .await;

        // 统计结果
        let mut success_count = 0;
        let mut failed_count = 0;
        let mut total_transferred: u64 = 0;
        let mut failed_list = Vec::new();

        for (path, result) in results {
            match result {
                Ok(size) => {
                    success_count += 1;
                    total_transferred += size;
                }
                Err(error) => {
                    failed_count += 1;
                    failed_list.push(FailedFile { path, error });
                }
            }
        }

        Ok(DirectoryTransferResult {
            success_count,
            failed_count,
            total_bytes: total_transferred,
            failed_files: failed_list,
        })
    }

    /// 下载对象前缀到本地目录
    ///
    /// 批量下载指定前缀下的所有对象到本地目录。支持并发下载和进度回调。
    /// 提供默认实现，基于 `list_objects` 和 `get_file` 方法。
    ///
    /// # 参数
    ///
    /// - `prefix`: 要下载的对象前缀
    ///   - 例如：`prefix = "backup/2024/"` 将下载所有以此开头的对象
    /// - `local_dir`: 本地目标目录
    /// - `options`: 下载选项，包括：
    ///   - `overwrite`: 是否覆盖已存在的本地文件（默认 `false`）
    ///   - `concurrency`: 并发下载数量
    ///   - `progress_callback`: 进度回调函数
    ///
    /// # 返回值
    ///
    /// - `Ok(DirectoryTransferResult)`: 下载完成，返回统计结果：
    ///   - `success_count`: 成功下载的文件数
    ///   - `failed_count`: 下载失败的文件数
    ///   - `total_bytes`: 成功传输的总字节数
    ///   - `failed_files`: 失败文件的详细信息
    /// - `Err(ObjectStoreError)`: 下载失败（列举失败等致命错误）
    ///
    /// # 行为说明
    ///
    /// - 首先调用 `list_objects` 获取所有匹配的对象
    /// - 使用 `buffer_unordered` 进行并发下载，顺序不保证
    /// - 单个文件失败不会中断整个目录下载
    /// - 本地文件路径 = `local_dir` + 对象键去掉前缀后的部分
    /// - 会自动创建必要的本地子目录
    async fn get_directory(
        &self,
        prefix: &str,
        local_dir: &Path,
        options: GetDirectoryOptions,
    ) -> Result<DirectoryTransferResult, ObjectStoreError> {
        // 列出所有对象
        let objects = self.list_objects(Some(prefix), None).await?;
        let total_files = objects.len();
        let total_bytes: u64 = objects.iter().map(|o| o.size).sum();

        // 进度跟踪
        let completed_files = Arc::new(AtomicUsize::new(0));
        let transferred_bytes = Arc::new(AtomicU64::new(0));

        // 并发下载
        let results: Vec<(String, Result<u64, String>)> =
            futures::stream::iter(objects.into_iter().map(|obj| {
                let local_dir = local_dir.to_path_buf();
                let prefix = prefix.to_string();
                let completed_files = completed_files.clone();
                let transferred_bytes = transferred_bytes.clone();
                let progress_callback = options.progress_callback.clone();
                let file_options = GetFileOptions {
                    overwrite: options.overwrite,
                };

                async move {
                    // 计算本地路径
                    let rel_path = if prefix.is_empty() {
                        obj.key.clone()
                    } else {
                        obj.key
                            .strip_prefix(&prefix)
                            .map(|s| s.trim_start_matches('/'))
                            .unwrap_or(&obj.key)
                            .to_string()
                    };

                    let local_path = local_dir.join(&rel_path);
                    let file_size = obj.size;

                    let result = match self.get_file(&obj.key, &local_path, file_options).await {
                        Ok(()) => {
                            transferred_bytes.fetch_add(file_size, Ordering::SeqCst);
                            Ok(file_size)
                        }
                        Err(e) => Err(e.to_string()),
                    };

                    let completed = completed_files.fetch_add(1, Ordering::SeqCst) + 1;

                    // 更新进度
                    if let Some(ref callback) = progress_callback {
                        callback.on_progress(&DirectoryTransferProgress {
                            current_file: obj.key.clone(),
                            completed_files: completed,
                            total_files,
                            transferred_bytes: transferred_bytes.load(Ordering::SeqCst),
                            total_bytes,
                        });
                        callback.on_file_complete(
                            &obj.key,
                            result.is_ok(),
                            result.as_ref().err().map(|s| s.as_str()),
                        );
                    }

                    (obj.key, result)
                }
            }))
            .buffer_unordered(options.concurrency)
            .collect()
            .await;

        // 统计结果
        let mut success_count = 0;
        let mut failed_count = 0;
        let mut total_transferred: u64 = 0;
        let mut failed_list = Vec::new();

        for (path, result) in results {
            match result {
                Ok(size) => {
                    success_count += 1;
                    total_transferred += size;
                }
                Err(error) => {
                    failed_count += 1;
                    failed_list.push(FailedFile { path, error });
                }
            }
        }

        Ok(DirectoryTransferResult {
            success_count,
            failed_count,
            total_bytes: total_transferred,
            failed_files: failed_list,
        })
    }
}

/// 收集目录中的所有文件
///
/// 扫描指定目录，返回所有符合条件的文件及其大小。
/// 内部调用 `collect_files_recursive` 进行递归收集。
///
/// # 参数
///
/// - `dir`: 要扫描的目录路径
/// - `options`: 过滤选项，包括：
///   - `recursive`: 是否递归扫描子目录
///   - `include_patterns`: 包含的文件模式
///   - `exclude_patterns`: 排除的文件模式
///
/// # 返回值
///
/// - `Ok(Vec<(String, u64)>)`: 文件列表，每项包含（相对路径, 文件大小）
/// - `Err(ObjectStoreError)`: 扫描失败（IO 错误等）
async fn collect_files(
    dir: &Path,
    options: &PutDirectoryOptions,
) -> Result<Vec<(String, u64)>, ObjectStoreError> {
    let mut files = Vec::new();
    collect_files_recursive(dir, dir, options, &mut files).await?;
    Ok(files)
}

/// 递归收集文件
///
/// 递归遍历目录结构，收集所有符合条件的文件。
/// 使用 `Box::pin` 实现异步递归。
///
/// # 参数
///
/// - `base_dir`: 基准目录，用于计算相对路径
/// - `current_dir`: 当前正在扫描的目录
/// - `options`: 过滤和递归选项
/// - `files`: 收集结果的可变引用，存储（相对路径, 文件大小）
///
/// # 返回值
///
/// - `Ok(())`: 收集成功
/// - `Err(ObjectStoreError)`: 收集失败（IO 错误、路径处理错误等）
///
/// # 行为说明
///
/// - 遍历当前目录的所有条目
/// - 对于文件：检查是否应该包含（通过 `should_include_file`），如果是则添加到结果
/// - 对于目录：如果 `options.recursive` 为 `true`，递归处理子目录
/// - 符号链接会被跟随（取决于 `tokio::fs::metadata` 的行为）
async fn collect_files_recursive(
    base_dir: &Path,
    current_dir: &Path,
    options: &PutDirectoryOptions,
    files: &mut Vec<(String, u64)>,
) -> Result<(), ObjectStoreError> {
    let mut entries = tokio::fs::read_dir(current_dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let metadata = entry.metadata().await?;

        if metadata.is_file() {
            let rel_path = path
                .strip_prefix(base_dir)
                .map_err(|e| ObjectStoreError::InvalidInput(e.to_string()))?
                .to_string_lossy()
                .to_string();

            // 应用过滤器
            if should_include_file(&rel_path, options) {
                files.push((rel_path, metadata.len()));
            }
        } else if metadata.is_dir() && options.recursive {
            Box::pin(collect_files_recursive(base_dir, &path, options, files)).await?;
        }
    }

    Ok(())
}

/// 检查文件是否应该被包含
///
/// 根据配置的过滤规则判断文件是否应该被包含在上传列表中。
///
/// # 参数
///
/// - `rel_path`: 文件的相对路径（相对于基准目录）
/// - `options`: 包含过滤规则的选项
///
/// # 返回值
///
/// - `true`: 文件应该被包含
/// - `false`: 文件应该被排除
///
/// # 过滤逻辑
///
/// 1. **排除模式优先**：如果文件匹配任何 `exclude_patterns`，返回 `false`
/// 2. **包含模式检查**：
///    - 如果没有设置 `include_patterns`，返回 `true`（包含所有文件）
///    - 如果设置了 `include_patterns`，文件必须匹配至少一个模式才返回 `true`
///
/// # 模式格式
///
/// 使用 glob 语法：
/// - `*`: 匹配任意字符（不含路径分隔符）
/// - `**`: 匹配任意路径
/// - `?`: 匹配单个字符
/// - `[abc]`: 匹配字符集
fn should_include_file(rel_path: &str, options: &PutDirectoryOptions) -> bool {
    // 检查排除模式
    if let Some(ref patterns) = options.exclude_patterns {
        for pattern in patterns {
            if let Ok(p) = Pattern::new(pattern) {
                if p.matches(rel_path) {
                    return false;
                }
            }
        }
    }

    // 检查包含模式
    if let Some(ref patterns) = options.include_patterns {
        for pattern in patterns {
            if let Ok(p) = Pattern::new(pattern) {
                if p.matches(rel_path) {
                    return true;
                }
            }
        }
        return false;
    }

    true
}
