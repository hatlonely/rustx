// TypeOptions 序列化相关实现

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// 类型选项结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeOptions {
    #[serde(rename = "type")]
    pub type_name: String,
    pub options: JsonValue,
}

/// TypeOptions 的便利函数 - 从各种格式创建和导出
impl TypeOptions {
    /// 从 JSON 字符串创建 TypeOptions（支持 JSON5 格式）
    pub fn from_json(json_str: &str) -> Result<Self> {
        // 使用 json5 解析（支持注释、尾随逗号、未引用的键等）
        Ok(json5::from_str(json_str)?)
    }

    /// 从 YAML 字符串创建 TypeOptions
    pub fn from_yaml(yaml_str: &str) -> Result<Self> {
        Ok(serde_yaml::from_str(yaml_str)?)
    }

    /// 从 TOML 字符串创建 TypeOptions
    pub fn from_toml(toml_str: &str) -> Result<Self> {
        Ok(toml::from_str(toml_str)?)
    }

    /// 导出为 JSON 字符串
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// 导出为 YAML 字符串
    pub fn to_yaml(&self) -> Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    /// 导出为 TOML 字符串
    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }
}

// 手动实现 PartialEq，因为 JsonValue 没有实现 PartialEq
impl PartialEq for TypeOptions {
    fn eq(&self, other: &Self) -> bool {
        // 1. 比较 type_name
        if self.type_name != other.type_name {
            return false;
        }

        // 2. 比较 options (JsonValue)
        json_value_equals(&self.options, &other.options)
    }
}

