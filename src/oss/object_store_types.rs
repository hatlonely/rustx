use chrono::{DateTime, Utc};
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::sync::Arc;

/// 对象元数据
///
/// 存储对象的基本信息，通常由 `head_object` 或 `list_objects` 返回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
    /// 对象的唯一标识符（键）
    ///
    /// 通常是类似路径的字符串，如 `"folder/subfolder/file.txt"`
    pub key: String,

    /// 对象大小（字节数）
    pub size: u64,

    /// 最后修改时间（UTC 时区）
    pub last_modified: DateTime<Utc>,

    /// 实体标签（ETag）
    ///
    /// 用于标识对象内容的唯一性，通常是 MD5 哈希值。
    /// 对于分片上传的对象，ETag 格式可能为 `"hash-partcount"`。
    /// 某些存储后端可能不提供此字段。
    pub etag: Option<String>,

    /// 内容类型（MIME 类型）
    ///
    /// 如 `"text/plain"`、`"application/json"`、`"image/png"` 等。
    /// 如果上传时未指定，存储后端可能会自动推断或使用默认值。
    pub content_type: Option<String>,
}

/// 上传对象选项
///
/// 用于 `put_object` 方法，配置对象上传的各种参数。
#[derive(Debug, Clone, SmartDefault)]
pub struct PutObjectOptions {
    /// 内容类型（MIME 类型）
    ///
    /// 指定对象的 MIME 类型，如 `"text/plain"`、`"application/json"` 等。
    /// 如果为 `None`，存储后端可能会使用默认值（通常是 `"application/octet-stream"`）。
    pub content_type: Option<String>,

    /// 自定义元数据
    ///
    /// 附加到对象的键值对元数据。这些元数据会与对象一起存储，
    /// 可以通过 `head_object` 获取。
    ///
    /// 注意：不同存储后端对元数据的键名和值有不同的限制（长度、字符集等）。
    pub metadata: Option<HashMap<String, String>>,
}

/// 获取对象选项
///
/// 用于 `get_object` 方法，配置对象下载的各种参数。
#[derive(Debug, Clone, SmartDefault)]
pub struct GetObjectOptions {
    /// 范围请求（字节范围）
    ///
    /// 指定要下载的字节范围，用于部分下载或断点续传。
    /// - `Some(0..1000)`: 下载第 0 到 999 字节（共 1000 字节）
    /// - `None`: 下载整个对象
    ///
    /// 注意：范围是左闭右开区间 `[start, end)`
    pub range: Option<std::ops::Range<u64>>,
}

/// 分片信息
///
/// 分片上传时，每个已上传分片的信息。用于完成分片上传时的合并操作。
#[derive(Debug, Clone)]
pub struct PartInfo {
    /// 分片编号
    ///
    /// 从 1 开始的分片序号，用于标识分片在完整对象中的位置。
    pub part_number: u32,

    /// 分片的 ETag
    ///
    /// 存储后端返回的分片标识符，通常是分片内容的 MD5 哈希。
    /// 完成分片上传时需要提供此值。
    pub etag: String,

    /// 分片大小（字节数）
    pub size: u64,
}

/// 目录传输进度信息
///
/// 目录上传/下载过程中的实时进度，通过 `DirectoryProgressCallback` 回调传递。
#[derive(Debug, Clone)]
pub struct DirectoryTransferProgress {
    /// 当前正在处理的文件路径
    pub current_file: String,

    /// 已完成的文件数量
    pub completed_files: usize,

    /// 总文件数量
    pub total_files: usize,

    /// 已传输的字节数（仅统计成功的文件）
    pub transferred_bytes: u64,

    /// 总字节数
    pub total_bytes: u64,
}

/// 目录进度回调 trait
///
/// 实现此 trait 以接收目录传输过程中的进度更新。
/// 需要是线程安全的（`Send + Sync`），因为回调可能从多个并发任务中调用。
pub trait DirectoryProgressCallback: Send + Sync {
    /// 进度更新回调
    ///
    /// 在每个文件处理完成后调用，提供当前的整体进度信息。
    ///
    /// # 参数
    ///
    /// - `progress`: 当前的进度信息
    fn on_progress(&self, progress: &DirectoryTransferProgress);

    /// 单个文件完成回调
    ///
    /// 在每个文件传输完成（成功或失败）后调用。
    ///
    /// # 参数
    ///
    /// - `key`: 文件的键/路径
    /// - `success`: 是否成功
    /// - `error_message`: 如果失败，包含错误信息
    fn on_file_complete(&self, key: &str, success: bool, error_message: Option<&str>);
}

