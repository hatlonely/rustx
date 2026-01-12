// 类型注册表和工厂函数

use std::collections::HashMap;
use std::any::Any;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use once_cell::sync::Lazy;
use std::sync::RwLock;
use anyhow::{Result, anyhow};

use super::type_options::TypeOptions;

// 构造函数类型
type Constructor = Box<dyn Fn(JsonValue) -> Result<Box<dyn Any + Send + Sync>> + Send + Sync>;

// 全局注册表
static REGISTRY: Lazy<RwLock<HashMap<String, Constructor>>> = Lazy::new(|| {
    RwLock::new(HashMap::new())
});

/// 智能注册方法 - 自动为任何实现 From<Config> 的类型创建适配器
///
/// 这个方法会：
/// 1. 使用标准库的 From trait 进行类型转换
/// 2. 生成合适的类型名称
/// 3. 创建透明的配置适配器
/// 4. 符合 Rust 惯用法
pub fn register_with_name<T, Config>(type_name: &str) -> Result<()>
where
    T: Send + Sync + 'static,
    Config: DeserializeOwned + Clone + Send + Sync + 'static,
    T: From<Config>,
{
    let type_name = type_name.to_string();
    let constructor: Constructor = Box::new(|value| {
        let config: Config = serde_json::from_value(value)?;
        let instance = T::from(config);
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
    T: From<Config>,
{
    // 同时使用完整类型名和简短类型名进行注册，解决名称冲突问题
    let full_type_name = generate_auto_type_name::<T>();
    let short_type_name = generate_short_type_name::<T>();

    // 使用完整名称注册
    register_with_name::<T, Config>(&full_type_name)?;

    // 使用简短名称注册
    register_with_name::<T, Config>(&short_type_name)?;

    Ok(())
}

/// 自动生成类型名称 - 直接使用 type_name 作为 key
pub fn generate_auto_type_name<T: 'static>() -> String {
    use std::any::type_name;
    type_name::<T>().to_string()
}

/// 生成简短的类型名称 - 简化完整路径为可读格式
pub fn generate_short_type_name<T: 'static>() -> String {
    use std::any::type_name;
    let full_name = type_name::<T>();

    // 简化类型名称，移除完整路径，只保留类型名和简化的泛型参数
    simplify_type_name(full_name)
}

/// 简化完整类型名称为更可读的格式
fn simplify_type_name(full_name: &str) -> String {
    // 处理泛型类型，例如将 rustx::kv::store::map_store::MapStore<alloc::string::String, alloc::string::String>
    // 简化为 MapStore<String, String>

    // 首先找到泛型参数的开始位置（第一个'<'）
    if let Some(generic_start) = full_name.find('<') {
        // 提取主类型名部分（不含泛型参数）
        let main_part = &full_name[..generic_start];

        // 获取泛型参数部分
        let generics_part = &full_name[generic_start..];

        // 提取主类型名的最后部分（即实际类型名）
        let main_type_name = main_part.split("::").last().unwrap_or(main_part);

        // 解析并简化泛型参数
        let simplified_generics = simplify_generics(generics_part);

        format!("{}{}", main_type_name, simplified_generics)
    } else {
        // 如果不是泛型类型，只取路径的最后部分
        full_name.split("::").last().unwrap_or(full_name).to_string()
    }
}

/// 简化泛型参数部分
fn simplify_generics(generics: &str) -> String {
    // 解析泛型参数，简化每个参数的路径
    let mut bracket_count = 0;
    let mut current_param = String::new();
    let mut params = Vec::new();

    for ch in generics.chars() {
        if ch == '<' {
            if bracket_count > 0 {
                current_param.push(ch);
            }
            bracket_count += 1;
        } else if ch == '>' {
            bracket_count -= 1;
            if bracket_count > 0 {
                current_param.push(ch);
            } else {
                // 完成一个参数
                if !current_param.is_empty() {
                    params.push(simplify_type_param(&current_param));
                    current_param.clear();
                }
            }
        } else if ch == ',' && bracket_count == 1 {
            // 分隔符，但不在嵌套泛型内
            if !current_param.is_empty() {
                params.push(simplify_type_param(&current_param));
                current_param.clear();
            }
        } else {
            current_param.push(ch);
        }
    }

    // 处理最后一个参数（如果有的话）
    if !current_param.trim().is_empty() {
        params.push(simplify_type_param(&current_param));
    }

    // 重新组装泛型参数
    format!("<{}>", params.join(", "))
}

/// 简化单个类型参数
fn simplify_type_param(param: &str) -> String {
    let param = param.trim();

    // 处理嵌套泛型类型，如 Vec<String>
    if param.contains('<') && param.contains('>') {
        // 找到主类型名和泛型参数
        if let Some(generic_start) = param.find('<') {
            let main_part = &param[..generic_start];
            let generics_part = &param[generic_start..];

            // 简化主类型名
            let main_type_name = main_part.split("::").last().unwrap_or(main_part);

            // 简化嵌套的泛型参数
            let simplified_generics = simplify_generics(generics_part);

            format!("{}{}", main_type_name, simplified_generics)
        } else {
            // 如果无法解析，尝试简化路径
            param.split("::").last().unwrap_or(param).to_string()
        }
    } else {
        // 简单类型，直接取路径的最后一部分
        param.split("::").last().unwrap_or(param).to_string()
    }
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
    use crate::cfg::type_options::TypeOptions;
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

    impl From<TestConfig> for TestService {
        fn from(config: TestConfig) -> Self {
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

        impl From<CustomConfig> for CustomService {
            fn from(config: CustomConfig) -> Self {
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

    #[test]
    fn test_duplicate_registration() -> Result<()> {
        // 第一次注册
        register::<TestService, TestConfig>()?;

        let config1 = TestConfig {
            message: "first_register".to_string(),
            count: 1,
        };

        let actual_type_name = generate_auto_type_name::<TestService>();
        let type_options1 = TypeOptions {
            type_name: actual_type_name.clone(),
            options: serde_json::to_value(config1.clone())?,
        };

        let obj1 = create_from_type_options(&type_options1)?;
        let service1 = obj1.downcast_ref::<TestService>().unwrap();
        assert_eq!(service1.config, config1);

        // 重复注册同一个类型，应该覆盖之前的注册
        register::<TestService, TestConfig>()?;

        let config2 = TestConfig {
            message: "second_register".to_string(),
            count: 2,
        };

        let type_options2 = TypeOptions {
            type_name: actual_type_name,
            options: serde_json::to_value(config2.clone())?,
        };

        let obj2 = create_from_type_options(&type_options2)?;
        let service2 = obj2.downcast_ref::<TestService>().unwrap();
        assert_eq!(service2.config, config2);

        Ok(())
    }

    #[test]
    fn test_duplicate_registration_with_different_types() -> Result<()> {
        // 注册第一个类型
        register_with_name::<TestService, TestConfig>("same_type_name")?;

        // 用相同名称注册另一个类型，应该覆盖
        #[derive(Debug, PartialEq, Clone, Deserialize)]
        struct AnotherConfig {
            value: String,
        }

        #[derive(Debug, PartialEq)]
        struct AnotherService {
            value: String,
        }

        impl From<AnotherConfig> for AnotherService {
            fn from(config: AnotherConfig) -> Self {
                Self { value: config.value }
            }
        }

        register_with_name::<AnotherService, AnotherConfig>("same_type_name")?;

        // 验证现在创建的是新类型而不是旧类型
        let type_options = TypeOptions {
            type_name: "same_type_name".to_string(),
            options: serde_json::json!({
                "value": "test_value"
            }),
        };

        let obj = create_from_type_options(&type_options)?;
        // 尝试转换为新类型
        let service = obj.downcast_ref::<AnotherService>();
        assert!(service.is_some());
        assert_eq!(service.unwrap().value, "test_value");

        // 尝试转换为旧类型应该失败
        let old_service = obj.downcast_ref::<TestService>();
        assert!(old_service.is_none());

        Ok(())
    }

    #[test]
    fn test_generate_short_type_name() {
        // 测试非泛型类型
        let short_name = generate_short_type_name::<TestService>();
        assert_eq!(short_name, "TestService");

        // 测试简化函数对 MapStore 类型的处理
        let full_name = "rustx::kv::store::map_store::MapStore<alloc::string::String, alloc::string::String>";
        let simplified = simplify_type_name(full_name);
        assert_eq!(simplified, "MapStore<String, String>");

        // 测试嵌套泛型
        let nested_full_name = "std::collections::HashMap<alloc::string::String, alloc::vec::Vec<alloc::string::String>>";
        let nested_simplified = simplify_type_name(nested_full_name);
        assert_eq!(nested_simplified, "HashMap<String, Vec<String>>");

        // 测试与原函数的区别
        let long_name = generate_auto_type_name::<TestService>();
        let short_name = generate_short_type_name::<TestService>();
        assert!(long_name.contains("TestService"));
        assert_eq!(short_name, "TestService");
        assert!(long_name.len() > short_name.len());
    }

    #[test]
    fn test_dual_registration() -> Result<()> {
        // 测试双注册机制：同时使用完整名称和简短名称进行注册
        register::<TestService, TestConfig>()?;

        let config = TestConfig {
            message: "dual_test".to_string(),
            count: 42,
        };

        // 使用完整类型名访问
        let full_type_name = generate_auto_type_name::<TestService>();
        let type_options_full = TypeOptions {
            type_name: full_type_name,
            options: serde_json::to_value(config.clone())?,
        };

        let obj_full = create_from_type_options(&type_options_full)?;
        let service_full = obj_full.downcast_ref::<TestService>().unwrap();
        assert_eq!(service_full.config, config);

        // 使用简短类型名访问
        let short_type_name = generate_short_type_name::<TestService>();
        let type_options_short = TypeOptions {
            type_name: short_type_name,
            options: serde_json::to_value(config.clone())?,
        };

        let obj_short = create_from_type_options(&type_options_short)?;
        let service_short = obj_short.downcast_ref::<TestService>().unwrap();
        assert_eq!(service_short.config, config);

        Ok(())
    }
}
