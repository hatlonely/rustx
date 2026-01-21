use aws_sdk_s3::{Client, primitives::ByteStream, error::SdkError};
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_credential_types::Credentials;
use aws_config::Region;
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::sync::Arc;
use garde::Validate;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::oss::{
    ObjectStore, ObjectStoreError, ObjectMeta, PutOptions, GetOptions,
    PutStreamOptions, GetStreamOptions, PartInfo, TransferProgress,
};

/// S3 ObjectStore 配置
///
/// 凭证获取顺序（优先级从高到低）：
/// 1. `access_key_id` + `secret_access_key` - 直接配置的访问密钥
/// 2. 默认凭证链（自动检测）：
///    - 环境变量 `AWS_ACCESS_KEY_ID` 和 `AWS_SECRET_ACCESS_KEY`
///    - 共享凭证文件 `~/.aws/credentials`
///    - 共享配置文件 `~/.aws/config`
///    - ECS 容器凭证（在 ECS 中运行时通过 `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI`）
///    - EC2 实例元数据服务 IMDS（在 EC2 中运行时）
#[derive(Deserialize, Serialize, SmartDefault, Clone, Validate)]
#[serde(default)]
pub struct AwsS3ObjectStoreConfig {
    /// 存储桶名称
    #[garde(length(min = 1))]
    #[default = ""]
    pub bucket: String,

    /// AWS 区域（也可通过环境变量 `AWS_REGION` 或 `AWS_DEFAULT_REGION` 设置）
    #[garde(skip)]
    #[default = "us-east-1"]
    pub region: String,

    /// 自定义端点（用于兼容 S3 的存储，如 MinIO、阿里云 OSS S3 兼容接口等）
    #[garde(skip)]
    pub endpoint: Option<String>,

    /// 是否使用 path-style URL（用于兼容 S3 的存储，通常需要设置为 true）
    /// 当设置了 endpoint 时默认为 true
    #[garde(skip)]
    pub force_path_style: Option<bool>,

    /// Access Key ID（优先级 1，最高）
    #[garde(skip)]
    pub access_key_id: Option<String>,

    /// Secret Access Key（优先级 1，需与 access_key_id 同时配置）
    #[garde(skip)]
    pub secret_access_key: Option<String>,
}

/// S3 ObjectStore 实现
pub struct AwsS3ObjectStore {
    client: Arc<Client>,
    config: AwsS3ObjectStoreConfig,
}

impl AwsS3ObjectStore {
    /// 唯一的构造方法
    pub fn new(config: AwsS3ObjectStoreConfig) -> Result<Self, ObjectStoreError> {
        // 使用 garde 验证配置
        if let Err(errors) = config.validate() {
            return Err(ObjectStoreError::Configuration(format!("{}", errors)));
        }

        // 创建 SDK 客户端（同步创建，使用 runtime blocker）
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| ObjectStoreError::Configuration(
                format!("创建 runtime 失败: {}", e)
            ))?;

        let client = rt.block_on(async {
            Self::create_client(&config).await
        })?;

        Ok(Self {
            client: Arc::new(client),
            config,
        })
    }

    async fn create_client(config: &AwsS3ObjectStoreConfig) -> Result<Client, ObjectStoreError> {
        let mut builder = aws_config::defaults(aws_config::BehaviorVersion::latest());

        // 设置区域
        builder = builder.region(Region::new(config.region.clone()));

        // 设置凭证
        if let (Some(ak), Some(sk)) = (&config.access_key_id, &config.secret_access_key) {
            let credentials = Credentials::new(ak, sk, None, None, "custom");
            builder = builder.credentials_provider(credentials);
        }

        // 加载配置
        let sdk_config = builder.load().await;

        // 构建客户端
        if let Some(endpoint) = &config.endpoint {
            // 自定义 endpoint，用于兼容 S3 的存储（如 MinIO、阿里云 OSS 等）
            // 默认使用 path-style URL，因为大多数 S3 兼容存储需要它
            let use_path_style = config.force_path_style.unwrap_or(true);

            let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
                .endpoint_url(endpoint)
                .force_path_style(use_path_style)
                .build();

            Ok(Client::from_conf(s3_config))
        } else {
            Ok(Client::new(&sdk_config))
        }
    }
}

#[async_trait]
impl ObjectStore for AwsS3ObjectStore {
    async fn put_object(&self, key: &str, value: Bytes) -> Result<(), ObjectStoreError> {
        self.put_object_ex(key, value, PutOptions::default()).await
    }

