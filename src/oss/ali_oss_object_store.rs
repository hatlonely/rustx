// API 文档参考：
// aliyun-oss-rust-sdk crate: https://docs.rs/aliyun-oss-rust-sdk
// 阿里云 OSS 文档: https://help.aliyun.com/zh/oss
// 阿里云 OSS API 参考: https://help.aliyun.com/zh/oss/developer-reference/api-reference

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use chrono::{DateTime, Utc};
use garde::Validate;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use std::sync::{Arc, RwLock};

use crate::oss::{
    ObjectStore, ObjectStoreError, ObjectMeta, PutOptions, GetOptions,
    PutStreamOptions, GetStreamOptions, PartInfo, TransferProgress,
};

// ============================================================================
// 凭证相关结构
// ============================================================================

/// 阿里云凭证
#[derive(Clone, Debug)]
struct AliyunCredentials {
    access_key_id: String,
    access_key_secret: String,
    security_token: Option<String>,
    /// STS 凭证过期时间
    expiration: Option<DateTime<Utc>>,
}

impl AliyunCredentials {
    /// 检查凭证是否即将过期（提前 5 分钟刷新）
    fn is_expired(&self) -> bool {
        if let Some(exp) = self.expiration {
            let buffer = chrono::Duration::minutes(5);
            Utc::now() + buffer >= exp
        } else {
            false // 永久凭证不过期
        }
    }
}

/// ECS 元数据服务返回的凭证结构
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EcsMetadataCredentials {
    access_key_id: String,
    access_key_secret: String,
    security_token: String,
    #[serde(with = "ecs_expiration_format")]
    expiration: DateTime<Utc>,
    #[allow(dead_code)]
    code: String,
}

/// ECS 元数据返回的时间格式解析（格式：2024-01-01T00:00:00Z）
mod ecs_expiration_format {
    use chrono::{DateTime, Utc};
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(serde::de::Error::custom)
    }
}

// 列举结果（内部使用）
#[derive(Debug, Clone, PartialEq, Eq)]
struct ListResult {
    objects: Vec<ObjectMeta>,
    next_token: Option<String>,
    is_truncated: bool,
}

// 阿里云 OSS ListObjects API 响应结构
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ListBucketResult {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    prefix: Option<String>,
    #[allow(dead_code)]
    max_keys: u32,
    #[allow(dead_code)]
    delimiter: Option<String>,
    is_truncated: bool,
    next_continuation_token: Option<String>,
    contents: Vec<ObjectContent>,
}

#[derive(Debug, Deserialize)]
struct ObjectContent {
    key: String,
    last_modified: DateTime<Utc>,
    #[serde(deserialize_with = "deserialize_etag")]
    etag: Option<String>,
    size: u64,
}

fn deserialize_etag<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    // OSS 返回的 ETag 包含引号，需要去掉
    Ok(Some(s.trim_matches('"').to_string()))
}

/// 阿里云 OSS 配置
///
/// 凭证获取顺序（优先级从高到低）：
/// 1. `access_key_id` + `access_key_secret` - 直接配置的访问密钥
/// 2. 环境变量 `ALIBABA_CLOUD_ACCESS_KEY_ID` 和 `ALIBABA_CLOUD_ACCESS_KEY_SECRET`
///    （也支持旧版环境变量 `ALIYUN_OSS_ACCESS_KEY_ID` 和 `ALIYUN_OSS_ACCESS_KEY_SECRET`）
/// 3. ECS 实例角色 - 通过元数据服务 `http://100.100.100.200/latest/meta-data/ram/security-credentials/` 获取
#[derive(Deserialize, Serialize, SmartDefault, Clone, Validate)]
#[serde(default)]
pub struct AliyunOssObjectStoreConfig {
    /// 存储桶名称
    #[garde(length(min = 1))]
    #[default = ""]
    pub bucket: String,

    /// 区域端点，如 oss-cn-hangzhou.aliyuncs.com
    #[garde(skip)]
    #[default = "oss-cn-hangzhou.aliyuncs.com"]
    pub endpoint: String,

