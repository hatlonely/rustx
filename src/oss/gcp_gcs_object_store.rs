// API 文档参考：
// google-cloud-storage crate (v1.6.0): https://docs.rs/google-cloud-storage/1.6.0/google_cloud_storage/
// GCP 官方文档: https://cloud.google.com/storage/docs
// GCP Rust SDK: https://docs.cloud.google.com/rust

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::sync::Arc;
use std::future::Future;
use garde::Validate;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::oss::{
    ObjectStore, ObjectStoreError, ObjectMeta, PutObjectOptions, GetObjectOptions,
    PutStreamOptions, GetStreamOptions,
};

/// GCP GCS 配置
///
/// 凭证获取顺序（优先级从高到低）：
/// 1. `service_account_key_json` - 直接配置的服务账号 JSON 内容
/// 2. `service_account_key_path` - 服务账号密钥文件路径
/// 3. 默认凭证（自动检测）：
///    - 环境变量 `GOOGLE_APPLICATION_CREDENTIALS` 指向的文件
///    - 默认位置 `~/.config/gcloud/application_default_credentials.json`
///    - GCE/GKE 实例元数据服务（在 Google Cloud 环境中运行时）
#[derive(Deserialize, Serialize, SmartDefault, Clone, Validate)]
#[serde(default)]
pub struct GcpGcsObjectStoreConfig {
    /// 存储桶名称
    #[garde(length(min = 1))]
    #[default = ""]
    pub bucket: String,

    /// 服务账号密钥文件路径（优先级 2）
    #[garde(skip)]
    pub service_account_key_path: Option<String>,

    /// 服务账号密钥 JSON 内容（优先级 1，最高）
    #[garde(skip)]
    pub service_account_key_json: Option<String>,

    /// 自定义端点（可选，用于本地模拟器如 fake-gcs-server 或私有部署）
    #[garde(skip)]
    pub endpoint: Option<String>,
}

/// GCP GCS 实现
pub struct GcpGcsObjectStore {
    /// Storage 客户端 - 用于对象读写
    storage: Arc<google_cloud_storage::client::Storage>,
    /// StorageControl 客户端 - 用于元数据和管理操作
    control: Arc<google_cloud_storage::client::StorageControl>,
    config: GcpGcsObjectStoreConfig,
}

