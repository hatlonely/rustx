//! 注册所有 ConfigSource 实现
//!
//! 提供统一的配置源注册接口，支持通过配置创建不同的 ConfigSource 实例

use anyhow::Result;

use crate::cfg::register_trait;

use super::{ApolloSource, ApolloSourceConfig, ConfigSource, FileSource, FileSourceConfig};

/// 注册所有基础 ConfigSource 实现
///
/// 注册所有可用的 ConfigSource 实现。
///
/// # 注册的类型
/// - `FileSource` - 文件配置源
/// - `ApolloSource` - Apollo 配置中心源
///
/// # 示例
/// ```ignore
/// use rustx::cfg::{register_sources, TypeOptions, create_trait_from_type_options};
///
/// // 注册所有配置源
/// register_sources()?;
///
/// // 通过配置创建实例
/// let opts = TypeOptions::from_json(r#"{
///     "type": "FileSource",
///     "options": {
///         "base_path": "/etc/config"
///     }
/// }"#)?;
///
/// let source: Box<dyn ConfigSource> = create_trait_from_type_options(&opts)?;
/// ```
pub fn register_sources() -> Result<()> {
    register_trait::<FileSource, dyn ConfigSource, FileSourceConfig>("FileSource")?;
    register_trait::<ApolloSource, dyn ConfigSource, ApolloSourceConfig>("ApolloSource")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{create_trait_from_type_options, TypeOptions};
    use tempfile::TempDir;
    use std::fs;
    use serde::Deserialize;

    #[test]
    fn test_register_sources_file_source() -> Result<()> {
        // 注册所有配置源
        register_sources()?;

        // 创建临时目录
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("test.json");

        // 写入测试配置
        fs::write(
            &config_path,
            r#"{
                "host": "localhost",
                "port": 3306
            }"#,
        )?;

        // 创建 FileSource 配置
        let opts = TypeOptions::from_json(
            format!(r#"{{
                "type": "FileSource",
                "options": {{
                    "base_path": "{}"
                }}
            }}"#, temp_dir.path().to_string_lossy()).as_str()
        )?;

        let source: Box<dyn ConfigSource> = create_trait_from_type_options(&opts)?;

        // 测试加载配置
        let config = source.load("test")?;
        assert_eq!(config.as_value()["host"], "localhost");
        assert_eq!(config.as_value()["port"], 3306);

        Ok(())
    }

    #[test]
    fn test_register_sources_apollo_source() -> Result<()> {
        register_sources()?;

        // 创建 ApolloSource 配置
        let opts = TypeOptions::from_json(
            r#"{
                "type": "ApolloSource",
                "options": {
                    "server_url": "http://localhost:8080",
                    "app_id": "test-app",
                    "namespace": "application",
                    "cluster": "default"
                }
            }"#
        )?;

        let source: Box<dyn ConfigSource> = create_trait_from_type_options(&opts)?;

        // 验证 source 可以正常创建（实际加载需要连接 Apollo 服务器）
        // 这里我们只测试对象创建是否成功
        let result = source.load("test");

        // 应该失败，因为没有实际的 Apollo 服务器
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_register_sources_with_type_options() -> Result<()> {
        register_sources()?;

        // 测试使用 TypeOptions 创建不同的配置源
        let file_opts = TypeOptions {
            type_name: "FileSource".to_string(),
            options: serde_json::json!({
                "base_path": "/tmp/config"
            }),
        };

        let apollo_opts = TypeOptions {
            type_name: "ApolloSource".to_string(),
            options: serde_json::json!({
                "server_url": "http://localhost:8080",
                "app_id": "my-app",
                "namespace": "application",
                "cluster": "default"
            }),
        };

        let _file_source: Box<dyn ConfigSource> = create_trait_from_type_options(&file_opts)?;
        let _apollo_source: Box<dyn ConfigSource> = create_trait_from_type_options(&apollo_opts)?;

        Ok(())
    }

    #[test]
    fn test_register_sources_unregistered_type() {
        register_sources().unwrap();

        // 尝试创建未注册的类型
        let opts = TypeOptions {
            type_name: "UnknownSource".to_string(),
            options: serde_json::json!({}),
        };

        let result: Result<Box<dyn ConfigSource>> = create_trait_from_type_options(&opts);
        assert!(result.is_err());

        if let Err(e) = result {
            let error_msg = e.to_string();
            assert!(error_msg.contains("not registered") || error_msg.contains("registered"));
        }
    }

    #[derive(Deserialize, Debug)]
    struct TestConfig {
        name: String,
        value: i32,
    }

    #[test]
    fn test_register_sources_file_source_load_and_deserialize() -> Result<()> {
        register_sources()?;

        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("app.json");

        fs::write(
            &config_path,
            r#"{
                "name": "test-app",
                "value": 42
            }"#,
        )?;

        let opts = TypeOptions::from_json(
            format!(r#"{{
                "type": "FileSource",
                "options": {{
                    "base_path": "{}"
                }}
            }}"#, temp_dir.path().to_string_lossy()).as_str()
        )?;

        let source: Box<dyn ConfigSource> = create_trait_from_type_options(&opts)?;

        // 加载并反序列化为具体类型
        let config_value = source.load("app")?;
        let config: TestConfig = config_value.into_type()?;

        assert_eq!(config.name, "test-app");
        assert_eq!(config.value, 42);

        Ok(())
    }
}