    async fn put_object_ex(
        &self,
        key: &str,
        value: Bytes,
        options: PutOptions,
    ) -> Result<(), ObjectStoreError> {
        let mut request = self.client
            .put_object()
            .bucket(&self.config.bucket)
            .key(key)
            .body(ByteStream::from(value));

        if let Some(ct) = &options.content_type {
            request = request.content_type(ct);
        }

        if let Some(metadata) = &options.metadata {
            for (k, v) in metadata {
                request = request.metadata(k, v);
            }
        }

        request
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "S3", "put_object"))?;

        Ok(())
    }

    async fn get_object(&self, key: &str) -> Result<Bytes, ObjectStoreError> {
        self.get_object_ex(key, GetOptions::default()).await
    }

    async fn get_object_ex(&self, key: &str, options: GetOptions) -> Result<Bytes, ObjectStoreError> {
        let mut builder = self.client
            .get_object()
            .bucket(&self.config.bucket)
            .key(key);

        if let Some(range) = options.range {
            let range_header = format!("bytes={}-{}", range.start, range.end.saturating_sub(1));
            builder = builder.range(range_header);
        }

        let output = builder
            .send()
            .await
            .map_err(|e| {
                // TODO: 更精确的错误判断
                ObjectStoreError::from_provider(e, "S3", "get_object_ex")
            })?;

        let bytes = output.body
            .collect()
            .await
            .map_err(|e| ObjectStoreError::Network(e.to_string()))?
            .into_bytes();

        Ok(bytes)
    }

    async fn delete_object(&self, key: &str) -> Result<(), ObjectStoreError> {
        self.client
            .delete_object()
            .bucket(&self.config.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "S3", "delete_object"))?;

        Ok(())
    }

    async fn head_object(&self, key: &str) -> Result<Option<ObjectMeta>, ObjectStoreError> {
        match self.client
            .head_object()
            .bucket(&self.config.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(output) => {
                let last_modified = output
                    .last_modified()
                    .and_then(|dt| {
                        chrono::DateTime::from_timestamp(dt.secs(), dt.subsec_nanos())
                    })
                    .unwrap_or_else(chrono::Utc::now);

                Ok(Some(ObjectMeta {
                    key: key.to_string(),
                    size: output.content_length().unwrap_or(0) as u64,
                    last_modified,
                    etag: output.e_tag().map(|s| s.to_string()),
                    content_type: output.content_type().map(|s| s.to_string()),
                }))
            }
            Err(e) => {
                match e {
                    SdkError::ServiceError(se) if se.err().is_not_found() => Ok(None),
                    _ => Err(ObjectStoreError::from_provider(e, "S3", "head_object")),
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
        let mut continuation_token = None;
        let mut remaining = max_keys.unwrap_or(usize::MAX);

        loop {
            let mut builder = self.client.list_objects_v2().bucket(&self.config.bucket);

            if let Some(p) = prefix {
                builder = builder.prefix(p);
            }

            if let Some(token) = &continuation_token {
                builder = builder.continuation_token(token);
            }

            let page_size = remaining.min(1000);
            builder = builder.max_keys(page_size as i32);

            let output = builder
                .send()
                .await
                .map_err(|e| ObjectStoreError::from_provider(e, "S3", "list_objects"))?;

            if let Some(objects) = output.contents {
                for obj in objects {
                    if remaining == 0 {
                        break;
                    }

                    result.push(ObjectMeta {
                        key: obj.key.unwrap_or_default(),
                        size: obj.size.unwrap_or(0) as u64,
                        last_modified: obj.last_modified
                            .and_then(|dt| chrono::DateTime::from_timestamp(dt.secs(), dt.subsec_nanos()))
                            .unwrap_or_else(|| chrono::Utc::now()),
                        etag: obj.e_tag,
                        content_type: None,
                    });

                    remaining = remaining.saturating_sub(1);
                }
            }

            continuation_token = output.next_continuation_token;

            if remaining == 0 || continuation_token.is_none() {
                break;
            }
        }

        Ok(result)
    }

    // === 流式接口 ===

    async fn put_stream(
        &self,
        key: &str,
        mut reader: Box<dyn AsyncRead + Send + Unpin>,
        size: u64,
        options: PutStreamOptions,
    ) -> Result<(), ObjectStoreError> {
        // 读取所有数据到缓冲区（S3 SDK 需要知道完整大小）
        let mut buffer = Vec::with_capacity(size as usize);
        let mut total_read = 0u64;

        loop {
            let mut chunk = vec![0u8; 64 * 1024]; // 64KB chunks
            let n = reader.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..n]);
            total_read += n as u64;

            // 报告进度
            if let Some(ref callback) = options.progress_callback {
                callback.on_progress(&TransferProgress {
                    transferred_bytes: total_read,
                    total_bytes: size,
                });
            }
        }

        let mut request = self.client
            .put_object()
            .bucket(&self.config.bucket)
            .key(key)
            .body(ByteStream::from(buffer));

        if let Some(ct) = &options.content_type {
            request = request.content_type(ct);
        }

        if let Some(metadata) = &options.metadata {
            for (k, v) in metadata {
                request = request.metadata(k, v);
            }
        }

        request
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "S3", "put_stream"))?;

        Ok(())
    }

    async fn get_stream(
        &self,
        key: &str,
        mut writer: Box<dyn AsyncWrite + Send + Unpin>,
        options: GetStreamOptions,
    ) -> Result<u64, ObjectStoreError> {
        let mut builder = self.client
            .get_object()
            .bucket(&self.config.bucket)
            .key(key);

        if let Some(range) = options.range.clone() {
            let range_header = format!("bytes={}-{}", range.start, range.end.saturating_sub(1));
            builder = builder.range(range_header);
        }

        let output = builder
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "S3", "get_stream"))?;

        let content_length = output.content_length.unwrap_or(0) as u64;
        let mut byte_stream = output.body;
        let mut total_written = 0u64;

        while let Some(chunk) = byte_stream.next().await {
            let data = chunk.map_err(|e| ObjectStoreError::Network(e.to_string()))?;
            writer.write_all(&data).await?;
            total_written += data.len() as u64;

            if let Some(ref callback) = options.progress_callback {
                callback.on_progress(&TransferProgress {
                    transferred_bytes: total_written,
                    total_bytes: content_length,
                });
            }
        }

        writer.flush().await?;
        Ok(total_written)
    }

    // === 分片上传接口 ===

    async fn create_multipart_upload(
        &self,
        key: &str,
        options: PutOptions,
    ) -> Result<String, ObjectStoreError> {
        let mut request = self.client
            .create_multipart_upload()
            .bucket(&self.config.bucket)
            .key(key);

        if let Some(ct) = &options.content_type {
            request = request.content_type(ct);
        }

        if let Some(metadata) = &options.metadata {
            for (k, v) in metadata {
                request = request.metadata(k, v);
            }
        }

        let output = request
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "S3", "create_multipart_upload"))?;

        output.upload_id
            .ok_or_else(|| ObjectStoreError::MultipartUpload {
                message: "No upload_id returned".to_string(),
            })
    }

    async fn upload_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        data: Bytes,
    ) -> Result<PartInfo, ObjectStoreError> {
        let size = data.len() as u64;

        let output = self.client
            .upload_part()
            .bucket(&self.config.bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(part_number as i32)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "S3", "upload_part"))?;

        let etag = output.e_tag
            .ok_or_else(|| ObjectStoreError::MultipartUpload {
                message: "No ETag returned for part".to_string(),
            })?;

        Ok(PartInfo {
            part_number,
            etag,
            size,
        })
    }

    async fn complete_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
        parts: Vec<PartInfo>,
    ) -> Result<(), ObjectStoreError> {
        let completed_parts: Vec<CompletedPart> = parts
            .into_iter()
            .map(|p| {
                CompletedPart::builder()
                    .part_number(p.part_number as i32)
                    .e_tag(p.etag)
                    .build()
            })
            .collect();

        let completed_upload = CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        self.client
            .complete_multipart_upload()
            .bucket(&self.config.bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(completed_upload)
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "S3", "complete_multipart_upload"))?;

        Ok(())
    }

    async fn abort_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
    ) -> Result<(), ObjectStoreError> {
        self.client
            .abort_multipart_upload()
            .bucket(&self.config.bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "S3", "abort_multipart_upload"))?;

        Ok(())
    }
}

// 实现 From trait
impl From<AwsS3ObjectStoreConfig> for AwsS3ObjectStore {
    fn from(config: AwsS3ObjectStoreConfig) -> Self {
        Self::new(config).expect("Failed to create S3ObjectStore")
    }
}

impl From<Box<AwsS3ObjectStore>> for Box<dyn ObjectStore> {
    fn from(store: Box<AwsS3ObjectStore>) -> Self {
        store as Box<dyn ObjectStore>
    }
}
