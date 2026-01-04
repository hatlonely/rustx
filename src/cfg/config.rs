// 核心配置类型和 trait 定义

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value as JsonValue;
use anyhow::Result;
use std::any::Any;

/// 核心trait - 定义配置类型的创建行为
pub trait Configurable: Send + Sync + 'static {
    type Config: DeserializeOwned + Clone;
    
    fn from_config(config: Self::Config) -> Result<Box<dyn Any + Send + Sync>>;
    fn type_name() -> &'static str;
}

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
    use serde_json::Value as JsonValue;

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
    struct MockConfig {
        id: u32,
        name: String,
        active: bool,
    }

    #[derive(Debug)]
    struct MockService {
        config: MockConfig,
    }

    impl Configurable for MockService {
        type Config = MockConfig;
        
        fn from_config(config: Self::Config) -> Result<Box<dyn Any + Send + Sync>> {
            Ok(Box::new(MockService { config }))
        }
        
        fn type_name() -> &'static str {
            "mock_service"
        }
    }

    #[test]
    fn test_configurable_trait() {
        let config = MockConfig {
            id: 1,
            name: "test".to_string(),
            active: true,
        };

        let result = MockService::from_config(config.clone());
        assert!(result.is_ok());

        let service_box = result.unwrap();
        let service = service_box.downcast_ref::<MockService>().unwrap();
        assert_eq!(service.config, config);
        assert_eq!(MockService::type_name(), "mock_service");
    }

    #[test]
    fn test_type_options_creation() {
        let config = MockConfig {
            id: 42,
            name: "test_config".to_string(),
            active: false,
        };

        let options = serde_json::to_value(config).unwrap();
        let type_options = TypeOptions {
            type_name: "test_type".to_string(),
            options,
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
    fn test_configurable_with_optional_fields() {
        #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
        struct OptionalConfig {
            required_field: String,
            #[serde(default)]
            optional_field: Option<String>,
            #[serde(default = "default_value")]
            default_field: u32,
        }

        fn default_value() -> u32 {
            42
        }

        struct OptionalService {
            config: OptionalConfig,
        }

        impl Configurable for OptionalService {
            type Config = OptionalConfig;
            
            fn from_config(config: Self::Config) -> Result<Box<dyn Any + Send + Sync>> {
                Ok(Box::new(OptionalService { config }))
            }
            
            fn type_name() -> &'static str {
                "optional_service"
            }
        }

        // 测试完整字段
        let full_config = OptionalConfig {
            required_field: "full_test".to_string(),
            optional_field: Some("optional_value".to_string()),
            default_field: 100,
        };

        let result = OptionalService::from_config(full_config.clone());
        assert!(result.is_ok());
        
        let service_box = result.unwrap();
        let service = service_box.downcast_ref::<OptionalService>().unwrap();
        assert_eq!(service.config, full_config);
    }
}