/// 流式上传选项
///
/// 用于 `put_stream` 方法，配置流式上传的各种参数。
/// 支持自动选择直接上传或分片上传策略。
#[derive(Clone, SmartDefault)]
pub struct PutStreamOptions {
    /// 内容类型（MIME 类型）
    ///
    /// 指定对象的 MIME 类型，如 `"text/plain"`、`"application/json"` 等。
    /// 如果为 `None`，存储后端可能会使用默认值。
    pub content_type: Option<String>,

    /// 自定义元数据
    ///
    /// 附加到对象的键值对元数据。
    pub metadata: Option<HashMap<String, String>>,

    /// 分片上传阈值（字节数）
    ///
    /// 当数据大小超过此阈值时，将使用分片上传而非直接上传。
    /// - 默认值：100MB (104857600 字节)
    /// - 分片上传可以提高大文件上传的可靠性和效率
    /// - 如果 `put_stream` 的 `size` 参数为 `None`，将强制使用分片上传
    #[default = 104857600]
    pub multipart_threshold: u64,

    /// 分片大小（字节数）
    ///
    /// 分片上传时每个分片的大小。
    /// - 默认值：8MB (8388608 字节)
    /// - 较大的分片可以减少请求次数，但增加单次失败的重传成本
    /// - 不同存储后端对分片大小有不同限制（通常最小 5MB，最大 5GB）
    #[default = 8388608]
    pub part_size: usize,

    /// 分片上传并发数
    ///
    /// 同时上传的分片数量。
    /// - 默认值：4
    /// - 增加并发数可以提高上传速度，但会消耗更多内存和网络带宽
    /// - 建议根据网络条件和服务器限制调整
    #[default = 4]
    pub multipart_concurrency: usize,
}

impl std::fmt::Debug for PutStreamOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PutStreamOptions")
            .field("content_type", &self.content_type)
            .field("metadata", &self.metadata)
            .field("multipart_threshold", &self.multipart_threshold)
            .field("part_size", &self.part_size)
            .field("multipart_concurrency", &self.multipart_concurrency)
            .finish()
    }
}

/// 流式下载选项
///
/// 用于 `get_stream` 方法，配置流式下载的各种参数。
#[derive(Clone, SmartDefault)]
pub struct GetStreamOptions {
    /// 范围请求（字节范围）
    ///
    /// 指定要下载的字节范围，用于部分下载或断点续传。
    /// - `Some(1000..2000)`: 下载第 1000 到 1999 字节
    /// - `None`: 下载整个对象
    ///
    /// 注意：范围是左闭右开区间 `[start, end)`
    pub range: Option<std::ops::Range<u64>>,
}

impl std::fmt::Debug for GetStreamOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetStreamOptions")
            .field("range", &self.range)
            .finish()
    }
}

/// 文件上传选项
///
/// 用于 `put_file` 方法，配置本地文件上传的各种参数。
/// 底层使用流式上传实现，参数与 `PutStreamOptions` 类似。
#[derive(Clone, SmartDefault)]
pub struct PutFileOptions {
    /// 内容类型（MIME 类型）
    ///
    /// 指定对象的 MIME 类型。如果为 `None`，可以根据文件扩展名推断。
    pub content_type: Option<String>,

    /// 自定义元数据
    ///
    /// 附加到对象的键值对元数据。
    pub metadata: Option<HashMap<String, String>>,

    /// 分片上传阈值（字节数）
    ///
    /// 当文件大小超过此阈值时，将使用分片上传。
    /// - 默认值：100MB (104857600 字节)
    #[default = 104857600]
    pub multipart_threshold: u64,

    /// 分片大小（字节数）
    ///
    /// 分片上传时每个分片的大小。
    /// - 默认值：8MB (8388608 字节)
    #[default = 8388608]
    pub part_size: usize,

    /// 分片上传并发数
    ///
    /// 同时上传的分片数量。
    /// - 默认值：4
    #[default = 4]
    pub multipart_concurrency: usize,
}

impl std::fmt::Debug for PutFileOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PutFileOptions")
            .field("content_type", &self.content_type)
            .field("metadata", &self.metadata)
            .field("multipart_threshold", &self.multipart_threshold)
            .field("part_size", &self.part_size)
            .field("multipart_concurrency", &self.multipart_concurrency)
            .finish()
    }
}

/// 文件下载选项
///
/// 用于 `get_file` 方法，配置对象下载到本地文件的各种参数。
#[derive(Clone, SmartDefault)]
pub struct GetFileOptions {
    /// 是否覆盖已存在的文件
    ///
    /// - `true`: 如果本地文件已存在，将被覆盖
    /// - `false`（默认）: 如果本地文件已存在，返回 `ObjectStoreError::FileExists` 错误
    #[default = false]
    pub overwrite: bool,
}

impl std::fmt::Debug for GetFileOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetFileOptions")
            .field("overwrite", &self.overwrite)
            .finish()
    }
}

