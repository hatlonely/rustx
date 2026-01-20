use std::marker::PhantomData;
use serde::{Deserialize, Serialize};
use crate::kv::serializer::core::{Serializer, SerializerError};

/// JSON 序列化器配置
#[derive(Deserialize, Debug, Clone)]
pub struct JsonSerializerConfig {
    /// 是否格式化输出（美化 JSON）
    #[serde(default)]
    pub pretty: bool,
}

impl Default for JsonSerializerConfig {
    fn default() -> Self {
        Self {
            pretty: false,
        }
    }
}

/// JSON 序列化器
/// 
/// 支持任意实现了 Serialize + DeserializeOwned 的类型与字节数组之间的序列化
pub struct JsonSerializer<T> {
    config: JsonSerializerConfig,
    _phantom: PhantomData<T>,
}

impl<T> JsonSerializer<T> {
    /// 创建 JSON 序列化器的唯一方法
    /// 
    /// # 参数
    /// * `config` - JSON 序列化器配置
    pub fn new(config: JsonSerializerConfig) -> Self {
        Self {
            config,
            _phantom: PhantomData,
        }
    }
}

impl<T> Serializer<T, Vec<u8>> for JsonSerializer<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    fn serialize(&self, from: T) -> Result<Vec<u8>, SerializerError> {
        if self.config.pretty {
            serde_json::to_vec_pretty(&from)
                .map_err(|e| SerializerError::SerializationFailed(e.to_string()))
        } else {
            serde_json::to_vec(&from)
                .map_err(|e| SerializerError::SerializationFailed(e.to_string()))
        }
    }

    fn deserialize(&self, to: Vec<u8>) -> Result<T, SerializerError> {
        serde_json::from_slice(&to)
            .map_err(|e| SerializerError::DeserializationFailed(e.to_string()))
    }
}

/// 支持 cfg 模块类型注册的 From trait 实现
impl<T> From<JsonSerializerConfig> for JsonSerializer<T> {
    fn from(config: JsonSerializerConfig) -> Self {
        JsonSerializer::new(config)
    }
}

impl<T> From<Box<JsonSerializer<T>>> for Box<dyn Serializer<T, Vec<u8>>>
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    fn from(source: Box<JsonSerializer<T>>) -> Self {
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

    #[test]
    fn test_json_serializer() {
        let config = JsonSerializerConfig::default();
        let serializer = JsonSerializer::new(config);

        let data = TestData {
            name: "Alice".to_string(),
            age: 30,
        };

        // 序列化
        let bytes = serializer.serialize(data.clone()).unwrap();

        // 反序列化
        let deserialized: TestData = serializer.deserialize(bytes).unwrap();

        assert_eq!(data, deserialized);
    }

    #[test]
    fn test_json_serializer_pretty() {
        let config = JsonSerializerConfig { pretty: true };
        let serializer = JsonSerializer::new(config);

        let data = TestData {
            name: "Alice".to_string(),
            age: 30,
        };

        let bytes = serializer.serialize(data).unwrap();
        let json_str = String::from_utf8(bytes).unwrap();

        // 验证输出包含换行符（美化格式）
        assert!(json_str.contains('\n'));
    }
}