/// 辅助函数：比较两个 JsonValue 是否相等
///
/// 这个函数实现了深度相等比较，处理各种 JSON 类型：
/// - Null
/// - Bool
/// - Number (整数、浮点数)
/// - String
/// - Array (递归比较元素)
/// - Object (递归比较所有键值对)
fn json_value_equals(a: &JsonValue, b: &JsonValue) -> bool {
    match (a, b) {
        // Null 比较
        (JsonValue::Null, JsonValue::Null) => true,

        // Bool 比较
        (JsonValue::Bool(a_bool), JsonValue::Bool(b_bool)) => a_bool == b_bool,

        // Number 比较
        // 需要处理 i64, u64, f64 的情况
        (JsonValue::Number(a_num), JsonValue::Number(b_num)) => {
            // 尝试作为 i64 比较
            if let (Some(a_i64), Some(b_i64)) = (a_num.as_i64(), b_num.as_i64()) {
                a_i64 == b_i64
            }
            // 尝试作为 u64 比较
            else if let (Some(a_u64), Some(b_u64)) = (a_num.as_u64(), b_num.as_u64()) {
                a_u64 == b_u64
            }
            // 作为 f64 比较（需要处理 NaN）
            else {
                match (a_num.as_f64(), b_num.as_f64()) {
                    (Some(a_f64), Some(b_f64)) => {
                        // 浮点数比较，处理 NaN
                        if a_f64.is_nan() && b_f64.is_nan() {
                            true
                        } else {
                            a_f64 == b_f64
                        }
                    }
                    _ => false,
                }
            }
        }

        // String 比较
        (JsonValue::String(a_str), JsonValue::String(b_str)) => a_str == b_str,

        // Array 比较（递归）
        (JsonValue::Array(a_arr), JsonValue::Array(b_arr)) => {
            if a_arr.len() != b_arr.len() {
                return false;
            }
            a_arr.iter()
                .zip(b_arr.iter())
                .all(|(a_item, b_item)| json_value_equals(a_item, b_item))
        }

        // Object 比较（递归比较所有键值对）
        (JsonValue::Object(a_obj), JsonValue::Object(b_obj)) => {
            if a_obj.len() != b_obj.len() {
                return false;
            }

            // 检查 a 的所有 key 都在 b 中且值相等
            for (key, a_value) in a_obj.iter() {
                match b_obj.get(key) {
                    Some(b_value) => {
                        if !json_value_equals(a_value, b_value) {
                            return false;
                        }
                    }
                    None => return false,
                }
            }

            true
        }

        // 类型不匹配
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value as JsonValue;

    #[test]
    fn test_type_options_partial_eq_same() {
        let opts1 = TypeOptions {
            type_name: "TestType".to_string(),
            options: serde_json::json!({
                "key1": "value1",
                "key2": 42,
                "key3": true,
                "key4": null
            }),
        };

        let opts2 = TypeOptions {
            type_name: "TestType".to_string(),
            options: serde_json::json!({
                "key1": "value1",
                "key2": 42,
                "key3": true,
                "key4": null
            }),
        };

        assert_eq!(opts1, opts2);
    }

    #[test]
    fn test_type_options_partial_eq_different_type_name() {
        let opts1 = TypeOptions {
            type_name: "TypeA".to_string(),
            options: serde_json::json!({ "key": "value" }),
        };

        let opts2 = TypeOptions {
            type_name: "TypeB".to_string(),
            options: serde_json::json!({ "key": "value" }),
        };

        assert_ne!(opts1, opts2);
    }

    #[test]
    fn test_type_options_partial_eq_different_options() {
        let opts1 = TypeOptions {
            type_name: "TestType".to_string(),
            options: serde_json::json!({ "key": "value1" }),
        };

        let opts2 = TypeOptions {
            type_name: "TestType".to_string(),
            options: serde_json::json!({ "key": "value2" }),
        };

        assert_ne!(opts1, opts2);
    }

    #[test]
    fn test_json_value_equals_null() {
        assert!(json_value_equals(&JsonValue::Null, &JsonValue::Null));
        assert!(!json_value_equals(&JsonValue::Null, &JsonValue::Bool(false)));
    }

    #[test]
    fn test_json_value_equals_bool() {
        assert!(json_value_equals(&JsonValue::Bool(true), &JsonValue::Bool(true)));
        assert!(json_value_equals(&JsonValue::Bool(false), &JsonValue::Bool(false)));
        assert!(!json_value_equals(&JsonValue::Bool(true), &JsonValue::Bool(false)));
    }

    #[test]
    fn test_json_value_equals_number() {
        // 整数比较
        assert!(json_value_equals(
            &JsonValue::Number(42.into()),
            &JsonValue::Number(42.into())
        ));
        assert!(!json_value_equals(
            &JsonValue::Number(42.into()),
            &JsonValue::Number(43.into())
        ));

        // 浮点数比较（通过字符串解析）
        let float1: JsonValue = serde_json::from_str("3.14").unwrap();
        let float2: JsonValue = serde_json::from_str("3.14").unwrap();
        let float3: JsonValue = serde_json::from_str("3.141").unwrap();

        assert!(json_value_equals(&float1, &float2));
        assert!(!json_value_equals(&float1, &float3));

        // 注意：JSON 标准不支持 NaN 和 Infinity
        // 所以我们不需要测试这些情况
    }

    #[test]
    fn test_json_value_equals_string() {
        assert!(json_value_equals(
            &JsonValue::String("hello".to_string()),
            &JsonValue::String("hello".to_string())
        ));
        assert!(!json_value_equals(
            &JsonValue::String("hello".to_string()),
            &JsonValue::String("world".to_string())
        ));
    }

    #[test]
    fn test_json_value_equals_array() {
        let arr1 = serde_json::json!([1, 2, 3]);
        let arr2 = serde_json::json!([1, 2, 3]);
        let arr3 = serde_json::json!([1, 2, 4]);

        assert!(json_value_equals(&arr1, &arr2));
        assert!(!json_value_equals(&arr1, &arr3));

        // 不同长度
        let arr4 = serde_json::json!([1, 2]);
        assert!(!json_value_equals(&arr1, &arr4));
    }

    #[test]
    fn test_json_value_equals_object() {
        let obj1 = serde_json::json!({
            "name": "Alice",
            "age": 30,
            "active": true
        });

        let obj2 = serde_json::json!({
            "name": "Alice",
            "age": 30,
            "active": true
        });

        let obj3 = serde_json::json!({
            "name": "Alice",
            "age": 31,  // 不同的值
            "active": true
        });

        assert!(json_value_equals(&obj1, &obj2));
        assert!(!json_value_equals(&obj1, &obj3));

        // 缺少 key
        let obj4 = serde_json::json!({
            "name": "Alice",
            "age": 30
        });
        assert!(!json_value_equals(&obj1, &obj4));
    }

    #[test]
    fn test_json_value_equals_nested() {
        let nested1 = serde_json::json!({
            "user": {
                "name": "Bob",
                "tags": ["developer", "rust"]
            },
            "settings": {
                "theme": "dark"
            }
        });

        let nested2 = serde_json::json!({
            "user": {
                "name": "Bob",
                "tags": ["developer", "rust"]
            },
            "settings": {
                "theme": "dark"
            }
        });

        assert!(json_value_equals(&nested1, &nested2));
    }

    #[test]
    fn test_json_value_equals_type_mismatch() {
        // 不同类型应该不相等
        assert!(!json_value_equals(&JsonValue::Null, &JsonValue::Bool(false)));
        assert!(!json_value_equals(&JsonValue::Bool(true), &JsonValue::String("true".to_string())));
        assert!(!json_value_equals(&JsonValue::Number(42.into()), &JsonValue::String("42".to_string())));
        assert!(!json_value_equals(&JsonValue::Array(vec![]), &JsonValue::Object(serde_json::Map::new())));
    }

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

    #[test]
    fn test_json5_support() -> Result<()> {
        // JSON5 支持注释
        let json5_str = r#"
        {
            // 这是一个注释
            "type": "json5_test",
            "options": {
                /* 多行注释 */
                name: "json5_service",  // 未引用的键
                port: 8080,             // 尾随逗号
                enabled: true,
            }
        }"#;

        let type_options = TypeOptions::from_json(json5_str)?;
        assert_eq!(type_options.type_name, "json5_test");

        let options = &type_options.options;
        assert_eq!(options["name"], "json5_service");
        assert_eq!(options["port"], 8080);
        assert_eq!(options["enabled"], true);

        Ok(())
    }

    #[test]
    fn test_json_serialization() -> Result<()> {
        let json_str = r#"
        {
            "type": "json_test",
            "options": {
                "string_field": "test_value",
                "number_field": 42,
                "boolean_field": true,
                "array_field": [1, 2, 3]
            }
        }"#;

        let type_options = TypeOptions::from_json(json_str)?;
        assert_eq!(type_options.type_name, "json_test");

        let options = &type_options.options;
        assert_eq!(options["string_field"], "test_value");
        assert_eq!(options["number_field"], 42);
        assert_eq!(options["boolean_field"], true);
        assert!(options["array_field"].is_array());

        // 测试往返转换
        let json_output = type_options.to_json()?;
        assert!(json_output.contains("json_test"));
        assert!(json_output.contains("test_value"));

        // 验证重新解析
        let reparsed = TypeOptions::from_json(&json_output)?;
        assert_eq!(reparsed.type_name, type_options.type_name);
        assert_eq!(reparsed.options, type_options.options);

        Ok(())
    }

    #[test]
    fn test_yaml_serialization() -> Result<()> {
        let yaml_str = r#"
type: yaml_test
options:
  name: yaml_service
  port: 8080
  features:
    - logging
    - metrics
"#;

        let type_options = TypeOptions::from_yaml(yaml_str)?;
        assert_eq!(type_options.type_name, "yaml_test");

        let options = &type_options.options;
        assert_eq!(options["name"], "yaml_service");
        assert_eq!(options["port"], 8080);
        assert!(options["features"].is_array());

        // 测试往返转换
        let yaml_output = type_options.to_yaml()?;
        assert!(yaml_output.contains("yaml_test"));
        assert!(yaml_output.contains("yaml_service"));

        Ok(())
    }

    #[test]
    fn test_toml_serialization() -> Result<()> {
        let toml_str = r#"
type = "toml_test"

[options]
service_name = "toml_service"
port = 9090
enabled = false
"#;

        let type_options = TypeOptions::from_toml(toml_str)?;
        assert_eq!(type_options.type_name, "toml_test");

        let options = &type_options.options;
        assert_eq!(options["service_name"], "toml_service");
        assert_eq!(options["port"], 9090);
        assert_eq!(options["enabled"], false);

        // 测试往返转换
        let toml_output = type_options.to_toml()?;
        assert!(toml_output.contains("toml_test"));
        assert!(toml_output.contains("toml_service"));

        Ok(())
    }

    #[test]
    fn test_invalid_json_error() {
        let invalid_json = r#"
        {
            "type": "invalid_test",
            "options": {
                "unclosed": "quote
            }
        }"#;

        let result = TypeOptions::from_json(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_yaml_error() {
        let invalid_yaml = r#"
type: invalid_test
options:
  invalid: [
    - missing_close_bracket
"#;

        let result = TypeOptions::from_yaml(invalid_yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_options() -> Result<()> {
        let type_options = TypeOptions {
            type_name: "empty_test".to_string(),
            options: JsonValue::Object(serde_json::Map::new()),
        };

        // JSON
        let json_output = type_options.to_json()?;
        let json_reparsed = TypeOptions::from_json(&json_output)?;
        assert_eq!(json_reparsed.type_name, "empty_test");
        assert!(json_reparsed.options.is_object());
        assert!(json_reparsed.options.as_object().unwrap().is_empty());

        Ok(())
    }

    #[test]
    fn test_format_cross_compatibility() -> Result<()> {
        // 创建一个配置
        let original = TypeOptions {
            type_name: "cross_format_test".to_string(),
            options: serde_json::json!({
                "service": {
                    "name": "test_service",
                    "port": 8080,
                    "enabled": true
                }
            }),
        };

        // JSON -> YAML -> TOML -> JSON
        let json_str = original.to_json()?;
        let from_json = TypeOptions::from_json(&json_str)?;

        let yaml_str = from_json.to_yaml()?;
        let from_yaml = TypeOptions::from_yaml(&yaml_str)?;

        let toml_str = from_yaml.to_toml()?;
        let from_toml = TypeOptions::from_toml(&toml_str)?;

        let final_json = from_toml.to_json()?;
        let final_result = TypeOptions::from_json(&final_json)?;

        // 验证往返转换后数据一致
        assert_eq!(final_result.type_name, original.type_name);
        assert_eq!(final_result.options["service"]["name"], original.options["service"]["name"]);
        assert_eq!(final_result.options["service"]["port"], original.options["service"]["port"]);

        Ok(())
    }
}
