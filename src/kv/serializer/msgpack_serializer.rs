use std::marker::PhantomData;
use serde::{Deserialize, Serialize};
use async_trait::async_trait;
use crate::kv::serializer::core::{Serializer, SerializerError};

/// MessagePack 序列化器配置
#[derive(Deserialize, Debug, Clone)]
pub struct MsgPackSerializerConfig {
    /// 是否使用命名字段（struct字段名）
    #[serde(default = "default_named")]
    pub named: bool,
}

fn default_named() -> bool {
    true
}

impl Default for MsgPackSerializerConfig {
    fn default() -> Self {
        Self {
            named: true,
        }
    }
}

/// MessagePack 序列化器
/// 
/// 支持任意实现了 Serialize + DeserializeOwned 的类型与字节数组之间的序列化
/// MessagePack 是一种高效的二进制序列化格式
pub struct MsgPackSerializer<T> {
    config: MsgPackSerializerConfig,
    _phantom: PhantomData<T>,
}

impl<T> MsgPackSerializer<T> {
    /// 创建 MessagePack 序列化器的唯一方法
    /// 
    /// # 参数
    /// * `config` - MessagePack 序列化器配置
    pub fn new(config: MsgPackSerializerConfig) -> Self {
        Self {
            config,
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<T> Serializer<T, Vec<u8>> for MsgPackSerializer<T> 
where 
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    async fn serialize(&self, from: T) -> Result<Vec<u8>, SerializerError> {
        if self.config.named {
            rmp_serde::to_vec_named(&from)
                .map_err(|e| SerializerError::SerializationFailed(e.to_string()))
        } else {
            rmp_serde::to_vec(&from)
                .map_err(|e| SerializerError::SerializationFailed(e.to_string()))
        }
    }
    
    async fn deserialize(&self, to: Vec<u8>) -> Result<T, SerializerError> {
        rmp_serde::from_slice(&to)
            .map_err(|e| SerializerError::DeserializationFailed(e.to_string()))
    }
}

/// 支持 cfg 模块类型注册的 From trait 实现
impl<T> From<MsgPackSerializerConfig> for MsgPackSerializer<T> {
    fn from(config: MsgPackSerializerConfig) -> Self {
        MsgPackSerializer::new(config)
    }
}

impl<T> From<Box<MsgPackSerializer<T>>> for Box<dyn Serializer<T, Vec<u8>>>
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    fn from(source: Box<MsgPackSerializer<T>>) -> Self {
        source as Box<dyn Serializer<T, Vec<u8>>>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct TestData {
        name: String,
        age: u32,
    }

    #[tokio::test]
    async fn test_msgpack_serializer() {
        let config = MsgPackSerializerConfig::default();
        let serializer = MsgPackSerializer::new(config);
        
        let data = TestData {
            name: "Alice".to_string(),
            age: 30,
        };
        
        // 序列化
        let bytes = serializer.serialize(data.clone()).await.unwrap();
        
        // 反序列化
        let deserialized: TestData = serializer.deserialize(bytes).await.unwrap();
        
        assert_eq!(data, deserialized);
    }

    #[tokio::test]
    async fn test_msgpack_serializer_without_names() {
        let config = MsgPackSerializerConfig { named: false };
        let serializer = MsgPackSerializer::new(config);
        
        let data = TestData {
            name: "Bob".to_string(),
            age: 25,
        };
        
        // 序列化
        let bytes = serializer.serialize(data.clone()).await.unwrap();
        
        // 反序列化
        let deserialized: TestData = serializer.deserialize(bytes).await.unwrap();
        
        assert_eq!(data, deserialized);
    }
}