/// 目录上传选项
///
/// 用于 `put_directory` 方法，配置本地目录批量上传的各种参数。
/// 支持并发上传、文件过滤、进度回调等功能。
#[derive(Clone, SmartDefault)]
pub struct PutDirectoryOptions {
    /// 并发上传的文件数量
    ///
    /// 同时上传的文件数量（不是分片数）。
    /// - 默认值：4
    /// - 增加并发数可以提高整体上传速度
    /// - 注意：每个文件还可能有自己的分片并发（由 `multipart_concurrency` 控制）
    #[default = 4]
    pub concurrency: usize,

    /// 包含模式（glob 格式）
    ///
    /// 只上传匹配这些模式的文件。
    /// - `Some(vec!["*.txt", "*.json"])`: 只上传 .txt 和 .json 文件
    /// - `None`: 不限制（包含所有文件）
    ///
    /// 支持的 glob 语法：
    /// - `*`: 匹配任意字符（不含路径分隔符）
    /// - `**`: 匹配任意路径
    /// - `?`: 匹配单个字符
    /// - `[abc]`: 匹配字符集
    pub include_patterns: Option<Vec<String>>,

    /// 排除模式（glob 格式）
    ///
    /// 排除匹配这些模式的文件（优先级高于 `include_patterns`）。
    /// - `Some(vec!["*.log", ".git/**"])`: 排除日志文件和 .git 目录
    /// - `None`: 不排除任何文件
    pub exclude_patterns: Option<Vec<String>>,

    /// 是否递归处理子目录
    ///
    /// - `true`（默认）: 递归上传所有子目录中的文件
    /// - `false`: 只上传指定目录下的直接子文件
    #[default = true]
    pub recursive: bool,

    /// 分片上传阈值（字节数）
    ///
    /// 单个文件大小超过此阈值时使用分片上传。
    /// - 默认值：100MB (104857600 字节)
    #[default = 104857600]
    pub multipart_threshold: u64,

    /// 分片大小（字节数）
    ///
    /// 分片上传时每个分片的大小。
    /// - 默认值：8MB (8388608 字节)
    #[default = 8388608]
    pub part_size: usize,

    /// 分片上传并发数
    ///
    /// 单个大文件上传时同时上传的分片数量。
    /// - 默认值：4
    #[default = 4]
    pub multipart_concurrency: usize,

    /// 进度回调
    ///
    /// 用于接收上传进度更新的回调对象。
    /// 每个文件完成时会触发回调，提供整体进度信息。
    /// 如果为 `None`，则不进行进度通知。
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
///
/// 用于 `get_directory` 方法，配置批量下载到本地目录的各种参数。
/// 支持并发下载、覆盖控制、进度回调等功能。
#[derive(Clone, SmartDefault)]
pub struct GetDirectoryOptions {
    /// 并发下载的文件数量
    ///
    /// 同时下载的文件数量。
    /// - 默认值：4
    /// - 增加并发数可以提高整体下载速度
    #[default = 4]
    pub concurrency: usize,

    /// 是否覆盖已存在的本地文件
    ///
    /// - `true`: 如果本地文件已存在，将被覆盖
    /// - `false`（默认）: 如果本地文件已存在，该文件下载失败并记录在 `failed_files` 中
    #[default = false]
    pub overwrite: bool,

    /// 进度回调
    ///
    /// 用于接收下载进度更新的回调对象。
    /// 每个文件完成时会触发回调，提供整体进度信息。
    /// 如果为 `None`，则不进行进度通知。
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
///
/// 记录目录传输过程中失败的单个文件的详细信息。
#[derive(Debug, Clone)]
pub struct FailedFile {
    /// 文件路径
    ///
    /// 对于上传：本地文件的相对路径
    /// 对于下载：对象的键
    pub path: String,

    /// 错误信息
    ///
    /// 描述失败原因的字符串，如网络错误、权限不足、文件已存在等。
    pub error: String,
}

/// 目录传输结果
///
/// `put_directory` 和 `get_directory` 方法的返回值，
/// 包含批量传输的统计信息和失败详情。
#[derive(Debug, Clone, Default)]
pub struct DirectoryTransferResult {
    /// 成功传输的文件数
    pub success_count: usize,

    /// 失败的文件数
    ///
    /// 等于 `failed_files.len()`
    pub failed_count: usize,

    /// 成功传输的总字节数
    ///
    /// 只统计成功传输的文件大小，不包括失败的文件。
    pub total_bytes: u64,

    /// 失败的文件列表
    ///
    /// 包含每个失败文件的路径和错误信息。
    /// 如果所有文件都成功，此列表为空。
    pub failed_files: Vec<FailedFile>,
}
