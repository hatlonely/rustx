// 核心配置类型和接口定义

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// 类型选项结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeOptions {
    #[serde(rename = "type")]
    pub type_name: String,
    pub options: JsonValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_options_creation() {
        let type_options = TypeOptions {
            type_name: "test_type".to_string(),
            options: serde_json::json!({
                "id": 42,
                "name": "test_config",
                "active": false
            }),
        };

        assert_eq!(type_options.type_name, "test_type");
        assert_eq!(type_options.options["id"], JsonValue::Number(42.into()));
        assert_eq!(type_options.options["name"], JsonValue::String("test_config".to_string()));
        assert_eq!(type_options.options["active"], JsonValue::Bool(false));
    }

    #[test]
    fn test_type_options_serialization() {
        let type_options = TypeOptions {
            type_name: "serialization_test".to_string(),
            options: serde_json::json!({
                "key": "value",
                "number": 123,
                "array": [1, 2, 3]
            }),
        };

        // 测试序列化
        let serialized = serde_json::to_string(&type_options).unwrap();
        assert!(serialized.contains("serialization_test"));
        assert!(serialized.contains("key"));
        assert!(serialized.contains("value"));

        // 测试反序列化
        let deserialized: TypeOptions = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.type_name, type_options.type_name);
        assert_eq!(deserialized.options, type_options.options);
    }
}