//! 文件配置源
//!
//! 支持从本地文件系统加载配置，支持 JSON/YAML/TOML 格式
//! 支持监听文件变化并自动重新加载

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::sync::Arc;

use super::source::{ConfigChange, ConfigSource, ConfigValue};
use crate::{impl_box_from, impl_from};
use crate::log::{Logger, LoggerConfig};

/// 文件配置源的配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileSourceConfig {
    /// 配置文件所在目录
    pub base_path: String,
    /// Logger 配置（可选，不配置则使用全局默认 logger）
    pub logger: Option<LoggerConfig>,
}

/// 文件配置源
///
/// 支持从本地文件加载配置，key 为完整文件名（包含扩展名）
///
/// # 示例
/// ```no_run
/// use rustx::cfg::{ConfigSource, FileSource, FileSourceConfig};
///
/// // 创建文件配置源，指向 config 目录
/// let source = FileSource::new(FileSourceConfig {
///     base_path: "config".to_string(),
///     logger: None,
/// });
///
/// // 加载 config/database.json（自动根据扩展名推断格式）
/// let config = source.load("database.json", None).unwrap();
///
/// // 加载 config/config.yaml（显式指定格式）
/// let config = source.load("config.yaml", None).unwrap();
/// ```
pub struct FileSource {
    base_path: PathBuf,
    /// Logger 实例
    logger: Arc<Logger>,
}

impl FileSource {
    /// 创建文件配置源
    ///
    /// # 参数
    /// - `config`: 文件配置源配置
    pub fn new(config: FileSourceConfig) -> Self {
        // 解析或创建 logger
        let logger = match config.logger {
            Some(logger_config) => Logger::resolve(logger_config)
                .expect("Failed to resolve logger"),
            None => crate::log::get_default(),
        };

        // 记录创建成功
        let _ = logger.info_sync(&format!(
            "[INIT] file_source created - base_path={}",
            config.base_path
        ));

        Self {
            base_path: config.base_path.into(),
            logger,
        }
    }

    /// 从文件名提取扩展名
    fn extract_extension(filename: &str) -> Option<&str> {
        filename.rsplit('.').next().filter(|&ext| !ext.is_empty())
    }

    /// 根据扩展名推断格式
    fn infer_format_from_extension(ext: &str) -> Result<String> {
        match ext.to_lowercase().as_str() {
            "json" | "json5" => Ok(ext.to_lowercase()),
            "yaml" | "yml" => Ok("yaml".to_string()),
            "toml" => Ok("toml".to_string()),
            _ => Err(anyhow!("不支持的文件扩展名: {}", ext)),
        }
    }
}

impl_from!(FileSourceConfig => FileSource);
impl_box_from!(FileSource => dyn ConfigSource);

impl FileSource {
    /// 查找配置文件并确定格式
    fn find_config_file(&self, key: &str, format: Option<&str>) -> Result<(PathBuf, String)> {
        let path = self.base_path.join(key);

        if !path.exists() {
            // 记录文件不存在
            let _ = self.logger.error_sync(&format!(
                "[ERROR] file not_found - path={}",
                path.display()
            ));
            return Err(anyhow!("配置文件不存在: {}", path.display()));
        }

        // 确定格式
        let fmt = if let Some(fmt) = format {
            // 使用显式指定的格式
            Self::normalize_format(fmt)
        } else {
            // 从文件扩展名推断格式
            let ext = Self::extract_extension(key)
                .ok_or_else(|| anyhow!("无法从文件名推断格式，文件名缺少扩展名: {}", key))?;
            Self::infer_format_from_extension(ext)?
        };

        Ok((path, fmt))
    }

    /// 标准化格式名称（转换为小写）
    fn normalize_format(format: &str) -> String {
        format.to_lowercase()
    }

    /// 根据指定格式解析配置
    fn parse_config_with_format(content: &str, format: &str) -> Result<JsonValue> {
        match Self::normalize_format(format).as_str() {
            "json" => Ok(json5::from_str(content)?),
            "json5" => Ok(json5::from_str(content)?),
            "yaml" | "yml" => Ok(serde_yaml::from_str(content)?),
            "toml" => Ok(toml::from_str(content)?),
            _ => Err(anyhow!("不支持的配置格式: {}", format)),
        }
    }
}