impl GcpGcsObjectStore {
    /// 唯一的构造方法
    pub fn new(config: GcpGcsObjectStoreConfig) -> Result<Self, ObjectStoreError> {
        // 使用 garde 验证配置
        if let Err(errors) = config.validate() {
            return Err(ObjectStoreError::Configuration(format!("{}", errors)));
        }

        // 尝试获取当前 runtime 的 handle，如果不存在则创建新的
        let (storage, control) = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // 已经在 runtime 中，使用 block_in_place 避免阻塞
            tokio::task::block_in_place(|| {
                handle.block_on(async {
                    Self::create_clients(&config).await
                })
            })
        } else {
            // 不在 runtime 中，创建新的 runtime
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| ObjectStoreError::Configuration(
                    format!("创建 runtime 失败: {}", e)
                ))?;
            rt.block_on(async {
                Self::create_clients(&config).await
            })
        }?;

        Ok(Self {
            storage: Arc::new(storage),
            control: Arc::new(control),
            config,
        })
    }

    /// 创建 Storage 和 StorageControl 客户端
    async fn create_clients(config: &GcpGcsObjectStoreConfig) -> Result<(google_cloud_storage::client::Storage, google_cloud_storage::client::StorageControl), ObjectStoreError> {
        use google_cloud_storage::client::{Storage, StorageControl};

        // 创建 Storage 客户端
        let mut storage_builder = Storage::builder();

        // 配置凭证 - Storage
        if let Some(ref json_content) = config.service_account_key_json {
            let json_value: serde_json::Value = serde_json::from_str(json_content)
                .map_err(|e| ObjectStoreError::Configuration(format!("解析服务账号 JSON 失败: {}", e)))?;
            let creds = google_cloud_auth::credentials::service_account::Builder::new(json_value)
                .build()
                .map_err(|e| ObjectStoreError::Configuration(format!("创建服务账号凭证失败: {}", e)))?;
            storage_builder = storage_builder.with_credentials(creds);
        } else if let Some(ref key_path) = config.service_account_key_path {
            let json_content = std::fs::read_to_string(key_path)
                .map_err(|e| ObjectStoreError::Configuration(format!("读取服务账号密钥文件失败: {}", e)))?;
            let json_value: serde_json::Value = serde_json::from_str(&json_content)
                .map_err(|e| ObjectStoreError::Configuration(format!("解析服务账号密钥文件失败: {}", e)))?;
            let creds = google_cloud_auth::credentials::service_account::Builder::new(json_value)
                .build()
                .map_err(|e| ObjectStoreError::Configuration(format!("创建服务账号凭证失败: {}", e)))?;
            storage_builder = storage_builder.with_credentials(creds);
        } else {
            let creds = google_cloud_auth::credentials::Builder::default()
                .build()
                .map_err(|e| ObjectStoreError::Configuration(format!("加载默认凭证失败: {}", e)))?;
            storage_builder = storage_builder.with_credentials(creds);
        }

        // 设置自定义端点（用于本地模拟器）
        if let Some(ref endpoint) = config.endpoint {
            storage_builder = storage_builder.with_endpoint(endpoint);
        }

        let storage = storage_builder.build()
            .await
            .map_err(|e| ObjectStoreError::Configuration(format!("创建 Storage 客户端失败: {}", e)))?;

        // 创建 StorageControl 客户端
        let mut control_builder = StorageControl::builder();

        // 配置凭证 - StorageControl
        if let Some(ref json_content) = config.service_account_key_json {
            let json_value: serde_json::Value = serde_json::from_str(json_content)
                .map_err(|e| ObjectStoreError::Configuration(format!("解析服务账号 JSON 失败: {}", e)))?;
            let creds = google_cloud_auth::credentials::service_account::Builder::new(json_value)
                .build()
                .map_err(|e| ObjectStoreError::Configuration(format!("创建服务账号凭证失败: {}", e)))?;
            control_builder = control_builder.with_credentials(creds);
        } else if let Some(ref key_path) = config.service_account_key_path {
            let json_content = std::fs::read_to_string(key_path)
                .map_err(|e| ObjectStoreError::Configuration(format!("读取服务账号密钥文件失败: {}", e)))?;
            let json_value: serde_json::Value = serde_json::from_str(&json_content)
                .map_err(|e| ObjectStoreError::Configuration(format!("解析服务账号密钥文件失败: {}", e)))?;
            let creds = google_cloud_auth::credentials::service_account::Builder::new(json_value)
                .build()
                .map_err(|e| ObjectStoreError::Configuration(format!("创建服务账号凭证失败: {}", e)))?;
            control_builder = control_builder.with_credentials(creds);
        } else {
            let creds = google_cloud_auth::credentials::Builder::default()
                .build()
                .map_err(|e| ObjectStoreError::Configuration(format!("加载默认凭证失败: {}", e)))?;
            control_builder = control_builder.with_credentials(creds);
        }

        // 设置自定义端点（用于本地模拟器）
        if let Some(ref endpoint) = config.endpoint {
            control_builder = control_builder.with_endpoint(endpoint);
        }

        let control = control_builder.build()
            .await
            .map_err(|e| ObjectStoreError::Configuration(format!("创建 StorageControl 客户端失败: {}", e)))?;

        Ok((storage, control))
    }

    /// 获取完整的 bucket 路径
    fn bucket_path(&self) -> String {
        format!("projects/_/buckets/{}", self.config.bucket)
    }
}

/// 自定义 StreamingSource 实现，包装 AsyncRead
/// 使用 tokio::sync::Mutex 使结构体满足 Sync 要求
struct AsyncReadSource {
    reader: tokio::sync::Mutex<Box<dyn AsyncRead + Send + Unpin>>,
    size: Option<u64>,
    chunk_size: usize,
    total_read: std::sync::atomic::AtomicU64,
}

