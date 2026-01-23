//! StoreManager 操作的选项和结果类型
//!
//! 定义了 `ObjectStoreManager` 高级操作所需的配置和返回类型。

use super::object_store_types::{DirectoryProgressCallback, DirectoryTransferResult, FailedFile};
use std::sync::Arc;

// ============ 选项类型 ============

/// 复制操作选项
///
/// 用于配置 `cp` 操作（文件/目录复制）的行为。
/// 支持本地到远程、远程到本地、远程到远程的复制。
#[derive(Clone, Default)]
pub struct CpOptions {
    /// 是否递归复制目录
    ///
    /// - `true`: 递归复制整个目录树
    /// - `false`: 仅复制单个文件
    pub recursive: bool,

    /// 是否覆盖已存在的文件
    ///
    /// - `true`: 如果目标文件已存在，则覆盖
    /// - `false`: 如果目标文件已存在，则返回错误
    pub overwrite: bool,

    /// 并发操作数量
    ///
    /// - `Some(n)`: 使用指定的并发数
    /// - `None`: 使用管理器的默认并发数
    pub concurrency: Option<usize>,

    /// 分片上传的分片大小（字节）
    ///
    /// - `Some(n)`: 使用指定的分片大小
    /// - `None`: 使用管理器的默认分片大小（8MB）
    pub part_size: Option<usize>,

    /// 启用分片上传的阈值（字节）
    ///
    /// - `Some(n)`: 文件大小超过此值时使用分片上传
    /// - `None`: 使用管理器的默认阈值（100MB）
    pub multipart_threshold: Option<u64>,

    /// 包含文件模式（glob 格式）
    ///
    /// 仅复制匹配此模式的文件。例如：
    /// - `"*.txt"`: 仅复制 .txt 文件
    /// - `"**/*.log"`: 递归匹配所有 .log 文件
    pub include: Option<String>,

    /// 排除文件模式（glob 格式）
    ///
    /// 排除匹配此模式的文件。优先级高于 `include`。
    pub exclude: Option<String>,

    /// 目录操作的进度回调
    ///
    /// 用于跟踪批量操作的进度，包括已完成文件数、传输字节数等。
    pub directory_progress_callback: Option<Arc<dyn DirectoryProgressCallback>>,
}

impl std::fmt::Debug for CpOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpOptions")
            .field("recursive", &self.recursive)
            .field("overwrite", &self.overwrite)
            .field("concurrency", &self.concurrency)
            .field("part_size", &self.part_size)
            .field("multipart_threshold", &self.multipart_threshold)
            .field("include", &self.include)
            .field("exclude", &self.exclude)
            .field(
                "directory_progress_callback",
                &self.directory_progress_callback.as_ref().map(|_| "..."),
            )
            .finish()
    }
}

/// 列举操作选项
///
/// 用于配置 `ls` 操作（列举对象）的行为。
#[derive(Debug, Clone, Default)]
pub struct LsOptions {
    /// 返回的最大对象数量
    ///
    /// - `Some(n)`: 最多返回 n 个对象
    /// - `None`: 返回所有匹配的对象（无限制）
    pub max_keys: Option<usize>,
}

/// 删除操作选项
///
/// 用于配置 `rm` 操作（删除对象）的行为。
#[derive(Debug, Clone, Default)]
pub struct RmOptions {
    /// 是否递归删除目录/前缀下的所有对象
    ///
    /// - `true`: 递归删除所有匹配前缀的对象
    /// - `false`: 仅删除单个对象
    pub recursive: bool,

    /// 包含文件模式（glob 格式）
    ///
    /// 仅删除匹配此模式的文件。递归模式下有效。
    pub include: Option<String>,

    /// 排除文件模式（glob 格式）
    ///
    /// 排除匹配此模式的文件。递归模式下有效。优先级高于 `include`。
    pub exclude: Option<String>,
}

// ============ 结果类型 ============

/// 复制操作结果
///
/// 表示 `cp` 操作的执行结果，包含成功和失败的统计信息。
#[derive(Debug, Default)]
pub struct CpResult {
    /// 成功复制的文件数量
    pub success_count: usize,

    /// 复制失败的文件数量
    pub failed_count: usize,

    /// 成功传输的总字节数
    pub total_bytes: u64,

    /// 失败文件的详细信息列表
    ///
    /// 每个元素包含文件路径和错误信息。
    pub failed_files: Vec<FailedFile>,
}

impl CpResult {
    /// 创建单个文件成功的结果
    ///
    /// # 参数
    ///
    /// - `bytes`: 成功传输的字节数
    ///
    /// # 返回值
    ///
    /// 返回一个 `CpResult`，`success_count` 为 1，`total_bytes` 为指定值。
    pub fn single_success(bytes: u64) -> Self {
        Self {
            success_count: 1,
            failed_count: 0,
            total_bytes: bytes,
            failed_files: Vec::new(),
        }
    }

    /// 创建单个文件失败的结果
    ///
    /// # 参数
    ///
    /// - `path`: 失败文件的路径
    /// - `error`: 错误信息
    ///
    /// # 返回值
    ///
    /// 返回一个 `CpResult`，`failed_count` 为 1，包含失败文件详情。
    pub fn single_failure(path: String, error: String) -> Self {
        Self {
            success_count: 0,
            failed_count: 1,
            total_bytes: 0,
            failed_files: vec![FailedFile { path, error }],
        }
    }
}

impl From<DirectoryTransferResult> for CpResult {
    fn from(result: DirectoryTransferResult) -> Self {
        Self {
            success_count: result.success_count,
            failed_count: result.failed_count,
            total_bytes: result.total_bytes,
            failed_files: result.failed_files,
        }
    }
}

/// 删除操作结果
///
/// 表示 `rm` 操作的执行结果，包含成功和失败的统计信息。
#[derive(Debug, Default)]
pub struct RmResult {
    /// 成功删除的对象数量
    pub deleted_count: usize,

    /// 删除失败的对象数量
    pub failed_count: usize,

    /// 删除失败的对象详细信息列表
    ///
    /// 每个元素包含对象键和错误信息。
    pub failed_files: Vec<FailedFile>,
}
