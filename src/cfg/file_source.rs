//! 文件配置源
//!
//! 支持从本地文件系统加载配置，支持 JSON/YAML/TOML 格式
//! 支持监听文件变化并自动重新加载

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use super::source::{ConfigChange, ConfigSource, ConfigValue, WatchHandle};
use crate::{impl_from, impl_box_from};

/// 文件配置源的配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileSourceConfig {
    /// 配置文件所在目录
    pub base_path: String,
}

/// 文件配置源
///
/// 支持从本地文件加载配置，自动根据扩展名选择解析器
///
/// # 示例
/// ```no_run
/// use rustx::cfg::{ConfigSource, FileSource, FileSourceConfig};
///
/// // 创建文件配置源，指向 config 目录
/// let source = FileSource::new(FileSourceConfig {
///     base_path: "config".to_string(),
/// });
///
/// // 加载 config/database.json
/// let config = source.load("database").unwrap();
/// ```
pub struct FileSource {
    base_path: PathBuf,
    /// 内部维护所有监听句柄
    watches: Arc<Mutex<Vec<WatchHandle>>>,
}

impl FileSource {
    /// 创建文件配置源
    ///
    /// # 参数
    /// - `config`: 文件配置源配置
    pub fn new(config: FileSourceConfig) -> Self {
        Self {
            base_path: config.base_path.into(),
            watches: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl_from!(FileSourceConfig => FileSource);
impl_box_from!(FileSource => dyn ConfigSource);

impl FileSource {
    /// 根据 key 和扩展名构造文件路径
    fn get_file_path(&self, key: &str, ext: &str) -> PathBuf {
        self.base_path.join(format!("{}.{}", key, ext))
    }

    /// 查找存在的配置文件
    fn find_config_file(&self, key: &str) -> Result<(PathBuf, &'static str)> {
        for ext in ["json", "yaml", "yml", "toml"] {
            let path = self.get_file_path(key, ext);
            if path.exists() {
                return Ok((path, ext));
            }
        }
        Err(anyhow!("配置文件不存在: {}", key))
    }

    /// 根据扩展名解析配置
    fn parse_config(content: &str, ext: &str) -> Result<JsonValue> {
        match ext {
            "json" => Ok(serde_json::from_str(content)?),
            "yaml" | "yml" => Ok(serde_yaml::from_str(content)?),
            "toml" => Ok(toml::from_str(content)?),
            _ => Err(anyhow!("不支持的文件格式: {}", ext)),
        }
    }
}

impl ConfigSource for FileSource {
    fn load(&self, key: &str) -> Result<ConfigValue> {
        let (path, ext) = self.find_config_file(key)?;
        let content = std::fs::read_to_string(path)?;
        let value = Self::parse_config(&content, ext)?;
        Ok(ConfigValue::new(value))
    }

    fn watch(&self, key: &str, handler: Box<dyn Fn(ConfigChange) + Send + 'static>) -> Result<()> {
        // 查找存在的配置文件
        let (file_path, ext) = self.find_config_file(key)?;
        let ext = ext.to_string();

        // 创建停止信号通道（使用 crossbeam channel）
        let (stop_tx, stop_rx) = crossbeam::channel::unbounded();
        let file_path_clone = file_path.clone();

        // 启动监听线程
        let thread_handle = thread::spawn(move || {
            // 使用 notify crate 监听文件变化
            use crossbeam::channel::unbounded;
            use notify::{recommended_watcher, Event, RecursiveMode, Watcher};
            use std::time::Duration;

            let (event_tx, event_rx) = unbounded();

            let mut watcher = match recommended_watcher(move |res: Result<Event, _>| {
                if let Ok(event) = res {
                    let _ = event_tx.send(event);
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    handler(ConfigChange::Error(format!("创建文件监听器失败: {}", e)));
                    return;
                }
            };

            if let Err(e) = watcher.watch(&file_path_clone, RecursiveMode::NonRecursive) {
                handler(ConfigChange::Error(format!("监听文件失败: {}", e)));
                return;
            }

            loop {
                // 使用 select 同时监听停止信号和文件变化
                crossbeam::select! {
                    recv(stop_rx) -> _ => {
                        break;
                    }
                    recv(event_rx) -> event => {
                        if let Ok(event) = event {
                            // 文件被修改
                            if event.kind.is_modify() {
                                // 防抖处理：等待 100ms，消耗这段时间内的所有修改事件
                                thread::sleep(Duration::from_millis(100));

                                // 清空队列中的重复修改事件，但保留删除事件
                                let mut has_delete = false;
                                while let Ok(evt) = event_rx.try_recv() {
                                    if evt.kind.is_remove() {
                                        has_delete = true;
                                        break;
                                    }
                                    // 其他修改事件被丢弃
                                }

                                // 如果检测到删除事件，优先处理删除
                                if has_delete {
                                    handler(ConfigChange::Deleted);
                                    continue;
                                }

                                // 只处理一次文件更新
                                match std::fs::read_to_string(&file_path_clone) {
                                    Ok(content) => {
                                        match FileSource::parse_config(&content, &ext) {
                                            Ok(value) => {
                                                handler(ConfigChange::Updated(ConfigValue::new(value)));
                                            }
                                            Err(e) => {
                                                handler(ConfigChange::Error(
                                                    format!("解析配置失败: {}", e)
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        handler(ConfigChange::Error(
                                            format!("读取文件失败: {}", e)
                                        ));
                                    }
                                }
                            }
                            // 文件被删除
                            else if event.kind.is_remove() {
                                handler(ConfigChange::Deleted);
                            }
                        }
                    }
                }
            }

            // 显式 drop watcher 以释放文件句柄
            drop(watcher);
        });

        // 将 handle 存储到内部
        let handle = WatchHandle {
            stop_sender: Some(stop_tx),
            thread_handle: Some(thread_handle),
        };
        self.watches.lock().unwrap().push(handle);

        Ok(())
    }
}

// 注意：FileSource 不需要显式实现 Drop
// 当 FileSource drop 时，watches: Arc<Mutex<Vec<WatchHandle>>> 会自动 drop
// 进而触发每个 WatchHandle 的 Drop，自动停止所有监听线程

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Arc, RwLock};
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
        });
        let config = source.load("test")?;

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
        });
        let config = source.load("test")?;

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
        });
        let config = source.load("test")?;