impl AsyncReadSource {
    fn new(
        reader: Box<dyn AsyncRead + Send + Unpin>,
        size: Option<u64>,
        chunk_size: usize,
    ) -> Self {
        Self {
            reader: tokio::sync::Mutex::new(reader),
            size,
            chunk_size,
            total_read: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl google_cloud_storage::streaming_source::StreamingSource for AsyncReadSource {
    type Error = std::io::Error;

    fn next(&mut self) -> impl Future<Output = Option<Result<Bytes, Self::Error>>> + Send {
        async move {
            let mut reader = self.reader.lock().await;
            let mut buffer = vec![0u8; self.chunk_size];
            let mut buffer_len = 0;

            // 读取一个 chunk
            while buffer_len < self.chunk_size {
                match reader.read(&mut buffer[buffer_len..]).await {
                    Ok(0) => break, // EOF
                    Ok(n) => buffer_len += n,
                    Err(e) => return Some(Err(e)),
                }
            }

            if buffer_len == 0 {
                return None; // 流结束
            }

            buffer.truncate(buffer_len);
            self.total_read.fetch_add(buffer_len as u64, std::sync::atomic::Ordering::Relaxed);
            Some(Ok(Bytes::from(buffer)))
        }
    }

    fn size_hint(&self) -> impl Future<Output = Result<google_cloud_storage::streaming_source::SizeHint, Self::Error>> + Send {
        let size = self.size;
        async move {
            match size {
                Some(s) => Ok(google_cloud_storage::streaming_source::SizeHint::with_exact(s)),
                None => Ok(google_cloud_storage::streaming_source::SizeHint::default()),
            }
        }
    }
}

#[async_trait]
impl ObjectStore for GcpGcsObjectStore {
    async fn put_object(
        &self,
        key: &str,
        value: Bytes,
        options: PutObjectOptions,
    ) -> Result<(), ObjectStoreError> {
        let content_type = options.content_type.as_deref()
            .unwrap_or("application/octet-stream");

        let mut write_request = self.storage
            .write_object(&self.bucket_path(), key, value)
            .set_content_type(content_type);

        if let Some(metadata) = &options.metadata {
            let metadata_vec: Vec<(&String, &String)> = metadata.iter().collect();
            write_request = write_request.set_metadata(metadata_vec);
        }

        write_request
            .send_buffered()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "GCP GCS", "put_object"))?;

        Ok(())
    }

    async fn get_object(
        &self,
        key: &str,
        options: GetObjectOptions,
    ) -> Result<Bytes, ObjectStoreError> {
        use google_cloud_storage::model_ext::ReadRange;

        let key_clone = key.to_string();

        let mut builder = self.storage.read_object(&self.bucket_path(), key);

        // 设置读取范围
        if let Some(range) = &options.range {
            builder = builder.set_read_range(ReadRange::segment(range.start, range.end - range.start));
        }

        let mut reader = builder
            .send()
            .await
            .map_err(|e| {
                let error_msg = format!("{:?}", e);
                if error_msg.contains("NotFound") || error_msg.contains("404") {
                    ObjectStoreError::NotFound { key: key_clone.clone() }
                } else {
                    ObjectStoreError::from_provider(e, "GCP GCS", "get_object")
                }
            })?;

        let mut data = Vec::new();
        while let Some(chunk) = reader.next().await.transpose()
            .map_err(|e| ObjectStoreError::from_provider(e, "GCP GCS", "get_object"))? {
            data.extend_from_slice(&chunk);
        }

        Ok(Bytes::from(data))
    }

    async fn delete_object(&self, key: &str) -> Result<(), ObjectStoreError> {
        self.control
            .delete_object()
            .set_bucket(&self.bucket_path())
            .set_object(key)
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "GCP GCS", "delete_object"))?;

