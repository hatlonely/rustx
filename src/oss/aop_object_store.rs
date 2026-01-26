//! AOP ObjectStore 装饰器
//!
//! 为 ObjectStore 提供 AOP（面向切面编程）功能，包括日志记录和重试。

use async_trait::async_trait;
use bytes::Bytes;
use garde::Validate;
use serde::Deserialize;
use smart_default::SmartDefault;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::aop::{Aop, AopConfig};
use crate::cfg::{create_trait_from_type_options, TypeOptions};
use crate::oss::{
    DirectoryTransferResult, GetDirectoryOptions, GetFileOptions, GetObjectOptions,
    GetStreamOptions, ObjectMeta, ObjectStore, ObjectStoreError, PutDirectoryOptions,
    PutFileOptions, PutObjectOptions, PutStreamOptions,
};
use crate::{aop, impl_box_from};

/// AopObjectStore 配置
#[derive(Debug, Clone, Deserialize, SmartDefault, Validate)]
#[serde(default)]
pub struct AopObjectStoreConfig {
    /// 内部 ObjectStore 配置
    #[garde(skip)]
    pub object_store: TypeOptions,

    /// AOP 配置（可选）
    #[garde(skip)]
    pub aop: Option<AopConfig>,
}

/// AOP ObjectStore 装饰器
///
/// 包装一个 ObjectStore 实现，为其所有方法添加 AOP 功能（日志记录和重试）。
pub struct AopObjectStore {
    /// 内部 ObjectStore 实例
    inner: Box<dyn ObjectStore>,

    /// AOP 配置（日志和重试）
    aop: Option<Arc<Aop>>,
}

impl AopObjectStore {
    /// 创建 AopObjectStore 实例
    ///
    /// # 参数
    ///
    /// - `config`: AopObjectStore 配置
    ///
    /// # 返回值
    ///
    /// - `Ok(Self)`: 创建成功
    /// - `Err(ObjectStoreError)`: 创建失败
    pub fn new(config: AopObjectStoreConfig) -> Result<Self, ObjectStoreError> {
        // 验证配置
        if let Err(e) = config.validate() {
            return Err(ObjectStoreError::Configuration(format!("{}", e)));
        }

        // 创建内部 ObjectStore
        let inner = create_trait_from_type_options::<dyn ObjectStore>(&config.object_store)
            .map_err(|e| ObjectStoreError::Configuration(format!("{}", e)))?;

        // 解析 AOP 配置
        let aop = match config.aop {
            Some(aop_config) => Some(
                Aop::resolve(aop_config)
                    .map_err(|e| ObjectStoreError::Configuration(format!("{}", e)))?,
            ),
            None => None,
        };

        Ok(Self { inner, aop })
    }

    /// 从已有的 ObjectStore 和 Aop 创建
    ///
    /// # 参数
    ///
    /// - `inner`: 内部 ObjectStore 实例
    /// - `aop`: AOP 实例（可选）
    pub fn from_parts(inner: Box<dyn ObjectStore>, aop: Option<Arc<Aop>>) -> Self {
        Self { inner, aop }
    }
}

#[async_trait]
impl ObjectStore for AopObjectStore {
    async fn put_object(
        &self,
        key: &str,
        value: Bytes,
        options: PutObjectOptions,
    ) -> Result<(), ObjectStoreError> {
        aop!(
            &self.aop,
            clone(value, options),
            self.inner.put_object(key, value, options).await
        )
    }

    async fn get_object(
        &self,
        key: &str,
        options: GetObjectOptions,
    ) -> Result<Bytes, ObjectStoreError> {
        aop!(
            &self.aop,
            clone(options),
            self.inner.get_object(key, options).await
        )
    }

    async fn delete_object(&self, key: &str) -> Result<(), ObjectStoreError> {
        aop!(&self.aop, self.inner.delete_object(key).await)
    }

    async fn head_object(&self, key: &str) -> Result<Option<ObjectMeta>, ObjectStoreError> {
        aop!(&self.aop, self.inner.head_object(key).await)
    }