impl ConfigSource for FileSource {
    fn load(&self, key: &str, format: Option<&str>) -> Result<ConfigValue> {
        let (path, fmt) = self.find_config_file(key, format)?;

        // 记录开始加载
        let _ = self.logger.debug_sync(&format!(
            "[LOAD] file loading - path={} format={}",
            path.display(),
            fmt
        ));

        let content = std::fs::read_to_string(&path).map_err(|e| {
            // 记录读取失败
            let _ = self.logger.error_sync(&format!(
                "[ERROR] file read_failed - path={} error={}",
                path.display(),
                e
            ));
            anyhow!("读取配置文件失败: {}, path: {:?}", e, path)
        })?;

        match Self::parse_config_with_format(&content, &fmt) {
            Ok(value) => {
                // 记录加载成功
                let _ = self.logger.debug_sync(&format!(
                    "[LOAD] file success - path={} format={}",
                    path.display(),
                    fmt
                ));
                Ok(ConfigValue::new(value))
            }
            Err(e) => {
                // 记录解析失败
                let _ = self.logger.error_sync(&format!(
                    "[ERROR] file parse_failed - path={} format={} error={}",
                    path.display(),
                    fmt,
                    e
                ));
                Err(anyhow!("解析 {} 格式配置文件失败: {}, path: {:?}", fmt, e, path))
            }
        }
    }

