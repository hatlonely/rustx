use async_trait::async_trait;
use thiserror::Error;

/// 序列化相关错误（对应 Golang 版本）
#[derive(Error, Debug)]
pub enum SerializerError {
    #[error("Serialization failed: {0}")]
    SerializationFailed(String),
    #[error("Deserialization failed: {0}")]
    DeserializationFailed(String),
}

/// 核心序列化 trait（对应 Golang Serializer[F, T] interface）
/// 
/// F: 源类型（From type）
/// T: 目标类型（To type）
#[async_trait]
pub trait Serializer<F, T>: Send + Sync {
    /// 序列化：将 F 类型转换为 T 类型
    async fn serialize(&self, from: F) -> Result<T, SerializerError>;
    
    /// 反序列化：将 T 类型转换为 F 类型  
    async fn deserialize(&self, to: T) -> Result<F, SerializerError>;
}