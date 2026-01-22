use chrono::{DateTime, Utc};
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::sync::Arc;

/// 对象元数据
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
    pub key: String,
    pub size: u64,
    pub last_modified: DateTime<Utc>,
    pub etag: Option<String>,
    pub content_type: Option<String>,
}

/// 上传选项
#[derive(Debug, Clone, SmartDefault)]
pub struct PutOptions {
    pub content_type: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
    pub tags: Option<HashMap<String, String>>,
}

/// 获取选项
#[derive(Debug, Clone, SmartDefault)]
pub struct GetOptions {
    pub range: Option<std::ops::Range<u64>>,
}

/// 分片信息
#[derive(Debug, Clone)]
pub struct PartInfo {
    pub part_number: u32,
    pub etag: String,
    pub size: u64,
}

/// 传输进度信息
#[derive(Debug, Clone)]
pub struct TransferProgress {
    pub transferred_bytes: u64,
    pub total_bytes: u64,
}

/// 目录传输进度信息
#[derive(Debug, Clone)]
pub struct DirectoryTransferProgress {
    pub current_file: String,
    pub completed_files: usize,
    pub total_files: usize,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
}

/// 进度回调 trait
pub trait ProgressCallback: Send + Sync {
    fn on_progress(&self, progress: &TransferProgress);
}

/// 目录进度回调 trait
pub trait DirectoryProgressCallback: Send + Sync {
    fn on_progress(&self, progress: &DirectoryTransferProgress);
    fn on_file_complete(&self, key: &str, success: bool, error_message: Option<&str>);
}

/// 流式上传选项
#[derive(Clone, SmartDefault)]
pub struct PutStreamOptions {
    pub content_type: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
    /// 进度回调
    pub progress_callback: Option<Arc<dyn ProgressCallback>>,
}

impl std::fmt::Debug for PutStreamOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PutStreamOptions")
            .field("content_type", &self.content_type)
            .field("metadata", &self.metadata)
            .field("progress_callback", &self.progress_callback.as_ref().map(|_| "..."))
            .finish()
    }
}

/// 流式下载选项
#[derive(Clone, SmartDefault)]
pub struct GetStreamOptions {
    pub range: Option<std::ops::Range<u64>>,
    /// 进度回调
    pub progress_callback: Option<Arc<dyn ProgressCallback>>,
}

impl std::fmt::Debug for GetStreamOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetStreamOptions")
            .field("range", &self.range)
            .field("progress_callback", &self.progress_callback.as_ref().map(|_| "..."))
            .finish()
    }
}

/// 文件上传选项
#[derive(Clone, SmartDefault)]
pub struct PutFileOptions {
    pub content_type: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
    /// 使用分片上传的阈值（默认 100MB）
    #[default = 104857600]
    pub multipart_threshold: u64,
    /// 分片大小（默认 8MB）
    #[default = 8388608]
    pub part_size: usize,
    /// 分片上传并发数
    #[default = 4]
    pub multipart_concurrency: usize,
    /// 进度回调
    pub progress_callback: Option<Arc<dyn ProgressCallback>>,
}

impl std::fmt::Debug for PutFileOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PutFileOptions")
            .field("content_type", &self.content_type)
            .field("metadata", &self.metadata)
            .field("multipart_threshold", &self.multipart_threshold)
            .field("part_size", &self.part_size)
            .field("multipart_concurrency", &self.multipart_concurrency)
            .field("progress_callback", &self.progress_callback.as_ref().map(|_| "..."))
            .finish()
    }
}

/// 文件下载选项
#[derive(Clone, SmartDefault)]
pub struct GetFileOptions {
    /// 是否覆盖已存在的文件
    #[default = false]
    pub overwrite: bool,
    /// 进度回调
    pub progress_callback: Option<Arc<dyn ProgressCallback>>,
}

impl std::fmt::Debug for GetFileOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetFileOptions")
            .field("overwrite", &self.overwrite)
            .field("progress_callback", &self.progress_callback.as_ref().map(|_| "..."))
            .finish()
    }
}

/// 目录上传选项
#[derive(Clone, SmartDefault)]
pub struct PutDirectoryOptions {
    /// 并发上传数
    #[default = 4]
    pub concurrency: usize,
    /// 文件过滤器（glob 模式，如 "*.txt"）
    pub include_patterns: Option<Vec<String>>,
    pub exclude_patterns: Option<Vec<String>>,
    /// 是否递归处理子目录
    #[default = true]
    pub recursive: bool,
    /// 使用分片上传的阈值（默认 100MB）
    #[default = 104857600]
    pub multipart_threshold: u64,
    /// 分片大小（默认 8MB）
    #[default = 8388608]
    pub part_size: usize,
    /// 分片上传并发数
    #[default = 4]
    pub multipart_concurrency: usize,
    /// 进度回调
    pub progress_callback: Option<Arc<dyn DirectoryProgressCallback>>,
}

impl std::fmt::Debug for PutDirectoryOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PutDirectoryOptions")
            .field("concurrency", &self.concurrency)
            .field("include_patterns", &self.include_patterns)
            .field("exclude_patterns", &self.exclude_patterns)
            .field("recursive", &self.recursive)
            .field("multipart_threshold", &self.multipart_threshold)
            .field("part_size", &self.part_size)
            .field("multipart_concurrency", &self.multipart_concurrency)
            .field("progress_callback", &self.progress_callback.as_ref().map(|_| "..."))
            .finish()
    }
}

/// 目录下载选项
#[derive(Clone, SmartDefault)]
pub struct GetDirectoryOptions {
    /// 并发下载数
    #[default = 4]
    pub concurrency: usize,
    /// 是否覆盖已存在的文件
    #[default = false]
    pub overwrite: bool,
    /// 进度回调
    pub progress_callback: Option<Arc<dyn DirectoryProgressCallback>>,
}

impl std::fmt::Debug for GetDirectoryOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetDirectoryOptions")
            .field("concurrency", &self.concurrency)
            .field("overwrite", &self.overwrite)
            .field("progress_callback", &self.progress_callback.as_ref().map(|_| "..."))
            .finish()
    }
}

/// 失败的文件信息
#[derive(Debug, Clone)]
pub struct FailedFile {
    pub path: String,
    pub error: String,
}

/// 目录传输结果
#[derive(Debug, Clone, Default)]
pub struct DirectoryTransferResult {
    /// 成功传输的文件数
    pub success_count: usize,
    /// 失败的文件数
    pub failed_count: usize,
    /// 总传输字节数
    pub total_bytes: u64,
    /// 失败的文件列表
    pub failed_files: Vec<FailedFile>,
}
