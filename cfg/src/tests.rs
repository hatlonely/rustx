#[cfg(test)]
mod tests {
    use crate::{Configurable, TypeOptions, register, register_type, create_from_type_options};
    use serde::{Deserialize, Serialize};
    use std::any::Any;
    use anyhow::Result;

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
    struct TestConfig {
        name: String,
        value: i32,
        enabled: bool,
    }

    #[derive(Debug, PartialEq)]
    struct TestService {
        config: TestConfig,
    }

    impl TestService {
        fn new(config: TestConfig) -> Self {
            Self { config }
        }
    }

    impl Configurable for TestService {
        type Config = TestConfig;
        
        fn from_config(config: Self::Config) -> Result<Box<dyn Any + Send + Sync>> {
            Ok(Box::new(TestService::new(config)))
        }
        
        fn type_name() -> &'static str {
            "test_service"
        }
    }

    #[test]
    fn test_register_and_create() -> Result<()> {
        register::<TestService>()?;
        
        let config = TestConfig {
            name: "test".to_string(),
            value: 42,
            enabled: true,
        };
        
        let type_options = TypeOptions {
            type_name: "test_service".to_string(),
            options: serde_json::to_value(config.clone())?,
        };
        
        let obj = create_from_type_options(&type_options)?;
        let service = obj.downcast_ref::<TestService>().unwrap();
        
        assert_eq!(service.config, config);
        Ok(())
    }

    #[test]
    fn test_json_serialization() -> Result<()> {
        let json_str = r#"
        {
            "type": "test_service",
            "options": {
                "name": "json_test",
                "value": 100,
                "enabled": false
            }
        }"#;
        
        let type_options = TypeOptions::from_json(json_str)?;
        assert_eq!(type_options.type_name, "test_service");
        
        let json_output = type_options.to_json()?;
        assert!(json_output.contains("test_service"));
        assert!(json_output.contains("json_test"));
        
        Ok(())
    }

    #[test]
    fn test_yaml_serialization() -> Result<()> {
        let yaml_str = r#"
type: test_service
options:
  name: yaml_test
  value: 200
  enabled: true
"#;
        
        let type_options = TypeOptions::from_yaml(yaml_str)?;
        assert_eq!(type_options.type_name, "test_service");
        
        let yaml_output = type_options.to_yaml()?;
        assert!(yaml_output.contains("test_service"));
        assert!(yaml_output.contains("yaml_test"));
        
        Ok(())
    }

    #[test]
    fn test_toml_serialization() -> Result<()> {
        let toml_str = r#"
type = "test_service"

[options]
name = "toml_test"
value = 300
enabled = false
"#;
        
        let type_options = TypeOptions::from_toml(toml_str)?;
        assert_eq!(type_options.type_name, "test_service");
        
        let toml_output = type_options.to_toml()?;
        assert!(toml_output.contains("test_service"));
        assert!(toml_output.contains("toml_test"));
        
        Ok(())
    }

    #[test]
    fn test_manual_registration() -> Result<()> {
        #[derive(Deserialize)]
        struct SimpleConfig {
            message: String,
        }

        #[derive(Debug, PartialEq)]
        struct SimpleService {
            message: String,
        }

        register_type("simple", |config: SimpleConfig| -> Result<Box<dyn Any + Send + Sync>> {
            Ok(Box::new(SimpleService {
                message: config.message,
            }))
        })?;
        
        let type_options = TypeOptions {
            type_name: "simple".to_string(),
            options: serde_json::json!({
                "message": "Hello, World!"
            }),
        };
        
        let obj = create_from_type_options(&type_options)?;
        let service = obj.downcast_ref::<SimpleService>().unwrap();
        
        assert_eq!(service.message, "Hello, World!");
        Ok(())
    }

    #[test]
    fn test_unregistered_type_error() {
        let type_options = TypeOptions {
            type_name: "unknown_type".to_string(),
            options: serde_json::json!({}),
        };
        
        let result = create_from_type_options(&type_options);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not registered"));
    }
}