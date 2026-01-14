// 类型注册表和工厂函数

use std::collections::HashMap;
use std::any::{Any, TypeId};
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

// Trait-based 构造函数类型
// 注意：这里我们仍然返回 Box<dyn Any>，但实际上它包含的是 Box<dyn Trait>
type TraitConstructor = Box<dyn Fn(JsonValue) -> Result<Box<dyn Any + Send + Sync>> + Send + Sync>;

// Trait 注册表：为每个 Trait 类型维护一个独立的注册表
// 外层 HashMap 的 key 是 Trait 的 TypeId，内层 HashMap 的 key 是类型名称
static TRAIT_REGISTRY: Lazy<RwLock<HashMap<TypeId, HashMap<String, TraitConstructor>>>> = Lazy::new(|| {
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

/// 为实现特定 Trait 的类型注册构造函数
///
/// 这个方法允许你注册多个实现同一 Trait 的不同类型，并在运行时根据配置创建 trait object
///
/// # 类型参数
/// - `T`: 具体实现类型
/// - `Trait`: 目标 trait（使用 `dyn Trait` 形式）
/// - `Config`: 配置类型
///
/// # 示例
/// ```ignore
/// trait Service {
///     fn serve(&self);
/// }
///
/// register_trait::<ServiceV1, dyn Service, ServiceV1Config>("service-v1")?;
/// register_trait::<ServiceV2, dyn Service, ServiceV2Config>("service-v2")?;
///
/// let service: Box<dyn Service> = create_trait_from_type_options(&type_options)?;
/// service.serve();
/// ```
pub fn register_trait<T, Trait, Config>(type_name: &str) -> Result<()>
where
    T: Send + Sync + 'static,
    Trait: ?Sized + Send + Sync + 'static,
    Config: DeserializeOwned + Clone + Send + Sync + 'static,
    T: From<Config>,
    Box<T>: Into<Box<Trait>>,
{
    let type_name = type_name.to_string();
    let trait_id = TypeId::of::<Trait>();

    let constructor: TraitConstructor = Box::new(move |value| {
        let config: Config = serde_json::from_value(value)?;
        let instance = T::from(config);
        let boxed_instance = Box::new(instance);
        let trait_object: Box<Trait> = boxed_instance.into();
        // 将 Box<dyn Trait> 包装成 Box<dyn Any>
        Ok(Box::new(trait_object) as Box<dyn Any + Send + Sync>)
    });

    let mut registry = TRAIT_REGISTRY.write().map_err(|_| anyhow!("Failed to acquire write lock"))?;
    registry
        .entry(trait_id)
        .or_insert_with(HashMap::new)
        .insert(type_name, constructor);

    Ok(())
}

/// 根据 TypeOptions 创建 trait object
///
/// # 类型参数
/// - `Trait`: 目标 trait（使用 `dyn Trait` 形式）
///
/// # 示例
/// ```ignore
/// let type_options = TypeOptions::from_json(r#"{"type": "service-v1", "options": {...}}"#)?;
/// let service: Box<dyn Service> = create_trait_from_type_options(&type_options)?;
/// ```
pub fn create_trait_from_type_options<Trait>(type_options: &TypeOptions) -> Result<Box<Trait>>
where
    Trait: ?Sized + Send + Sync + 'static,
{
    let trait_id = TypeId::of::<Trait>();
    let registry = TRAIT_REGISTRY.read().map_err(|_| anyhow!("Failed to acquire read lock"))?;

    let trait_registry = registry
        .get(&trait_id)
        .ok_or_else(|| anyhow!("No implementations registered for trait"))?;

    let constructor = trait_registry
        .get(&type_options.type_name)
        .ok_or_else(|| anyhow!("Type '{}' not registered for this trait", type_options.type_name))?;

    let any_box = constructor(type_options.options.clone())?;

    // 从 Box<dyn Any> 中提取 Box<dyn Trait>
    // 这里使用 downcast 将 Box<dyn Any> 转换回 Box<Box<dyn Trait>>
    any_box
        .downcast::<Box<Trait>>()
        .map(|boxed| *boxed)
        .map_err(|_| anyhow!("Failed to downcast to target trait type"))
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

    // ========== Trait-based 注册测试 ==========

    // 定义测试用的 trait
    trait TestTrait: Send + Sync {
        fn execute(&self) -> String;
        fn get_type_name(&self) -> &str;
    }

    // 第一个实现
    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct ImplAConfig {
        message: String,
    }

    #[derive(Debug)]
    struct ImplA {
        message: String,
    }

    impl From<ImplAConfig> for ImplA {
        fn from(config: ImplAConfig) -> Self {
            Self {
                message: config.message,
            }
        }
    }

    impl TestTrait for ImplA {
        fn execute(&self) -> String {
            format!("ImplA: {}", self.message)
        }

        fn get_type_name(&self) -> &str {
            "ImplA"
        }
    }

    impl From<Box<ImplA>> for Box<dyn TestTrait> {
        fn from(impl_a: Box<ImplA>) -> Self {
            impl_a as Box<dyn TestTrait>
        }
    }

    // 第二个实现
    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct ImplBConfig {
        value: i32,
    }

    #[derive(Debug)]
    struct ImplB {
        value: i32,
    }

    impl From<ImplBConfig> for ImplB {
        fn from(config: ImplBConfig) -> Self {
            Self {
                value: config.value,
            }
        }
    }

    impl TestTrait for ImplB {
        fn execute(&self) -> String {
            format!("ImplB: {}", self.value)
        }

        fn get_type_name(&self) -> &str {
            "ImplB"
        }
    }

    impl From<Box<ImplB>> for Box<dyn TestTrait> {
        fn from(impl_b: Box<ImplB>) -> Self {
            impl_b as Box<dyn TestTrait>
        }
    }

    #[test]
    fn test_register_trait_basic() -> Result<()> {
        // 注册两个不同的实现
        register_trait::<ImplA, dyn TestTrait, ImplAConfig>("impl-a")?;
        register_trait::<ImplB, dyn TestTrait, ImplBConfig>("impl-b")?;

        // 创建 ImplA 实例
        let type_options_a = TypeOptions {
            type_name: "impl-a".to_string(),
            options: serde_json::json!({
                "message": "hello"
            }),
        };

        let trait_obj_a: Box<dyn TestTrait> = create_trait_from_type_options(&type_options_a)?;
        assert_eq!(trait_obj_a.execute(), "ImplA: hello");
        assert_eq!(trait_obj_a.get_type_name(), "ImplA");

        // 创建 ImplB 实例
        let type_options_b = TypeOptions {
            type_name: "impl-b".to_string(),
            options: serde_json::json!({
                "value": 42
            }),
        };

        let trait_obj_b: Box<dyn TestTrait> = create_trait_from_type_options(&type_options_b)?;
        assert_eq!(trait_obj_b.execute(), "ImplB: 42");
        assert_eq!(trait_obj_b.get_type_name(), "ImplB");

        Ok(())
    }

    #[test]
    fn test_register_trait_unregistered_type() -> Result<()> {
        register_trait::<ImplA, dyn TestTrait, ImplAConfig>("test-impl-a")?;

        let type_options = TypeOptions {
            type_name: "unknown-impl".to_string(),
            options: serde_json::json!({}),
        };

        let result: Result<Box<dyn TestTrait>> = create_trait_from_type_options(&type_options);
        assert!(result.is_err());

        if let Err(e) = result {
            let error_msg = e.to_string();
            assert!(error_msg.contains("not registered"));
        }

        Ok(())
    }

    #[test]
    fn test_register_trait_invalid_config() -> Result<()> {
        register_trait::<ImplA, dyn TestTrait, ImplAConfig>("test-impl-a2")?;

        // 提供错误的配置格式
        let type_options = TypeOptions {
            type_name: "test-impl-a2".to_string(),
            options: serde_json::json!({
                "wrong_field": "invalid"
            }),
        };

        let result: Result<Box<dyn TestTrait>> = create_trait_from_type_options(&type_options);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_register_trait_multiple_instances() -> Result<()> {
        register_trait::<ImplA, dyn TestTrait, ImplAConfig>("multi-impl-a")?;
        register_trait::<ImplB, dyn TestTrait, ImplBConfig>("multi-impl-b")?;

        // 创建多个实例并存储在 Vec 中
        let mut instances: Vec<Box<dyn TestTrait>> = Vec::new();

        for i in 0..3 {
            let type_options_a = TypeOptions {
                type_name: "multi-impl-a".to_string(),
                options: serde_json::json!({
                    "message": format!("message-{}", i)
                }),
            };

            let trait_obj: Box<dyn TestTrait> = create_trait_from_type_options(&type_options_a)?;
            instances.push(trait_obj);
        }

        for i in 0..2 {
            let type_options_b = TypeOptions {
                type_name: "multi-impl-b".to_string(),
                options: serde_json::json!({
                    "value": i * 10
                }),
            };

            let trait_obj: Box<dyn TestTrait> = create_trait_from_type_options(&type_options_b)?;
            instances.push(trait_obj);
        }

        assert_eq!(instances.len(), 5);

        // 验证每个实例都能正常工作
        assert_eq!(instances[0].execute(), "ImplA: message-0");
        assert_eq!(instances[1].execute(), "ImplA: message-1");
        assert_eq!(instances[2].execute(), "ImplA: message-2");
        assert_eq!(instances[3].execute(), "ImplB: 0");
        assert_eq!(instances[4].execute(), "ImplB: 10");

        Ok(())
    }

    #[test]
    fn test_trait_and_concrete_type_registry_independent() -> Result<()> {
        // 测试 trait 注册表和具体类型注册表是独立的

        // 使用相同的名称注册到两个不同的注册表
        register_with_name::<ImplA, ImplAConfig>("same-name")?;
        register_trait::<ImplB, dyn TestTrait, ImplBConfig>("same-name")?;

        // 从具体类型注册表创建实例
        let concrete_type_options = TypeOptions {
            type_name: "same-name".to_string(),
            options: serde_json::json!({
                "message": "concrete"
            }),
        };

        let concrete_obj = create_from_type_options(&concrete_type_options)?;
        let impl_a = concrete_obj.downcast_ref::<ImplA>().unwrap();
        assert_eq!(impl_a.message, "concrete");

        // 从 trait 注册表创建实例
        let trait_type_options = TypeOptions {
            type_name: "same-name".to_string(),
            options: serde_json::json!({
                "value": 99
            }),
        };

        let trait_obj: Box<dyn TestTrait> = create_trait_from_type_options(&trait_type_options)?;
        assert_eq!(trait_obj.execute(), "ImplB: 99");

        Ok(())
    }
}
