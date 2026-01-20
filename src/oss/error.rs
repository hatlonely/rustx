use thiserror::Error;

/// 对象存储统一错误类型
#[derive(Error, Debug)]
pub enum ObjectStoreError {
    #[error("对象不存在: {key}")]
    NotFound { key: String },

    #[error("认证失败: {0}")]
    Authentication(String),

    #[error("权限不足: {0}")]
    PermissionDenied(String),

    #[error("网络错误: {0}")]
    Network(String),

    #[error("无效参数: {0}")]
    InvalidInput(String),

    #[error("限流: {0}")]
    RateLimited(String),

    #[error("配置错误: {0}")]
    Configuration(String),

    #[error("厂商错误 [{provider}]: {message}")]
    Provider {
        provider: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("文件已存在: {path}")]
    FileExists { path: String },

    #[error("目录不存在: {path}")]
    DirectoryNotFound { path: String },

    #[error("不是目录: {path}")]
    NotADirectory { path: String },

    #[error("分片上传失败: {message}")]
    MultipartUpload { message: String },
}

impl ObjectStoreError {
    /// 从厂商 SDK 错误转换
    pub fn from_provider<E>(err: E, provider: &str, context: &str) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        ObjectStoreError::Provider {
            provider: provider.to_string(),
            message: context.to_string(),
            source: Some(Box::new(err)),
        }
    }
}
