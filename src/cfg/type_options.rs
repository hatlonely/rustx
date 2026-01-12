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
    /// 从 JSON 字符串创建 TypeOptions
    pub fn from_json(json_str: &str) -> Result<Self> {
        Ok(serde_json::from_str(json_str)?)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value as JsonValue;

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