    /// Access Key ID（优先级 1，最高）
    #[garde(skip)]
    pub access_key_id: Option<String>,

    /// Access Key Secret（优先级 1，需与 access_key_id 同时配置）
    #[garde(skip)]
    pub access_key_secret: Option<String>,

    /// ECS 实例角色名称（可选，不配置时会自动从元数据服务获取）
    #[garde(skip)]
    pub ecs_ram_role: Option<String>,

    /// 是否使用 HTTPS
    #[garde(skip)]
    #[default = true]
    pub https: bool,
}

/// 阿里云 OSS 实现
pub struct AliyunOssObjectStore {
    /// 使用 Arc 包装 client，便于在 async 上下文中安全克隆
    client: RwLock<Arc<aliyun_oss_rust_sdk::oss::OSS>>,
    credentials: RwLock<AliyunCredentials>,
    config: AliyunOssObjectStoreConfig,
}

// ECS 元数据服务地址
const ECS_METADATA_BASE_URL: &str = "http://100.100.100.200/latest/meta-data/ram/security-credentials/";

impl AliyunOssObjectStore {
    /// 唯一的构造方法
    pub fn new(config: AliyunOssObjectStoreConfig) -> Result<Self, ObjectStoreError> {
        // 使用 garde 验证配置
        if let Err(errors) = config.validate() {
            return Err(ObjectStoreError::Configuration(format!("{}", errors)));
        }

        // 同步获取初始凭证
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| ObjectStoreError::Configuration(format!("创建 runtime 失败: {}", e)))?;

        let credentials = rt.block_on(Self::fetch_credentials(&config))?;

        // 创建 OSS 客户端
        let client = aliyun_oss_rust_sdk::oss::OSS::new(
            &credentials.access_key_id,
            &credentials.access_key_secret,
            &config.endpoint,
            &config.bucket,
        );