    async fn list_objects(
        &self,
        prefix: Option<&str>,
        max_keys: Option<usize>,
    ) -> Result<Vec<ObjectMeta>, ObjectStoreError> {
        aop!(&self.aop, self.inner.list_objects(prefix, max_keys).await)
    }

    /// 流式上传不支持 AOP 重试（流是一次性消耗的），直接透传
    async fn put_stream(
        &self,
        key: &str,
        reader: Box<dyn AsyncRead + Send + Unpin>,
        size: Option<u64>,
        options: PutStreamOptions,
    ) -> Result<(), ObjectStoreError> {
        self.inner.put_stream(key, reader, size, options).await
    }

    /// 流式下载不支持 AOP 重试（流是一次性消耗的），直接透传
    async fn get_stream(
        &self,
        key: &str,
        writer: Box<dyn AsyncWrite + Send + Unpin>,
        options: GetStreamOptions,
    ) -> Result<u64, ObjectStoreError> {
        self.inner.get_stream(key, writer, options).await
    }

    async fn put_file(
        &self,
        key: &str,
        local_path: &Path,
        options: PutFileOptions,
    ) -> Result<(), ObjectStoreError> {
        aop!(
            &self.aop,
            clone(options),
            self.inner.put_file(key, local_path, options).await
        )
    }

    async fn get_file(
        &self,
        key: &str,
        local_path: &Path,
        options: GetFileOptions,
    ) -> Result<(), ObjectStoreError> {
        aop!(
            &self.aop,
            clone(options),
            self.inner.get_file(key, local_path, options).await
        )
    }

    async fn put_directory(
        &self,
        prefix: &str,
        local_dir: &Path,
        options: PutDirectoryOptions,
    ) -> Result<DirectoryTransferResult, ObjectStoreError> {
        aop!(
            &self.aop,
            clone(options),
            self.inner.put_directory(prefix, local_dir, options).await
        )
    }

    async fn get_directory(
        &self,
        prefix: &str,
        local_dir: &Path,
        options: GetDirectoryOptions,
    ) -> Result<DirectoryTransferResult, ObjectStoreError> {
        aop!(
            &self.aop,
            clone(options),
            self.inner.get_directory(prefix, local_dir, options).await
        )
    }
}

// 实现 From trait
impl From<AopObjectStoreConfig> for AopObjectStore {
    fn from(config: AopObjectStoreConfig) -> Self {
        Self::new(config).expect("Failed to create AopObjectStore")
    }
}

impl_box_from!(AopObjectStore => dyn ObjectStore);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialize() {
        let config: AopObjectStoreConfig = json5::from_str(
            r#"{
                object_store: {
                    type: "AwsS3ObjectStore",
                    options: {
                        bucket: "test-bucket",
                        region: "us-east-1"
                    }
                },
                aop: {
                    retry: {
                        max_times: 3,
                        strategy: "constant",
                        delay: "100ms"
                    }
                }
            }"#,
        )
        .unwrap();

        assert_eq!(config.object_store.type_name, "AwsS3ObjectStore");
        assert!(config.aop.is_some());
    }

    #[test]
    fn test_config_without_aop() {
        let config: AopObjectStoreConfig = json5::from_str(
            r#"{
                object_store: {
                    type: "AwsS3ObjectStore",
                    options: {
                        bucket: "test-bucket",
                        region: "us-east-1"
                    }
                }
            }"#,
        )
        .unwrap();

        assert_eq!(config.object_store.type_name, "AwsS3ObjectStore");
        assert!(config.aop.is_none());
    }

    #[test]
    fn test_config_with_aop_reference() {
        let config: AopObjectStoreConfig = json5::from_str(
            r#"{
                object_store: {
                    type: "AwsS3ObjectStore",
                    options: {
                        bucket: "test-bucket",
                        region: "us-east-1"
                    }
                },
                aop: {
                    "$instance": "main"
                }
            }"#,
        )
        .unwrap();

        assert!(config.aop.is_some());
    }
}