        Ok(())
    }

    async fn head_object(&self, key: &str) -> Result<Option<ObjectMeta>, ObjectStoreError> {
        // 使用 StorageControl.get_object() 获取元数据
        let result = self.control
            .get_object()
            .set_bucket(&self.bucket_path())
            .set_object(key)
            .send()
            .await;

        match result {
            Ok(obj) => {
                // 将 wkt::Timestamp 转换为 chrono::DateTime<Utc>
                let last_modified = obj.update_time
                    .map(|t| {
                        chrono::DateTime::from_timestamp(t.seconds(), t.nanos() as u32)
                            .unwrap_or_else(chrono::Utc::now)
                    })
                    .unwrap_or_else(chrono::Utc::now);

                Ok(Some(ObjectMeta {
                    key: obj.name.clone(),
                    size: obj.size as u64,
                    last_modified,
                    etag: if obj.etag.is_empty() { None } else { Some(obj.etag.clone()) },
                    content_type: if obj.content_type.is_empty() { None } else { Some(obj.content_type.clone()) },
                }))
            }
            Err(e) => {
                let error_msg = format!("{:?}", e);
                if error_msg.contains("NotFound") || error_msg.contains("404") {
                    Ok(None)
                } else {
                    Err(ObjectStoreError::from_provider(e, "GCP GCS", "head_object"))
                }
            }
        }
    }

    async fn list_objects(
        &self,
        prefix: Option<&str>,
        max_keys: Option<usize>,
    ) -> Result<Vec<ObjectMeta>, ObjectStoreError> {
        let mut result = Vec::new();
        let mut page_token: Option<String> = None;
        let mut remaining = max_keys.unwrap_or(usize::MAX);

        loop {
            let mut builder = self.control
                .list_objects()
                .set_parent(&self.bucket_path());

            if let Some(p) = prefix {
                builder = builder.set_prefix(p);
            }

            // 设置分页大小（最多 1000 个）
            let page_size = remaining.min(1000) as i32;
            builder = builder.set_page_size(page_size);

            // 设置分页 token
            if let Some(ref token) = page_token {
                builder = builder.set_page_token(token);
            }

            let response = builder
                .send()
                .await
                .map_err(|e| ObjectStoreError::from_provider(e, "GCP GCS", "list_objects"))?;

            for obj in response.objects {
                if remaining == 0 {
                    break;
                }

                // 将 wkt::Timestamp 转换为 chrono::DateTime<Utc>
                let last_modified = obj.update_time
                    .map(|t| {
                        chrono::DateTime::from_timestamp(t.seconds(), t.nanos() as u32)
                            .unwrap_or_else(chrono::Utc::now)
                    })
                    .unwrap_or_else(chrono::Utc::now);

                result.push(ObjectMeta {
                    key: obj.name,
                    size: obj.size as u64,
                    last_modified,
                    etag: if obj.etag.is_empty() { None } else { Some(obj.etag) },
                    content_type: if obj.content_type.is_empty() { None } else { Some(obj.content_type) },
                });

                remaining = remaining.saturating_sub(1);
            }

            // 检查是否还有更多页
            page_token = if response.next_page_token.is_empty() {
                None
            } else {
                Some(response.next_page_token)
            };

            if remaining == 0 || page_token.is_none() {
                break;
            }
        }

        Ok(result)
    }

    async fn put_stream(
        &self,
        key: &str,
        reader: Box<dyn AsyncRead + Send + Unpin>,
        size: Option<u64>,
        options: PutStreamOptions,
    ) -> Result<(), ObjectStoreError> {
        // 使用自定义 StreamingSource 实现真正的流式上传
        let source = AsyncReadSource::new(
            reader,
            size,
            options.part_size,
        );

        let content_type = options.content_type.as_deref()
            .unwrap_or("application/octet-stream");

        self.storage
            .write_object(&self.bucket_path(), key, source)
            .set_content_type(content_type)
            .send_buffered()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "GCP GCS", "put_stream"))?;

        Ok(())
    }

    async fn get_stream(
        &self,
        key: &str,
        mut writer: Box<dyn AsyncWrite + Send + Unpin>,
        options: GetStreamOptions,
    ) -> Result<u64, ObjectStoreError> {
        use google_cloud_storage::model_ext::ReadRange;

        let key_clone = key.to_string();

        let mut builder = self.storage.read_object(&self.bucket_path(), key);

        // 设置读取范围
        if let Some(range) = &options.range {
            builder = builder.set_read_range(ReadRange::segment(range.start, range.end - range.start));
        }

        let mut reader = builder
            .send()
            .await
            .map_err(|e| {
                let error_msg = format!("{:?}", e);
                if error_msg.contains("NotFound") || error_msg.contains("404") {
                    ObjectStoreError::NotFound { key: key_clone.clone() }
                } else {
                    ObjectStoreError::from_provider(e, "GCP GCS", "get_stream")
                }
            })?;

        let mut written = 0u64;

        // 流式写入
        while let Some(chunk) = reader.next().await.transpose()
            .map_err(|e| ObjectStoreError::from_provider(e, "GCP GCS", "get_stream"))? {
            writer.write_all(&chunk).await?;
            written += chunk.len() as u64;
        }

        writer.flush().await?;
        Ok(written)
    }
}

// 实现 From trait
impl From<GcpGcsObjectStoreConfig> for GcpGcsObjectStore {
    fn from(config: GcpGcsObjectStoreConfig) -> Self {
        Self::new(config).expect("Failed to create GcpGcsObjectStore")
    }
}

impl From<Box<GcpGcsObjectStore>> for Box<dyn ObjectStore> {
    fn from(store: Box<GcpGcsObjectStore>) -> Self {
        store as Box<dyn ObjectStore>
    }
} 
