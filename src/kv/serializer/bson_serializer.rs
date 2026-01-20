use crate::kv::serializer::core::{Serializer, SerializerError};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

/// BSON 序列化器配置
#[derive(Deserialize, Debug, Clone)]
pub struct BsonSerializerConfig {}
impl Default for BsonSerializerConfig {
    fn default() -> Self {
        Self {}
    }
}

/// BSON 序列化器
///
/// 支持任意实现了 Serialize + DeserializeOwned 的类型与字节数组之间的序列化
/// BSON（Binary JSON）是 MongoDB 使用的二进制序列化格式
pub struct BsonSerializer<T> {
    _phantom: PhantomData<T>,
}

impl<T> BsonSerializer<T> {
    /// 创建 BSON 序列化器的唯一方法
    ///
    /// # 参数
    /// * `_` - BSON 序列化器配置
    pub fn new(_: BsonSerializerConfig) -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T> Serializer<T, Vec<u8>> for BsonSerializer<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    fn serialize(&self, from: T) -> Result<Vec<u8>, SerializerError> {
        // 首先将对象序列化为 BSON Document
        let doc = bson::to_document(&from)
            .map_err(|e| SerializerError::SerializationFailed(e.to_string()))?;

        // 然后将 Document 序列化为字节数组
        let mut buf = Vec::new();
        doc.to_writer(&mut buf)
            .map_err(|e| SerializerError::SerializationFailed(e.to_string()))?;

        Ok(buf)
    }

    fn deserialize(&self, to: Vec<u8>) -> Result<T, SerializerError> {
        // 首先从字节数组反序列化为 BSON Document
        let doc = bson::Document::from_reader(&*to)
            .map_err(|e| SerializerError::DeserializationFailed(e.to_string()))?;

        // 然后将 Document 反序列化为目标类型
        bson::from_document(doc).map_err(|e| SerializerError::DeserializationFailed(e.to_string()))
    }
}

/// 支持 cfg 模块类型注册的 From trait 实现
impl<T> From<BsonSerializerConfig> for BsonSerializer<T> {
    fn from(config: BsonSerializerConfig) -> Self {
        BsonSerializer::new(config)
    }
}

impl<T> From<Box<BsonSerializer<T>>> for Box<dyn Serializer<T, Vec<u8>>>
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    fn from(source: Box<BsonSerializer<T>>) -> Self {
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
        active: bool,
    }

    #[test]
    fn test_bson_serializer() {
        let config = BsonSerializerConfig::default();
        let serializer = BsonSerializer::new(config);

        let data = TestData {
            name: "Alice".to_string(),
            age: 30,
            active: true,
        };

        // 序列化
        let bytes = serializer.serialize(data.clone()).unwrap();

        // 反序列化
        let deserialized: TestData = serializer.deserialize(bytes).unwrap();

        assert_eq!(data, deserialized);
    }

    #[test]
    fn test_bson_serializer_with_custom_config() {
        let config = BsonSerializerConfig {};
        let serializer = BsonSerializer::new(config);

        let data = TestData {
            name: "Bob".to_string(),
            age: 25,
            active: false,
        };

        // 序列化
        let bytes = serializer.serialize(data.clone()).unwrap();

        // 反序列化
        let deserialized: TestData = serializer.deserialize(bytes).unwrap();

        assert_eq!(data, deserialized);
    }
}