        Ok(Self {
            client: RwLock::new(Arc::new(client)),
            credentials: RwLock::new(credentials),
            config,
        })
    }

    /// 按优先级获取凭证
    async fn fetch_credentials(config: &AliyunOssObjectStoreConfig) -> Result<AliyunCredentials, ObjectStoreError> {
        // 1. 直接配置的 AK/SK
        if let (Some(ak), Some(sk)) = (&config.access_key_id, &config.access_key_secret) {
            return Ok(AliyunCredentials {
                access_key_id: ak.clone(),
                access_key_secret: sk.clone(),
                security_token: None,
                expiration: None,
            });
        }

        // 2. 环境变量（支持新旧两种命名）
        let ak_from_env = std::env::var("ALIBABA_CLOUD_ACCESS_KEY_ID")
            .or_else(|_| std::env::var("ALIYUN_OSS_ACCESS_KEY_ID"))
            .ok();
        let sk_from_env = std::env::var("ALIBABA_CLOUD_ACCESS_KEY_SECRET")
            .or_else(|_| std::env::var("ALIYUN_OSS_ACCESS_KEY_SECRET"))
            .ok();
        let token_from_env = std::env::var("ALIBABA_CLOUD_SECURITY_TOKEN").ok();

        if let (Some(ak), Some(sk)) = (ak_from_env, sk_from_env) {
            return Ok(AliyunCredentials {
                access_key_id: ak,
                access_key_secret: sk,
                security_token: token_from_env,
                expiration: None,
            });
        }

        // 3. ECS 实例角色
        Self::fetch_credentials_from_ecs_metadata(config).await
    }

    /// 从 ECS 元数据服务获取凭证
    async fn fetch_credentials_from_ecs_metadata(
        config: &AliyunOssObjectStoreConfig,
    ) -> Result<AliyunCredentials, ObjectStoreError> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .map_err(|e| ObjectStoreError::Configuration(format!("创建 HTTP 客户端失败: {}", e)))?;

        // 获取角色名称
        let role_name = if let Some(ref role) = config.ecs_ram_role {
            role.clone()
        } else {
            // 自动获取角色名称
            let resp = http_client
                .get(ECS_METADATA_BASE_URL)
                .send()
                .await
                .map_err(|e| ObjectStoreError::Configuration(
                    format!("无法连接 ECS 元数据服务，可能不在 ECS 实例上运行: {}", e)
                ))?;

            if !resp.status().is_success() {
                return Err(ObjectStoreError::Configuration(
                    format!("ECS 元数据服务返回错误: HTTP {}", resp.status())
                ));
            }

            resp.text().await
                .map_err(|e| ObjectStoreError::Configuration(format!("读取角色名称失败: {}", e)))?
                .trim()
                .to_string()
        };

        if role_name.is_empty() {
            return Err(ObjectStoreError::Configuration(
                "ECS 实例未绑定 RAM 角色".to_string()
            ));
        }

        // 获取凭证
        let url = format!("{}{}", ECS_METADATA_BASE_URL, role_name);
        let resp = http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| ObjectStoreError::Configuration(format!("获取 ECS 凭证失败: {}", e)))?;

        if !resp.status().is_success() {
            return Err(ObjectStoreError::Configuration(
                format!("获取 ECS 凭证失败: HTTP {}", resp.status())
            ));
        }

        let ecs_creds: EcsMetadataCredentials = resp.json().await
            .map_err(|e| ObjectStoreError::Configuration(format!("解析 ECS 凭证失败: {}", e)))?;

        if ecs_creds.code != "Success" {
            return Err(ObjectStoreError::Configuration(
                format!("ECS 凭证返回错误: {}", ecs_creds.code)
            ));
        }

        Ok(AliyunCredentials {
            access_key_id: ecs_creds.access_key_id,
            access_key_secret: ecs_creds.access_key_secret,
            security_token: Some(ecs_creds.security_token),
            expiration: Some(ecs_creds.expiration),
        })
    }

    /// 确保凭证有效（如果过期则刷新）
    async fn ensure_credentials(&self) -> Result<Option<String>, ObjectStoreError> {
        // 检查是否需要刷新
        let needs_refresh = {
            let creds = self.credentials.read().unwrap();
            creds.is_expired()
        };

        if needs_refresh {
            let new_creds = Self::fetch_credentials(&self.config).await?;

            // 更新客户端
            let new_client = aliyun_oss_rust_sdk::oss::OSS::new(
                &new_creds.access_key_id,
                &new_creds.access_key_secret,
                &self.config.endpoint,
                &self.config.bucket,
            );

            let security_token = new_creds.security_token.clone();

            // 更新缓存
            *self.client.write().unwrap() = Arc::new(new_client);
            *self.credentials.write().unwrap() = new_creds;

            Ok(security_token)
        } else {
            let creds = self.credentials.read().unwrap();
            Ok(creds.security_token.clone())
        }
    }

    /// 获取当前客户端的 Arc 克隆（可安全跨 await 使用）
    fn get_client(&self) -> Arc<aliyun_oss_rust_sdk::oss::OSS> {
        self.client.read().unwrap().clone()
    }

    // 分页列举: 通过 OSS ListObjects API (list-type=2) 实现
    async fn _list_objects(
        &self,
        prefix: Option<&str>,
        max_keys: usize,
        continuation_token: Option<String>,
    ) -> Result<ListResult, ObjectStoreError> {
        use aliyun_oss_rust_sdk::request::RequestBuilder;

        // 确保凭证有效
        let security_token = self.ensure_credentials().await?;

        // 构建查询参数
        let mut query_params = vec![
            ("list-type".to_string(), "2".to_string()),
            ("max-keys".to_string(), max_keys.to_string()),
        ];

        if let Some(p) = prefix {
            query_params.push(("prefix".to_string(), p.to_string()));
        }

        if let Some(token) = &continuation_token {
            query_params.push(("continuation-token".to_string(), token.clone()));
        }

        // 构建请求 URL
        let query_string = query_params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let url = if self.config.https {
            format!("https://{}.{}?{}",
                self.config.bucket,
                self.config.endpoint.replacen("https://", "", 1),
                query_string
            )
        } else {
            format!("http://{}.{}?{}",
                self.config.bucket,
                self.config.endpoint.replacen("http://", "", 1),
                query_string
            )
        };

        // 创建签名请求
        let mut builder = RequestBuilder::new();
        builder.method = aliyun_oss_rust_sdk::request::RequestType::Get;

        // 获取签名的 headers
        let (_signed_url, headers) = self.get_client()
            .build_request("/", builder)
            .map_err(|e| ObjectStoreError::Configuration(
                format!("Failed to build request: {}", e)
            ))?;

        // 使用 reqwest 发送请求
        let http_client = reqwest::Client::new();
        let mut request = http_client
            .get(&url)
            .header("Authorization", headers.get("Authorization").unwrap().to_str().unwrap())
            .header("Date", headers.get("date").unwrap().to_str().unwrap());

        // 添加 STS token（如果有）
        if let Some(ref token) = security_token {
            request = request.header("x-oss-security-token", token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "Aliyun OSS", "list_objects"))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ObjectStoreError::Configuration(
                format!("Aliyun OSS list_objects HTTP {}: {}", status, error_text)
            ));
        }

        // 解析 XML 响应
        let xml_text = response.text().await
            .map_err(|e| ObjectStoreError::Configuration(
                format!("Failed to read response: {}", e)
            ))?;

        let list_result: ListBucketResult = quick_xml::de::from_str(&xml_text)
            .map_err(|e| ObjectStoreError::Configuration(
                format!("Failed to parse XML response: {}", e)
            ))?;

        // 转换为 ObjectMeta
        let objects: Vec<ObjectMeta> = list_result.contents
            .into_iter()
            .map(|content| ObjectMeta {
                key: content.key,
                size: content.size,
                last_modified: content.last_modified,
                etag: content.etag,
                content_type: None, // ListObjects 不返回 content-type
            })
            .collect();

        Ok(ListResult {
            objects,
            next_token: list_result.next_continuation_token,
            is_truncated: list_result.is_truncated,
        })
    }
}

