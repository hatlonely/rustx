// 类型注册表和工厂函数

use std::collections::HashMap;
use std::any::Any;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use once_cell::sync::Lazy;
use std::sync::RwLock;
use anyhow::{Result, anyhow};

use super::config::{TypeOptions, WithConfig};

// 构造函数类型
type Constructor = Box<dyn Fn(JsonValue) -> Result<Box<dyn Any + Send + Sync>> + Send + Sync>;

// 全局注册表
static REGISTRY: Lazy<RwLock<HashMap<String, Constructor>>> = Lazy::new(|| {
    RwLock::new(HashMap::new())
});

/// 智能注册方法 - 自动为任何有 with_config 的类型创建适配器
///
/// 这个方法会：
/// 1. 自动检测类型的 with_config 方法
/// 2. 生成合适的类型名称
/// 3. 创建透明的配置适配器
/// 4. 完全无需类型实现任何 trait
pub fn register_with_name<T, Config>(type_name: &str) -> Result<()>
where
    T: Send + Sync + 'static,
    Config: DeserializeOwned + Clone + Send + Sync + 'static,
    T: WithConfig<Config>,
{
    let type_name = type_name.to_string();
    let constructor: Constructor = Box::new(|value| {
        let config: Config = serde_json::from_value(value)?;
        let instance = T::with_config(config);
        Ok(Box::new(instance))
    });

    let mut registry = REGISTRY.write().map_err(|_| anyhow!("Failed to acquire write lock"))?;
    registry.insert(type_name, constructor);
    Ok(())
}

/// 带自动类型名称生成的智能注册
pub fn register<T, Config>() -> Result<()>
where
    T: Send + Sync + 'static,
    Config: DeserializeOwned + Clone + Send + Sync + 'static,
    T: WithConfig<Config>,
{
    let type_name = generate_auto_type_name::<T>();
    register_with_name::<T, Config>(&type_name)
}


/// 自动生成类型名称 - 直接使用 type_name 作为 key
pub fn generate_auto_type_name<T: 'static>() -> String {
    use std::any::type_name;
    type_name::<T>().to_string()
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
    use crate::cfg::config::TypeOptions;
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

    impl WithConfig<TestConfig> for TestService {
        fn with_config(config: TestConfig) -> Self {
            Self { config }
        }
    }

    #[test]
    fn test_register_auto_with_type() -> Result<()> {
        register::<TestService, TestConfig>()?;

        let config = TestConfig {
            message: "test_message".to_string(),
            count: 10,
        };

        let actual_type_name = generate_auto_type_name::<TestService>();
        let type_options = TypeOptions {
            type_name: actual_type_name,
            options: serde_json::to_value(config.clone())?,
        };

        let obj = create_from_type_options(&type_options)?;
        let service = obj.downcast_ref::<TestService>().unwrap();

        assert_eq!(service.config, config);
        Ok(())
    }

    #[test]
    fn test_register_auto_manual_type_name() -> Result<()> {
        #[derive(Debug, PartialEq, Clone, Deserialize)]
        struct CustomConfig {
            value: String,
        }

        #[derive(Debug, PartialEq)]
        struct CustomService {
            value: String,
        }

        impl WithConfig<CustomConfig> for CustomService {
            fn with_config(config: CustomConfig) -> Self {
                Self { value: config.value }
            }
        }

        register_with_name::<CustomService, CustomConfig>("custom_service")?;

        let type_options = TypeOptions {
            type_name: "custom_service".to_string(),
            options: serde_json::json!({
                "value": "custom_test"
            }),
        };

        let obj = create_from_type_options(&type_options)?;
        let service = obj.downcast_ref::<CustomService>().unwrap();

        assert_eq!(service.value, "custom_test");
        Ok(())
    }

    #[test]
    fn test_generate_auto_type_name() {
        // 测试类型名生成
        let generated_type_name = generate_auto_type_name::<TestService>();
        assert!(generated_type_name.contains("TestService"));

        // 验证是完整的类型名
        let actual_type_name = std::any::type_name::<TestService>();
        assert_eq!(generated_type_name, actual_type_name);
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
        register::<TestService, TestConfig>()?;

        // 提供错误的配置格式
        let actual_type_name = generate_auto_type_name::<TestService>();
        let type_options = TypeOptions {
            type_name: actual_type_name,
            options: serde_json::json!({
                "wrong_field": "invalid"
            }),
        };

        let result = create_from_type_options(&type_options);
        assert!(result.is_err());
        Ok(())
    }
}
