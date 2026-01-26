// 类型注册表和工厂函数

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::RwLock;

use super::type_options::TypeOptions;

use std::sync::Arc;

// Trait-based 构造函数类型
// 注意：这里我们仍然返回 Box<dyn Any>，但实际上它包含的是 Box<dyn Trait>
// 使用 Arc 以便在查找后快速克隆（只是增加引用计数）
type TraitConstructor = Arc<dyn Fn(JsonValue) -> Result<Box<dyn Any + Send + Sync>> + Send + Sync>;

// Trait 注册表：为每个 Trait 类型维护一个独立的注册表
// 外层 HashMap 的 key 是 Trait 的 TypeId，内层 HashMap 的 key 是类型名称
static TRAIT_REGISTRY: Lazy<RwLock<HashMap<TypeId, HashMap<String, TraitConstructor>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

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

    let constructor: TraitConstructor = Arc::new(move |value| {
        let config: Config = serde_json::from_value(value)?;
        let instance = T::from(config);
        let boxed_instance = Box::new(instance);
        let trait_object: Box<Trait> = boxed_instance.into();
        // 将 Box<dyn Trait> 包装成 Box<dyn Any>
        Ok(Box::new(trait_object) as Box<dyn Any + Send + Sync>)
    });

    let mut registry = TRAIT_REGISTRY
        .write()
        .map_err(|_| anyhow!("Failed to acquire write lock"))?;
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

    // 步骤1: 在读锁中查找并克隆构造函数（锁在此步骤后立即释放）
    let constructor = {
        let registry_guard = TRAIT_REGISTRY.read()
            .map_err(|_| anyhow!("Failed to acquire read lock"))?;

        let trait_registry = registry_guard
            .get(&trait_id)
            .ok_or_else(|| anyhow!("No implementations registered for trait"))?;

        trait_registry
            .get(&type_options.type_name)
            .ok_or_else(|| anyhow!(
                "Type '{}' not registered for this trait",
                type_options.type_name
            ))?
            .clone()
        // registry_guard 在此自动释放
    };

    // 步骤2: 在无锁状态下调用构造函数（可能包含耗时操作）
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
    use serde::Deserialize;

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
}