#[async_trait]
impl ObjectStore for AliyunOssObjectStore {
    async fn put_object(&self, key: &str, value: Bytes) -> Result<(), ObjectStoreError> {
        self.put_object_ex(key, value, PutOptions::default()).await
    }

    // 上传内存文件 API (异步): https://docs.rs/aliyun-oss-rust-sdk/aliyun_oss_rust_sdk/oss/struct.OSS.html#method.pub_object_from_buffer
    async fn put_object_ex(
        &self,
        key: &str,
        value: Bytes,
        options: PutOptions,
    ) -> Result<(), ObjectStoreError> {
        use aliyun_oss_rust_sdk::request::RequestBuilder;

        // 确保凭证有效
        let security_token = self.ensure_credentials().await?;

        let mut builder = RequestBuilder::new();

        // 添加 STS token（如果有）
        if let Some(ref token) = security_token {
            builder = builder.oss_header_put("x-oss-security-token", token);
        }

        // 设置 content-type
        if let Some(ct) = &options.content_type {
            builder = builder.with_content_type(ct);
        }

        // 设置自定义元数据 (x-oss-meta-*)
        if let Some(metadata) = &options.metadata {
            for (key, value) in metadata {
                builder = builder.oss_header_put(
                    format!("x-oss-meta-{}", key).as_str(),
                    value.as_str()
                );
            }
        }

        // 设置标签 (x-oss-tagging: key1=value1&key2=value2)
        if let Some(tags) = &options.tags {
            let tagging = tags.iter()
                .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
                .collect::<Vec<_>>()
                .join("&");
            builder = builder.oss_header_put("x-oss-tagging", &tagging);
        }

        // 直接调用异步 API
        self.get_client()
            .pub_object_from_buffer(key, value.as_ref(), builder)
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "Aliyun OSS", "put_object"))?;

        Ok(())
    }

    // 文件下载 API (异步): https://docs.rs/aliyun-oss-rust-sdk/aliyun_oss_rust_sdk/oss/struct.OSS.html#method.get_object
    async fn get_object(&self, key: &str) -> Result<Bytes, ObjectStoreError> {
        self.get_object_ex(key, GetOptions::default()).await
    }

    // 范围下载: 通过 Range header 实现
    async fn get_object_ex(&self, key: &str, options: GetOptions) -> Result<Bytes, ObjectStoreError> {
        use aliyun_oss_rust_sdk::request::RequestBuilder;

        // 确保凭证有效
        let security_token = self.ensure_credentials().await?;

        let key_clone = key.to_string();
        let mut builder = RequestBuilder::new();

        // 添加 STS token（如果有）
        if let Some(ref token) = security_token {
            builder = builder.oss_header_put("x-oss-security-token", token);
        }

        // 设置 Range header: bytes=start-end
        if let Some(range) = options.range {
            builder.headers.insert(
                "Range".to_string(),
                format!("bytes={}-{}", range.start, range.end.saturating_sub(1))
            );
        }

        let bytes = self.get_client()
            .get_object(key, builder)
            .await
            .map_err(|e| {
                let error_msg = format!("{:?}", e);
                if error_msg.contains("NoSuchKey") || error_msg.contains("404") {
                    ObjectStoreError::NotFound { key: key_clone }
                } else {
                    ObjectStoreError::from_provider(e, "Aliyun OSS", "get_object_ex")
                }
            })?;

        Ok(Bytes::from(bytes))
    }

    // 文件删除 API (异步): https://docs.rs/aliyun-oss-rust-sdk/aliyun_oss_rust_sdk/oss/struct.OSS.html#method.delete_object
    async fn delete_object(&self, key: &str) -> Result<(), ObjectStoreError> {
        use aliyun_oss_rust_sdk::request::RequestBuilder;

        // 确保凭证有效
        let security_token = self.ensure_credentials().await?;

        let mut builder = RequestBuilder::new();

        // 添加 STS token（如果有）
        if let Some(ref token) = security_token {
            builder = builder.oss_header_put("x-oss-security-token", token);
        }

        self.get_client()
            .delete_object(key, builder)
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "Aliyun OSS", "delete_object"))?;

        Ok(())
    }

    // 检查对象是否存在 (异步): 通过获取元信息判断
    async fn head_object(&self, key: &str) -> Result<bool, ObjectStoreError> {
        use aliyun_oss_rust_sdk::request::RequestBuilder;

        // 确保凭证有效
        let security_token = self.ensure_credentials().await?;

        let mut builder = RequestBuilder::new();

        // 添加 STS token（如果有）
        if let Some(ref token) = security_token {
            builder = builder.oss_header_put("x-oss-security-token", token);
        }

        match self.get_client().get_object_metadata(key, builder).await {
            Ok(_) => Ok(true),
            Err(e) => {
                let error_msg = format!("{:?}", e);
                if error_msg.contains("NoSuchKey") || error_msg.contains("404") {
                    Ok(false)
                } else {
                    Err(ObjectStoreError::from_provider(e, "Aliyun OSS", "head_object"))
                }
            }
        }
    }

    // 列举对象: 循环调用分页 API，直到达到 limit 或没有更多数据
    // https://help.aliyun.com/zh/oss/developer-reference/listobjects
    async fn list_objects(
        &self,
        prefix: Option<&str>,
        max_keys: Option<usize>,
    ) -> Result<Vec<ObjectMeta>, ObjectStoreError> {
        let mut result = Vec::new();
        let mut continuation_token = None;
        let mut remaining = max_keys.unwrap_or(usize::MAX);

        loop {
            // 每次请求的 page_size 不超过 1000（OSS 限制）
            let page_size = remaining.min(1000);

            let page_result = self._list_objects(prefix, page_size, continuation_token).await?;

            // 累积结果
            for obj in page_result.objects {
                if remaining == 0 {
                    break;
                }
                result.push(obj);
                remaining = remaining.saturating_sub(1);
            }

            // 检查是否还有更多数据
            continuation_token = page_result.next_token;

            if remaining == 0 || !page_result.is_truncated || continuation_token.is_none() {
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
        use aliyun_oss_rust_sdk::request::RequestBuilder;

        // 确保凭证有效
        let security_token = self.ensure_credentials().await?;

        // 读取所有数据到缓冲区
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

        let mut builder = RequestBuilder::new();

        // 添加 STS token（如果有）
        if let Some(ref token) = security_token {
            builder = builder.oss_header_put("x-oss-security-token", token);
        }

        if let Some(ct) = &options.content_type {
            builder = builder.with_content_type(ct);
        }

        if let Some(metadata) = &options.metadata {
            for (k, v) in metadata {
                builder = builder.oss_header_put(
                    format!("x-oss-meta-{}", k).as_str(),
                    v.as_str()
                );
            }
        }

        self.get_client()
            .pub_object_from_buffer(key, &buffer, builder)
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "Aliyun OSS", "put_stream"))?;

        Ok(())
    }

    async fn get_stream(
        &self,
        key: &str,
        mut writer: Box<dyn AsyncWrite + Send + Unpin>,
        options: GetStreamOptions,
    ) -> Result<u64, ObjectStoreError> {
        use aliyun_oss_rust_sdk::request::RequestBuilder;

        // 确保凭证有效
        let security_token = self.ensure_credentials().await?;

        let key_clone = key.to_string();
        let mut builder = RequestBuilder::new();

        // 添加 STS token（如果有）
        if let Some(ref token) = security_token {
            builder = builder.oss_header_put("x-oss-security-token", token);
        }

        if let Some(range) = &options.range {
            builder.headers.insert(
                "Range".to_string(),
                format!("bytes={}-{}", range.start, range.end.saturating_sub(1))
            );
        }

        let bytes = self.get_client()
            .get_object(key, builder)
            .await
            .map_err(|e| {
                let error_msg = format!("{:?}", e);
                if error_msg.contains("NoSuchKey") || error_msg.contains("404") {
                    ObjectStoreError::NotFound { key: key_clone }
                } else {
                    ObjectStoreError::from_provider(e, "Aliyun OSS", "get_stream")
                }
            })?;

        let total_size = bytes.len() as u64;
        let mut written = 0u64;

        // 分块写入
        for chunk in bytes.chunks(64 * 1024) {
            writer.write_all(chunk).await?;
            written += chunk.len() as u64;

            if let Some(ref callback) = options.progress_callback {
                callback.on_progress(&TransferProgress {
                    transferred_bytes: written,
                    total_bytes: total_size,
                });
            }
        }

        writer.flush().await?;
        Ok(total_size)
    }

    // === 分片上传接口 ===
    // 阿里云 OSS 分片上传 API: https://help.aliyun.com/zh/oss/developer-reference/multipart-upload

    async fn create_multipart_upload(
        &self,
        key: &str,
        options: PutOptions,
    ) -> Result<String, ObjectStoreError> {
        use aliyun_oss_rust_sdk::request::RequestBuilder;

        // 确保凭证有效
        let security_token = self.ensure_credentials().await?;

        // 构建请求 URL
        let url = if self.config.https {
            format!("https://{}.{}/{}?uploads",
                self.config.bucket,
                self.config.endpoint.replacen("https://", "", 1),
                urlencoding::encode(key)
            )
        } else {
            format!("http://{}.{}/{}?uploads",
                self.config.bucket,
                self.config.endpoint.replacen("http://", "", 1),
                urlencoding::encode(key)
            )
        };

        let mut builder = RequestBuilder::new();
        builder.method = aliyun_oss_rust_sdk::request::RequestType::Post;

        if let Some(ct) = &options.content_type {
            builder = builder.with_content_type(ct);
        }

        // 获取签名的 headers
        let resource = format!("/{}?uploads", key);
        let (_signed_url, headers) = self.get_client()
            .build_request(&resource, builder)
            .map_err(|e| ObjectStoreError::Configuration(
                format!("Failed to build request: {}", e)
            ))?;

        let http_client = reqwest::Client::new();
        let mut request = http_client
            .post(&url)
            .header("Authorization", headers.get("Authorization").unwrap().to_str().unwrap())
            .header("Date", headers.get("date").unwrap().to_str().unwrap());

        // 添加 STS token（如果有）
        if let Some(ref token) = security_token {
            request = request.header("x-oss-security-token", token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "Aliyun OSS", "create_multipart_upload"))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ObjectStoreError::MultipartUpload {
                message: format!("HTTP {}: {}", status, error_text)
            });
        }

        // 解析 XML 响应获取 UploadId
        let xml_text = response.text().await
            .map_err(|e| ObjectStoreError::Network(e.to_string()))?;

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct InitiateMultipartUploadResult {
            upload_id: String,
        }

        let result: InitiateMultipartUploadResult = quick_xml::de::from_str(&xml_text)
            .map_err(|e| ObjectStoreError::MultipartUpload {
                message: format!("Failed to parse XML: {}", e)
            })?;

        Ok(result.upload_id)
    }

    async fn upload_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        data: Bytes,
    ) -> Result<PartInfo, ObjectStoreError> {
        use aliyun_oss_rust_sdk::request::RequestBuilder;

        // 确保凭证有效
        let security_token = self.ensure_credentials().await?;

        let size = data.len() as u64;

        // 构建请求 URL
        let url = if self.config.https {
            format!("https://{}.{}/{}?partNumber={}&uploadId={}",
                self.config.bucket,
                self.config.endpoint.replacen("https://", "", 1),
                urlencoding::encode(key),
                part_number,
                urlencoding::encode(upload_id)
            )
        } else {
            format!("http://{}.{}/{}?partNumber={}&uploadId={}",
                self.config.bucket,
                self.config.endpoint.replacen("http://", "", 1),
                urlencoding::encode(key),
                part_number,
                urlencoding::encode(upload_id)
            )
        };

        let mut builder = RequestBuilder::new();
        builder.method = aliyun_oss_rust_sdk::request::RequestType::Put;

        // 获取签名的 headers
        let resource = format!("/{}?partNumber={}&uploadId={}", key, part_number, upload_id);
        let (_signed_url, headers) = self.get_client()
            .build_request(&resource, builder)
            .map_err(|e| ObjectStoreError::Configuration(
                format!("Failed to build request: {}", e)
            ))?;

        let http_client = reqwest::Client::new();
        let mut request = http_client
            .put(&url)
            .header("Authorization", headers.get("Authorization").unwrap().to_str().unwrap())
            .header("Date", headers.get("date").unwrap().to_str().unwrap())
            .header("Content-Length", data.len())
            .body(data.to_vec());

        // 添加 STS token（如果有）
        if let Some(ref token) = security_token {
            request = request.header("x-oss-security-token", token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "Aliyun OSS", "upload_part"))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ObjectStoreError::MultipartUpload {
                message: format!("HTTP {}: {}", status, error_text)
            });
        }

        // 从响应头获取 ETag
        let etag = response.headers()
            .get("ETag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim_matches('"').to_string())
            .ok_or_else(|| ObjectStoreError::MultipartUpload {
                message: "No ETag in response".to_string()
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
        use aliyun_oss_rust_sdk::request::RequestBuilder;

        // 确保凭证有效
        let security_token = self.ensure_credentials().await?;

        // 构建请求 URL
        let url = if self.config.https {
            format!("https://{}.{}/{}?uploadId={}",
                self.config.bucket,
                self.config.endpoint.replacen("https://", "", 1),
                urlencoding::encode(key),
                urlencoding::encode(upload_id)
            )
        } else {
            format!("http://{}.{}/{}?uploadId={}",
                self.config.bucket,
                self.config.endpoint.replacen("http://", "", 1),
                urlencoding::encode(key),
                urlencoding::encode(upload_id)
            )
        };

        // 构建 XML body
        let mut xml_parts = String::from("<CompleteMultipartUpload>");
        for part in &parts {
            xml_parts.push_str(&format!(
                "<Part><PartNumber>{}</PartNumber><ETag>\"{}\"</ETag></Part>",
                part.part_number, part.etag
            ));
        }
        xml_parts.push_str("</CompleteMultipartUpload>");

        let mut builder = RequestBuilder::new();
        builder.method = aliyun_oss_rust_sdk::request::RequestType::Post;

        // 获取签名的 headers
        let resource = format!("/{}?uploadId={}", key, upload_id);
        let (_signed_url, headers) = self.get_client()
            .build_request(&resource, builder)
            .map_err(|e| ObjectStoreError::Configuration(
                format!("Failed to build request: {}", e)
            ))?;

        let http_client = reqwest::Client::new();
        let mut request = http_client
            .post(&url)
            .header("Authorization", headers.get("Authorization").unwrap().to_str().unwrap())
            .header("Date", headers.get("date").unwrap().to_str().unwrap())
            .header("Content-Type", "application/xml")
            .body(xml_parts);

        // 添加 STS token（如果有）
        if let Some(ref token) = security_token {
            request = request.header("x-oss-security-token", token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "Aliyun OSS", "complete_multipart_upload"))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ObjectStoreError::MultipartUpload {
                message: format!("HTTP {}: {}", status, error_text)
            });
        }

        Ok(())
    }

    async fn abort_multipart_upload(
        &self,
        key: &str,
        upload_id: &str,
    ) -> Result<(), ObjectStoreError> {
        use aliyun_oss_rust_sdk::request::RequestBuilder;

        // 确保凭证有效
        let security_token = self.ensure_credentials().await?;

        // 构建请求 URL
        let url = if self.config.https {
            format!("https://{}.{}/{}?uploadId={}",
                self.config.bucket,
                self.config.endpoint.replacen("https://", "", 1),
                urlencoding::encode(key),
                urlencoding::encode(upload_id)
            )
        } else {
            format!("http://{}.{}/{}?uploadId={}",
                self.config.bucket,
                self.config.endpoint.replacen("http://", "", 1),
                urlencoding::encode(key),
                urlencoding::encode(upload_id)
            )
        };

        let mut builder = RequestBuilder::new();
        builder.method = aliyun_oss_rust_sdk::request::RequestType::Delete;

        // 获取签名的 headers
        let resource = format!("/{}?uploadId={}", key, upload_id);
        let (_signed_url, headers) = self.get_client()
            .build_request(&resource, builder)
            .map_err(|e| ObjectStoreError::Configuration(
                format!("Failed to build request: {}", e)
            ))?;

        let http_client = reqwest::Client::new();
        let mut request = http_client
            .delete(&url)
            .header("Authorization", headers.get("Authorization").unwrap().to_str().unwrap())
            .header("Date", headers.get("date").unwrap().to_str().unwrap());

        // 添加 STS token（如果有）
        if let Some(ref token) = security_token {
            request = request.header("x-oss-security-token", token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ObjectStoreError::from_provider(e, "Aliyun OSS", "abort_multipart_upload"))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ObjectStoreError::MultipartUpload {
                message: format!("HTTP {}: {}", status, error_text)
            });
        }

        Ok(())
    }
}

// 实现 From trait
impl From<AliyunOssObjectStoreConfig> for AliyunOssObjectStore {
    fn from(config: AliyunOssObjectStoreConfig) -> Self {
        Self::new(config).expect("Failed to create AliyunOssObjectStore")
    }
}

impl From<Box<AliyunOssObjectStore>> for Box<dyn ObjectStore> {
    fn from(store: Box<AliyunOssObjectStore>) -> Self {
        store as Box<dyn ObjectStore>
    }
}
