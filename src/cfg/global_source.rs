//! 全局配置源
//!
//! 提供全局单例 ConfigSource，方便在应用任何地方访问配置

use anyhow::Result;
use once_cell::sync::Lazy;
use std::sync::{Arc, RwLock};

use super::{create_trait_from_type_options, source::ConfigChange, ConfigSource, ConfigValue, TypeOptions};

/// 全局 ConfigSource 单例
static GLOBAL_SOURCE: Lazy<Arc<RwLock<Option<Box<dyn ConfigSource>>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

/// 初始化全局 ConfigSource
///
/// 通过 TypeOptions 创建并设置全局配置源。使用前需要先调用 register_sources() 注册所有配置源类型。
///
/// # 参数
/// - `options`: 配置源的类型和选项
///
/// # 返回
/// - 成功返回 Ok(())，失败返回错误信息
///
/// # 示例
/// ```ignore
/// use rustx::cfg::{register_sources, init, TypeOptions};
///
/// // 注册所有配置源类型
/// register_sources().unwrap();
///
/// // 初始化全局配置源
/// let options = TypeOptions::from_json(r#"{
///     "type": "FileSource",
///     "options": {
///         "base_path": "./config"
///     }
/// }"#).unwrap();
///
/// init(options).unwrap();
/// ```
pub fn init(options: TypeOptions) -> Result<()> {
    let source: Box<dyn ConfigSource> = create_trait_from_type_options(&options)?;
    let mut global = GLOBAL_SOURCE
        .write()
        .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;
    *global = Some(source);
    Ok(())
}

/// 全局加载配置
///
/// 使用全局配置源加载配置。
///
/// # 参数
/// - `key`: 配置键
/// - `format`: 配置格式，支持 "json", "json5", "yaml", "toml"，None 表示自动推断
///
/// # 返回
/// - 成功返回 ConfigValue，可通过 into_type() 转换为具体类型
/// - 失败返回错误信息（包括未初始化全局配置源的情况）
///
/// # 示例
/// ```ignore
/// use rustx::cfg::load;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct DatabaseConfig {
///     host: String,
///     port: u16,
/// }
///
/// let config_value = load("database", None).unwrap();
/// let config: DatabaseConfig = config_value.into_type().unwrap();
/// ```
pub fn load(key: &str, format: Option<&str>) -> Result<ConfigValue> {
    let global = GLOBAL_SOURCE
        .read()
        .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;
    match global.as_ref() {
        Some(source) => source.load(key, format),
        None => Err(anyhow::anyhow!(
            "Global source not initialized. Call init() first."
        )),
    }
}

/// 全局监听配置变化
///
/// 使用全局配置源监听配置变化。
///
/// # 参数
/// - `key`: 配置键
/// - `format`: 配置格式，支持 "json", "json5", "yaml", "toml"，None 表示自动推断
/// - `handler`: 配置变化时的回调函数
///
/// # 返回
/// - 成功返回 Ok(())，失败返回错误信息
///
/// # 示例
/// ```ignore
/// use rustx::cfg::{watch, ConfigChange};
///
/// watch("database", None, Box::new(|change| {
///     match change {
///         ConfigChange::Updated(value) => println!("配置已更新"),
///         ConfigChange::Deleted => println!("配置已删除"),
///         ConfigChange::Error(msg) => eprintln!("错误: {}", msg),
///     }
/// })).unwrap();
/// ```
pub fn watch(
    key: &str,
    format: Option<&str>,
    handler: Box<dyn Fn(ConfigChange) + Send + Sync + 'static>,
) -> Result<()> {
    let global = GLOBAL_SOURCE
        .read()
        .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;
    match global.as_ref() {
        Some(source) => source.watch(key, format, handler),
        None => Err(anyhow::anyhow!(
            "Global source not initialized. Call init() first."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[derive(Deserialize, Debug)]
    struct TestConfig {
        name: String,
        value: i32,
    }

    fn setup_global_source(temp_dir: &TempDir) -> Result<()> {
        // 注册所有配置源类型
        super::super::register_sources()?;

        // 创建测试配置文件
        let config_path = temp_dir.path().join("test.json");
        fs::write(
            &config_path,
            r#"{
                "name": "test-app",
                "value": 42
            }"#,
        )?;

        // 创建 TypeOptions 并初始化全局配置源
        let options = TypeOptions::from_json(&format!(
            r#"{{
                "type": "FileSource",
                "options": {{
                    "base_path": "{}"
                }}
            }}"#,
            temp_dir.path().to_string_lossy()
        ))?;

        init(options)?;

        Ok(())
    }

    #[test]
    #[serial]
    fn test_init_global_source() -> Result<()> {
        let temp_dir = TempDir::new()?;
        setup_global_source(&temp_dir)?;
        Ok(())
    }

    #[test]
    #[serial]
    fn test_load_from_global_source() -> Result<()> {
        let temp_dir = TempDir::new()?;
        setup_global_source(&temp_dir)?;

        // 测试加载配置
        let config_value = load("test.json", None)?;
        assert_eq!(config_value.as_value()["name"], "test-app");
        assert_eq!(config_value.as_value()["value"], 42);

        // 测试转换为具体类型
        let config: TestConfig = config_value.into_type()?;
        assert_eq!(config.name, "test-app");
        assert_eq!(config.value, 42);

        Ok(())
    }

    #[test]
    #[serial]
    fn test_load_before_init() {
        // 重置全局状态（如果之前的测试已初始化）
        {
            let mut global = GLOBAL_SOURCE.write().unwrap();
            *global = None;
        }

        // 尝试在未初始化时加载
        let result = load("test.json", None);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("not initialized"));
    }

    #[test]
    #[serial]
    fn test_load_with_explicit_format() -> Result<()> {
        let temp_dir = TempDir::new()?;
        setup_global_source(&temp_dir)?;

        // 创建 YAML 配置文件
        let yaml_path = temp_dir.path().join("test.yaml");
        fs::write(
            &yaml_path,
            r#"
name: yaml-app
value: 100
"#,
        )?;

        // 显式指定格式加载
        let config_value = load("test.yaml", Some("yaml"))?;
        assert_eq!(config_value.as_value()["name"], "yaml-app");
        assert_eq!(config_value.as_value()["value"], 100);

        Ok(())
    }

    #[test]
    #[serial]
    fn test_load_nonexistent_key() {
        let temp_dir = TempDir::new().unwrap();
        setup_global_source(&temp_dir).unwrap();

        // 尝试加载不存在的配置
        let result = load("nonexistent.json", None);
        assert!(result.is_err());
    }

}