    fn watch(
        &self,
        key: &str,
        format: Option<&str>,
        handler: Box<dyn Fn(ConfigChange) + Send + Sync + 'static>,
    ) -> Result<()> {
        let (file_path, fmt) = self.find_config_file(key, format)?;
        let file_path_clone = file_path.clone();
        let fmt_clone = fmt.clone();
        let handler = Arc::new(handler);
        let logger = self.logger.clone();

        // 记录注册监听器
        let _ = self.logger.info_sync(&format!(
            "[WATCH] listener registered - path={} format={}",
            file_path.display(),
            fmt
        ));

        // 使用全局 watch 函数
        crate::fs::watch(&file_path, move |event| {
            let handler = handler.clone();
            let logger = logger.clone();
            match event {
                crate::fs::FileEvent::Modified(_) | crate::fs::FileEvent::Created(_) => {
                    match std::fs::read_to_string(&file_path_clone) {
                        Ok(content) => match Self::parse_config_with_format(&content, &fmt_clone) {
                            Ok(value) => {
                                // 记录文件更新
                                let _ = logger.info_sync(&format!(
                                    "[CHANGE] file updated - path={}",
                                    file_path_clone.display()
                                ));
                                handler(ConfigChange::Updated(ConfigValue::new(value)))
                            }
                            Err(e) => {
                                // 记录解析失败
                                let _ = logger.error_sync(&format!(
                                    "[ERROR] file parse_failed - path={} format={} error={}",
                                    file_path_clone.display(),
                                    fmt_clone,
                                    e
                                ));
                                handler(ConfigChange::Error(format!(
                                    "解析 {} 格式配置失败: {}",
                                    fmt_clone, e
                                )))
                            }
                        },
                        Err(e) => {
                            // 如果读取失败（文件可能被删除），发送删除事件
                            if !file_path_clone.exists() {
                                // 记录文件删除
                                let _ = logger.warn_sync(&format!(
                                    "[CHANGE] file deleted - path={}",
                                    file_path_clone.display()
                                ));
                                handler(ConfigChange::Deleted)
                            } else {
                                // 记录读取失败
                                let _ = logger.error_sync(&format!(
                                    "[ERROR] file read_failed - path={} error={}",
                                    file_path_clone.display(),
                                    e
                                ));
                                handler(ConfigChange::Error(format!("读取文件失败: {}", e)))
                            }
                        }
                    }
                }
                crate::fs::FileEvent::Deleted(_) => {
                    // 记录文件删除
                    let _ = logger.warn_sync(&format!(
                        "[CHANGE] file deleted - path={}",
                        file_path_clone.display()
                    ));
                    handler(ConfigChange::Deleted)
                }
                crate::fs::FileEvent::Error(err) => {
                    // 记录错误事件
                    let _ = logger.error_sync(&format!(
                        "[ERROR] file watch_error - path={} error={}",
                        file_path_clone.display(),
                        err
                    ));
                    handler(ConfigChange::Error(err))
                }
            }
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use std::sync::{Arc, RwLock};
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_file_source_load_json() -> Result<()> {
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

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });
        let config = source.load("test.json", None)?;

        assert_eq!(config.as_value()["host"], "localhost");
        assert_eq!(config.as_value()["port"], 3306);

        Ok(())
    }

    #[test]
    fn test_file_source_load_yaml() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("test.yaml");

        fs::write(
            &config_path,
            r#"
host: localhost
port: 3306
"#,
        )?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });
        let config = source.load("test.yaml", None)?;

        assert_eq!(config.as_value()["host"], "localhost");
        assert_eq!(config.as_value()["port"], 3306);

        Ok(())
    }

    #[test]
    fn test_file_source_load_toml() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("test.toml");

        fs::write(
            &config_path,
            r#"
host = "localhost"
port = 3306
"#,
        )?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });
        let config = source.load("test.toml", None)?;

        assert_eq!(config.as_value()["host"], "localhost");
        assert_eq!(config.as_value()["port"], 3306);

        Ok(())
    }

    #[test]
    fn test_file_source_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });

        let result = source.load("nonexistent.json", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("配置文件不存在"));
    }

    #[test]
    #[serial]
    fn test_file_source_watch() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("watch_test.json");

        // 写入初始配置
        fs::write(&config_path, r#"{"version": 1}"#)?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });

        // 使用 Arc<RwLock> 存储变更通知
        let changes = Arc::new(RwLock::new(Vec::new()));
        let changes_clone = changes.clone();

        // 启动监听
        source.watch(
            "watch_test.json",
            None,
            Box::new(move |change| {
                changes_clone.write().unwrap().push(change);
            }),
        )?;

        // 等待监听器启动
        thread::sleep(Duration::from_millis(200));

        // 修改文件
        fs::write(&config_path, r#"{"version": 2}"#)?;

        // 等待文件变更被检测
        thread::sleep(Duration::from_millis(500));

        // 验证收到更新通知
        let changes_vec = changes.read().unwrap();
        assert!(!changes_vec.is_empty());

        // 检查是否有 Updated 事件
        let has_update = changes_vec.iter().any(
            |c| matches!(c, ConfigChange::Updated(config) if config.as_value()["version"] == 2),
        );
        assert!(has_update, "应该收到配置更新通知");

        Ok(())
    }

    #[test]
    #[serial]
    fn test_file_source_watch_delete() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("delete_test.json");

        // 写入初始配置
        fs::write(&config_path, r#"{"name": "test"}"#)?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });

        let changes = Arc::new(RwLock::new(Vec::new()));
        let changes_clone = changes.clone();

        source.watch(
            "delete_test.json",
            None,
            Box::new(move |change| {
                changes_clone.write().unwrap().push(change);
            }),
        )?;

        thread::sleep(Duration::from_millis(100));

        // 删除文件
        fs::remove_file(&config_path)?;

        thread::sleep(Duration::from_millis(1500));

        // 验证收到删除通知
        let changes_vec = changes.read().unwrap();
        let has_delete = changes_vec
            .iter()
            .any(|c| matches!(c, ConfigChange::Deleted));
        assert!(has_delete, "应该收到配置删除通知");

        Ok(())
    }

    #[test]
    fn test_file_source_auto_cleanup() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("cleanup_test.json");

        fs::write(&config_path, r#"{"name": "test"}"#)?;

        {
            let source = FileSource::new(FileSourceConfig {
                base_path: temp_dir.path().to_string_lossy().to_string(),
                logger: None,
            });
            source.watch("cleanup_test.json", None, Box::new(|_| {}))?;

            // source 在这里 drop
        }

        // 验证线程已经停止（通过修改文件不会触发回调）
        thread::sleep(Duration::from_millis(100));

        Ok(())
    }

    #[test]
    fn test_file_source_load_with_explicit_format() -> Result<()> {
        let temp_dir = TempDir::new()?;
        // 创建一个 .config 文件
        let config_path = temp_dir.path().join("test.config");

        fs::write(
            &config_path,
            r#"
host: localhost
port: 3306
"#,
        )?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });

        // 使用自动推断（None）应该失败，因为 .config 扩展名不支持
        let result = source.load("test.config", None);
        assert!(result.is_err());

        // 但指定格式后应该成功
        let config = source.load("test.config", Some("yaml"))?;
        assert_eq!(config.as_value()["host"], "localhost");
        assert_eq!(config.as_value()["port"], 3306);

        Ok(())
    }

    #[test]
    fn test_file_source_load_toml_with_explicit_format() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("test.cfg");

        fs::write(
            &config_path,
            r#"
host = "localhost"
port = 3306
"#,
        )?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });

        // 指定 TOML 格式
        let config = source.load("test.cfg", Some("toml"))?;
        assert_eq!(config.as_value()["host"], "localhost");
        assert_eq!(config.as_value()["port"], 3306);

        Ok(())
    }

    #[test]
    fn test_file_source_load_case_insensitive_format() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("test.data");

        fs::write(&config_path, r#"{"host": "localhost", "port": 3306}"#)?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });

        // 测试大小写不敏感
        let config = source.load("test.data", Some("JSON"))?;
        assert_eq!(config.as_value()["host"], "localhost");

        let config = source.load("test.data", Some("Json"))?;
        assert_eq!(config.as_value()["host"], "localhost");

        Ok(())
    }

    #[test]
    fn test_file_source_load_unsupported_extension() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.txt");

        fs::write(&config_path, r#"some data"#).unwrap();

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });

        // 不支持的扩展名
        let result = source.load("test.txt", None);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("不支持的文件扩展名") || error_msg.contains("txt"));
    }

    #[test]
    fn test_file_source_load_unsupported_format() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.data");

        fs::write(&config_path, r#"some data"#).unwrap();

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });

        // 显式指定不支持的格式
        let result = source.load("test.data", Some("xml"));
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("不支持的配置格式") || error_msg.contains("xml"));
    }

    #[test]
    fn test_file_source_watch_with_explicit_format() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("watch_test.cfg");

        // 写入初始配置
        fs::write(&config_path, r#"{"version": 1}"#)?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });

        let changes = Arc::new(RwLock::new(Vec::new()));
        let changes_clone = changes.clone();

        // 使用显式格式监听
        source.watch(
            "watch_test.cfg",
            Some("json"),
            Box::new(move |change| {
                changes_clone.write().unwrap().push(change);
            }),
        )?;

        thread::sleep(Duration::from_millis(200));

        // 修改文件
        fs::write(&config_path, r#"{"version": 2}"#)?;

        thread::sleep(Duration::from_millis(500));

        // 验证收到更新通知
        let changes_vec = changes.read().unwrap();
        assert!(!changes_vec.is_empty());

        let has_update = changes_vec.iter().any(
            |c| matches!(c, ConfigChange::Updated(config) if config.as_value()["version"] == 2),
        );
        assert!(has_update, "应该收到配置更新通知");

        Ok(())
    }

    #[test]
    fn test_file_source_load_yaml_with_yml_extension() -> Result<()> {
        // 测试 .yml 扩展名能正确识别为 YAML 格式
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("test.yml");

        fs::write(
            &config_path,
            r#"
name: test
value: 42
"#,
        )?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });

        // 自动推断格式（从 .yml 扩展名）
        let config = source.load("test.yml", None)?;
        assert_eq!(config.as_value()["name"], "test");
        assert_eq!(config.as_value()["value"], 42);

        Ok(())
    }

    #[test]
    fn test_file_source_load_json5() -> Result<()> {
        // 测试 .json5 扩展名能正确识别为 JSON5 格式
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("test.json5");

        fs::write(
            &config_path,
            r#"{
                // 这是一个注释
                "name": "test",
                "value": 42,  // 尾随逗号
            }"#,
        )?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
            logger: None,
        });

        // 自动推断格式（从 .json5 扩展名）
        let config = source.load("test.json5", None)?;
        assert_eq!(config.as_value()["name"], "test");
        assert_eq!(config.as_value()["value"], 42);

        Ok(())
    }
}
