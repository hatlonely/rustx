// 类型注册表和工厂函数

use std::collections::HashMap;
use std::any::Any;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use once_cell::sync::Lazy;
use std::sync::RwLock;
use anyhow::{Result, anyhow};

use super::config::{Configurable, TypeOptions};

// 构造函数类型
type Constructor = Box<dyn Fn(JsonValue) -> Result<Box<dyn Any + Send + Sync>> + Send + Sync>;

// 全局注册表
static REGISTRY: Lazy<RwLock<HashMap<String, Constructor>>> = Lazy::new(|| {
    RwLock::new(HashMap::new())
});

/// 注册实现了 Configurable trait 的类型
pub fn register<T: Configurable>() -> Result<()> {
    let type_name = T::type_name().to_string();
    let constructor: Constructor = Box::new(|value| {
        let config: T::Config = serde_json::from_value(value)?;
        T::from_config(config)
    });
    
    let mut registry = REGISTRY.write().map_err(|_| anyhow!("Failed to acquire write lock"))?;
    registry.insert(type_name, constructor);
    Ok(())
}

/// 手动注册函数（当无法使用泛型时）
pub fn register_type<C>(
    type_name: &str,
    constructor: impl Fn(C) -> Result<Box<dyn Any + Send + Sync>> + Send + Sync + 'static,
) -> Result<()>
where
    C: DeserializeOwned + 'static,
{
    let type_name = type_name.to_string();
    let wrapped_constructor: Constructor = Box::new(move |value| {
        let config: C = serde_json::from_value(value)?;
        constructor(config)
    });
    
    let mut registry = REGISTRY.write().map_err(|_| anyhow!("Failed to acquire write lock"))?;
    registry.insert(type_name, wrapped_constructor);
    Ok(())
}

/// 根据 TypeOptions 创建对象实例
pub fn create_from_type_options(type_options: &TypeOptions) -> Result<Box<dyn Any + Send + Sync>> {
    let registry = REGISTRY.read().map_err(|_| anyhow!("Failed to acquire read lock"))?;
    
    let constructor = registry
        .get(&type_options.type_name)
        .ok_or_else(|| anyhow!("Type '{}' not registered", type_options.type_name))?;
    
    constructor(type_options.options.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::config::{Configurable, TypeOptions};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
    struct TestConfig {
        message: String,
        count: usize,
    }

    #[derive(Debug, PartialEq)]
    struct TestService {
        config: TestConfig,
    }

    impl Configurable for TestService {
        type Config = TestConfig;
        
        fn from_config(config: Self::Config) -> Result<Box<dyn Any + Send + Sync>> {
            Ok(Box::new(TestService { config }))
        }
        
        fn type_name() -> &'static str {
            "test_service"
        }
    }

    #[test]
    fn test_register_and_create() -> Result<()> {
        register::<TestService>()?;
        
        let config = TestConfig {
            message: "test_message".to_string(),
            count: 10,
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
    fn test_register_type_manual() -> Result<()> {
        #[derive(Deserialize)]
        struct ManualConfig {
            value: String,
        }

        #[derive(Debug, PartialEq)]
        struct ManualService {
            value: String,
        }

        register_type("manual_service", |config: ManualConfig| -> Result<Box<dyn Any + Send + Sync>> {
            Ok(Box::new(ManualService {
                value: config.value,
            }))
        })?;
        
        let type_options = TypeOptions {
            type_name: "manual_service".to_string(),
            options: serde_json::json!({
                "value": "manual_test"
            }),
        };
        
        let obj = create_from_type_options(&type_options)?;
        let service = obj.downcast_ref::<ManualService>().unwrap();
        
        assert_eq!(service.value, "manual_test");
        Ok(())
    }

    #[test]
    fn test_unregistered_type_error() {
        let type_options = TypeOptions {
            type_name: "unknown_service".to_string(),
            options: serde_json::json!({}),
        };
        
        let result = create_from_type_options(&type_options);
        assert!(result.is_err());
        
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("not registered"));
        assert!(error_msg.contains("unknown_service"));
    }

    #[test]
    fn test_invalid_config_error() -> Result<()> {
        register::<TestService>()?;
        
        // 提供错误的配置格式
        let type_options = TypeOptions {
            type_name: "test_service".to_string(),
            options: serde_json::json!({
                "wrong_field": "invalid"
            }),
        };
        
        let result = create_from_type_options(&type_options);
        assert!(result.is_err());
        Ok(())
    }
}