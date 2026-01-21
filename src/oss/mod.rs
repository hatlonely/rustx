mod error;
mod object_store;
mod object_meta;
mod aws_s3_object_store;
mod ali_oss_object_store;
mod gcp_gcs_object_store;

pub use error::ObjectStoreError;
pub use object_store::ObjectStore;
pub use object_meta::{
    ObjectMeta, PutOptions, GetOptions,
    // 新增类型
    PartInfo, TransferProgress, DirectoryTransferProgress,
    ProgressCallback, DirectoryProgressCallback,
    PutStreamOptions, GetStreamOptions,
    PutFileOptions, GetFileOptions,
    PutDirectoryOptions, GetDirectoryOptions,
    FailedFile, DirectoryTransferResult,
};
pub use aws_s3_object_store::{AwsS3ObjectStore, AwsS3ObjectStoreConfig};
pub use ali_oss_object_store::{AliOssObjectStore, AliOssObjectStoreConfig};
pub use gcp_gcs_object_store::{GcpGcsObjectStore, GcpGcsObjectStoreConfig};

use crate::cfg::register_trait;

/// 注册所有 ObjectStore 实现
///
/// 该函数会注册所有内置的 ObjectStore 实现到类型系统中，
/// 使得它们可以通过配置动态创建。
///
/// # 示例
///
/// ```rust
/// use rustx::oss::register_object_store;
///
/// // 初始化注册
/// register_object_store();
/// ```
pub fn register_object_store() {
    // 注册 S3
    register_trait::<AwsS3ObjectStore, dyn ObjectStore, AwsS3ObjectStoreConfig>(
        "AwsS3ObjectStore"
    ).expect("Failed to register AwsS3ObjectStore");

    // 注册阿里云 OSS
    register_trait::<AliOssObjectStore, dyn ObjectStore, AliOssObjectStoreConfig>(
        "AliOssObjectStore"
    ).expect("Failed to register AliOssObjectStore");

    // 注册 GCP GCS
    register_trait::<GcpGcsObjectStore, dyn ObjectStore, GcpGcsObjectStoreConfig>(
        "GcpGcsObjectStore"
    ).expect("Failed to register GcpGcsObjectStore");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry() {
        register_object_store();
        // 如果没有 panic，说明注册成功
    }
}
