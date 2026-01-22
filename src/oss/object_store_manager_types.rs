// Options and result types for StoreManager operations

use super::object_store_types::{DirectoryProgressCallback, DirectoryTransferResult, FailedFile, ProgressCallback};
use std::sync::Arc;

// ============ Options ============

/// Copy operation options
#[derive(Clone, Default)]
pub struct CpOptions {
    /// Recursive copy for directories
    pub recursive: bool,

    /// Overwrite existing files
    pub overwrite: bool,

    /// Concurrent operations count (None = use manager defaults)
    pub concurrency: Option<usize>,

    /// Part size for multipart upload in bytes (None = use manager defaults)
    pub part_size: Option<usize>,

    /// Threshold for multipart upload in bytes (None = use manager defaults)
    pub multipart_threshold: Option<u64>,

    /// Include file pattern (glob)
    pub include: Option<String>,

    /// Exclude file pattern (glob)
    pub exclude: Option<String>,

    /// Progress callback for single file operations
    pub progress_callback: Option<Arc<dyn ProgressCallback>>,

    /// Progress callback for directory operations
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
                "progress_callback",
                &self.progress_callback.as_ref().map(|_| "..."),
            )
            .field(
                "directory_progress_callback",
                &self.directory_progress_callback.as_ref().map(|_| "..."),
            )
            .finish()
    }
}

/// List operation options
#[derive(Debug, Clone, Default)]
pub struct LsOptions {
    /// Maximum number of objects to return
    pub max_keys: Option<usize>,
}

/// Remove operation options
#[derive(Debug, Clone, Default)]
pub struct RmOptions {
    /// Recursive delete for directories/prefixes
    pub recursive: bool,

    /// Include file pattern (glob)
    pub include: Option<String>,

    /// Exclude file pattern (glob)
    pub exclude: Option<String>,
}

// ============ Results ============

/// Copy operation result
#[derive(Debug, Default)]
pub struct CpResult {
    /// Number of successfully copied files
    pub success_count: usize,

    /// Number of failed files
    pub failed_count: usize,

    /// Total bytes transferred
    pub total_bytes: u64,

    /// List of failed files with error messages
    pub failed_files: Vec<FailedFile>,
}

impl CpResult {
    /// Create a result for a single successful file
    pub fn single_success(bytes: u64) -> Self {
        Self {
            success_count: 1,
            failed_count: 0,
            total_bytes: bytes,
            failed_files: Vec::new(),
        }
    }

    /// Create a result for a single failed file
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

/// Remove operation result
#[derive(Debug, Default)]
pub struct RmResult {
    /// Number of successfully deleted objects
    pub deleted_count: usize,

    /// Number of failed deletions
    pub failed_count: usize,

    /// List of failed files with error messages
    pub failed_files: Vec<FailedFile>,
}
