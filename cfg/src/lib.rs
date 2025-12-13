// cfg library - Configuration management

use std::collections::HashMap;
use std::any::Any;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value as JsonValue;
use once_cell::sync::Lazy;
use std::sync::RwLock;
use anyhow::{Result, anyhow};

// 核心trait - 定义配置类型的创建行为
pub trait Configurable: Send + Sync + 'static {
    type Config: DeserializeOwned + Clone;
    
    fn from_config(config: Self::Config) -> Result<Box<dyn Any + Send + Sync>>;
    fn type_name() -> &'static str;
}

// 类型选项结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeOptions {
    #[serde(rename = "type")]
    pub type_name: String,
    pub options: JsonValue,
}

// 构造函数类型
type Constructor = Box<dyn Fn(JsonValue) -> Result<Box<dyn Any + Send + Sync>> + Send + Sync>;

// 全局注册表
static REGISTRY: Lazy<RwLock<HashMap<String, Constructor>>> = Lazy::new(|| {
    RwLock::new(HashMap::new())
});

// 注册函数
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

// 手动注册函数（当无法使用泛型时）
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

// 工厂函数
pub fn create_from_type_options(type_options: &TypeOptions) -> Result<Box<dyn Any + Send + Sync>> {
    let registry = REGISTRY.read().map_err(|_| anyhow!("Failed to acquire read lock"))?;
    
    let constructor = registry
        .get(&type_options.type_name)
        .ok_or_else(|| anyhow!("Type '{}' not registered", type_options.type_name))?;
    
    constructor(type_options.options.clone())
}

// 便利函数 - 从各种格式创建TypeOptions
impl TypeOptions {
    pub fn from_json(json_str: &str) -> Result<Self> {
        Ok(serde_json::from_str(json_str)?)
    }
    
    pub fn from_yaml(yaml_str: &str) -> Result<Self> {
        Ok(serde_yaml::from_str(yaml_str)?)
    }
    
    pub fn from_toml(toml_str: &str) -> Result<Self> {
        Ok(toml::from_str(toml_str)?)
    }
    
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
    
    pub fn to_yaml(&self) -> Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }
    
    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }
}

pub mod duration;
mod tests;