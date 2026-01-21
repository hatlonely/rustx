use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::StreamExt;
use glob::Pattern;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};

use crate::oss::{
    DirectoryTransferProgress, DirectoryTransferResult, FailedFile, GetDirectoryOptions,
    GetFileOptions, GetOptions, GetStreamOptions, ObjectMeta, ObjectStoreError, PartInfo,
    PutDirectoryOptions, PutFileOptions, PutOptions, PutStreamOptions, TransferProgress,
};

/// 对象存储统一接口
#[async_trait]
pub trait ObjectStore: Send + Sync {
    // === 基础 CRUD ===

    /// 上传对象
    async fn put_object(&self, key: &str, value: Bytes) -> Result<(), ObjectStoreError>;

    /// 上传对象（带选项）
    async fn put_object_ex(
        &self,
        key: &str,
        value: Bytes,
        options: PutOptions,
    ) -> Result<(), ObjectStoreError>;

    /// 获取对象
    async fn get_object(&self, key: &str) -> Result<Bytes, ObjectStoreError>;

    /// 获取对象（带选项）
    async fn get_object_ex(&self, key: &str, options: GetOptions)
        -> Result<Bytes, ObjectStoreError>;

    /// 删除对象
    async fn delete_object(&self, key: &str) -> Result<(), ObjectStoreError>;

    /// 获取对象元数据
    ///
    /// 返回 `Ok(Some(ObjectMeta))` 如果对象存在
    /// 返回 `Ok(None)` 如果对象不存在
    async fn head_object(&self, key: &str) -> Result<Option<ObjectMeta>, ObjectStoreError>;

    /// 循环调用 list_objects 获取所有全部或最大个数的 objects
    async fn list_objects(
        &self,
        prefix: Option<&str>,
        max_keys: Option<usize>,
    ) -> Result<Vec<ObjectMeta>, ObjectStoreError>;

    // === 流式接口 ===

    /// 流式上传
    async fn put_stream(
        &self,
        key: &str,
        reader: Box<dyn AsyncRead + Send + Unpin>,
        size: u64,
        options: PutStreamOptions,
    ) -> Result<(), ObjectStoreError>;

    /// 流式下载
    async fn get_stream(
        &self,
        key: &str,
        writer: Box<dyn AsyncWrite + Send + Unpin>,
        options: GetStreamOptions,
    ) -> Result<u64, ObjectStoreError>;

    // === 分片上传接口 ===

    /// 初始化分片上传
    async fn create_multipart_upload(
        &self,
        key: &str,
        options: PutOptions,
    ) -> Result<String, ObjectStoreError>;

    /// 上传分片
    async fn upload_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        data: Bytes,
    ) -> Result<PartInfo, ObjectStoreError>;

    /// 完成分片上传
    async fn complete_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
        parts: Vec<PartInfo>,
    ) -> Result<(), ObjectStoreError>;

    /// 取消分片上传
    async fn abort_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
    ) -> Result<(), ObjectStoreError>;

    // === 文件操作（默认实现） ===

    /// 上传本地文件
    async fn put_file(
        &self,
        key: &str,
        local_path: &Path,
        options: PutFileOptions,
    ) -> Result<(), ObjectStoreError> {
        let metadata = tokio::fs::metadata(local_path).await?;
        let file_size = metadata.len();

        if file_size <= options.multipart_threshold {
            // 小文件：使用流式上传
            let file = tokio::fs::File::open(local_path).await?;
            let stream_options = PutStreamOptions {
                content_type: options.content_type.clone(),
                metadata: options.metadata.clone(),
                progress_callback: options.progress_callback.clone(),
            };
            self.put_stream(key, Box::new(file), file_size, stream_options)
                .await
        } else {
            // 大文件：使用分片上传
            self.put_file_multipart(key, local_path, file_size, options)
                .await
        }
    }

    /// 分片上传大文件（内部方法）
    async fn put_file_multipart(
        &self,
        key: &str,
        local_path: &Path,
        file_size: u64,
        options: PutFileOptions,
    ) -> Result<(), ObjectStoreError> {
        let put_options = PutOptions {
            content_type: options.content_type.clone(),
            metadata: options.metadata.clone(),
            tags: None,
        };

        let upload_id = self.create_multipart_upload(key, put_options).await?;

        // 计算分片数量
        let part_size = options.part_size as u64;
        let part_count = (file_size + part_size - 1) / part_size;

        // 准备分片任务
        let mut part_tasks = Vec::new();
        for i in 0..part_count {
            let start = i * part_size;
            let end = std::cmp::min(start + part_size, file_size);
            part_tasks.push((i as u32 + 1, start, end));
        }

        // 进度跟踪
        let transferred = Arc::new(AtomicU64::new(0));

        // 并发上传分片
        let results: Vec<Result<PartInfo, ObjectStoreError>> =
            futures::stream::iter(part_tasks.into_iter().map(|(part_number, start, end)| {
                let local_path = local_path.to_path_buf();
                let key = key.to_string();
                let upload_id = upload_id.clone();
                let transferred = transferred.clone();
                let progress_callback = options.progress_callback.clone();

                async move {
                    // 读取分片数据
                    let mut file = tokio::fs::File::open(&local_path).await?;
                    tokio::io::AsyncSeekExt::seek(
                        &mut file,
                        std::io::SeekFrom::Start(start),
                    )
                    .await?;

                    let chunk_size = (end - start) as usize;
                    let mut buffer = vec![0u8; chunk_size];
                    file.read_exact(&mut buffer).await?;

                    let result = self
                        .upload_part(&key, &upload_id, part_number, Bytes::from(buffer))
                        .await?;

                    // 更新进度
                    let new_transferred =
                        transferred.fetch_add(chunk_size as u64, Ordering::SeqCst)
                            + chunk_size as u64;
                    if let Some(ref callback) = progress_callback {
                        callback.on_progress(&TransferProgress {
                            transferred_bytes: new_transferred,
                            total_bytes: file_size,
                        });
                    }

                    Ok(result)
                }
            }))
            .buffer_unordered(options.multipart_concurrency)
            .collect()
            .await;

        // 检查是否有错误
        let mut parts = Vec::new();
        for result in results {
            match result {
                Ok(part) => parts.push(part),
                Err(e) => {
                    // 出错时取消上传
                    let _ = self.abort_multipart_upload(key, &upload_id).await;
                    return Err(e);
                }
            }
        }

        // 按 part_number 排序
        parts.sort_by_key(|p| p.part_number);

        // 完成上传
        self.complete_multipart_upload(key, &upload_id, parts)
            .await
    }

    /// 下载对象到本地文件
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
            progress_callback: options.progress_callback,
        };

        self.get_stream(key, Box::new(file), stream_options).await?;
        Ok(())
    }

    // === 目录操作（默认实现） ===

    /// 上传本地目录
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
                    progress_callback: None,
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
                    progress_callback: None,
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
async fn collect_files(
    dir: &Path,
    options: &PutDirectoryOptions,
) -> Result<Vec<(String, u64)>, ObjectStoreError> {
    let mut files = Vec::new();
    collect_files_recursive(dir, dir, options, &mut files).await?;
    Ok(files)
}

/// 递归收集文件
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