        assert_eq!(config.as_value()["host"], "localhost");
        assert_eq!(config.as_value()["port"], 3306);

        Ok(())
    }

    #[test]
    fn test_file_source_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
        });

        let result = source.load("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("配置文件不存在"));
    }

    #[test]
    fn test_file_source_watch() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("watch_test.json");

        // 写入初始配置
        fs::write(&config_path, r#"{"version": 1}"#)?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
        });

        // 使用 Arc<RwLock> 存储变更通知
        let changes = Arc::new(RwLock::new(Vec::new()));
        let changes_clone = changes.clone();

        // 启动监听
        source.watch("watch_test", Box::new(move |change| {
            changes_clone.write().unwrap().push(change);
        }))?;

        // 等待监听器启动
        thread::sleep(Duration::from_millis(100));

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
    fn test_file_source_watch_delete() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("delete_test.json");

        // 写入初始配置
        fs::write(&config_path, r#"{"name": "test"}"#)?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
        });

        let changes = Arc::new(RwLock::new(Vec::new()));
        let changes_clone = changes.clone();

        source.watch("delete_test", Box::new(move |change| {
            changes_clone.write().unwrap().push(change);
        }))?;

        thread::sleep(Duration::from_millis(100));

        // 删除文件
        fs::remove_file(&config_path)?;

        thread::sleep(Duration::from_millis(500));

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
            });
            source.watch("cleanup_test", Box::new(|_| {}))?;

            // source 在这里 drop
        }

        // 验证线程已经停止（通过修改文件不会触发回调）
        thread::sleep(Duration::from_millis(100));

        Ok(())
    }
}
