use crate::log::Logger;
use crate::log::logger::LoggerConfig;
use anyhow::Result;
use serde::Deserialize;
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Logger Manager 配置
///
/// 用于统一管理多个 Logger 实例
#[derive(Debug, Clone, Deserialize, SmartDefault)]
#[serde(default)]
pub struct LoggerManagerConfig {
    /// 全局默认配置（如果未配置则使用默认值）
    #[default(LoggerConfig::default())]
    pub default: LoggerConfig,

    /// 命名 logger 配置映射
    pub loggers: HashMap<String, LoggerConfig>,
}

/// Logger 管理器
///
/// 全局单例，负责统一维护所有 Logger 实例
pub struct LoggerManager {
    pub(crate) loggers: Arc<RwLock<HashMap<String, Arc<Logger>>>>,
    default: Arc<RwLock<Arc<Logger>>>,
}

impl LoggerManager {
    /// 从配置创建 LoggerManager
    pub fn new(config: LoggerManagerConfig) -> Result<Self> {
        let mut loggers_map = HashMap::new();

        // 创建命名 logger 实例
        for (key, logger_config) in config.loggers {
            let logger = Logger::new(logger_config)?;
            loggers_map.insert(key, Arc::new(logger));
        }

        // 创建默认 logger（始终存在）
        let default_logger = Arc::new(Logger::new(config.default)?);

        Ok(Self {
            loggers: Arc::new(RwLock::new(loggers_map)),
            default: Arc::new(RwLock::new(default_logger)),
        })
    }

    /// 获取指定 key 的 logger
    ///
    /// 如果 key 不存在，返回 None
    pub fn get_logger(&self, key: &str) -> Option<Arc<Logger>> {
        let loggers = self.loggers.read().unwrap();
        loggers.get(key).cloned()
    }

    /// 获取默认 logger
    pub fn get_default(&self) -> Arc<Logger> {
        let default = self.default.read().unwrap();
        Arc::clone(&default)
    }

    /// 设置默认 logger
    pub fn set_default(&self, logger: Arc<Logger>) {
        let mut default = self.default.write().unwrap();
        *default = logger;
    }

    /// 动态添加 logger
    pub fn add_logger(&self, key: String, logger: Logger) {
        let mut loggers = self.loggers.write().unwrap();
        loggers.insert(key, Arc::new(logger));
    }

    /// 检查指定 key 的 logger 是否存在
    pub fn contains(&self, key: &str) -> bool {
        let loggers = self.loggers.read().unwrap();
        loggers.contains_key(key)
    }

    /// 获取所有 logger 的 key 列表
    pub fn keys(&self) -> Vec<String> {
        let loggers = self.loggers.read().unwrap();
        loggers.keys().cloned().collect()
    }

    /// 移除指定 key 的 logger
    pub fn remove_logger(&self, key: &str) -> Option<Arc<Logger>> {
        let mut loggers = self.loggers.write().unwrap();
        loggers.remove(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::LogLevel;

    /// 辅助函数：创建测试用的 LoggerConfig
    fn create_test_logger_config(level: &str) -> LoggerConfig {
        let config_json = format!(r#"{{
            level: "{}",
            formatter: {{
                type: "TextFormatter",
                options: {{}}
            }},
            appender: {{
                type: "ConsoleAppender",
                options: {{}}
            }}
        }}"#, level);

        json5::from_str(&config_json).expect("Failed to parse LoggerConfig")
    }

    #[tokio::test]
    async fn test_manager_new() -> Result<()> {
        let mut loggers = HashMap::new();
        loggers.insert("main".to_string(), create_test_logger_config("info"));
        loggers.insert("db".to_string(), create_test_logger_config("debug"));

        let config = LoggerManagerConfig {
            default: create_test_logger_config("warn"),
            loggers,
        };

        let manager = LoggerManager::new(config)?;

        // 测试获取 logger
        assert!(manager.contains("main"));
        assert!(manager.contains("db"));
        assert!(!manager.contains("nonexistent"));

        // 测试默认 logger
        let _default = manager.get_default();

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_get_logger() -> Result<()> {
        let mut loggers = HashMap::new();
        loggers.insert("main".to_string(), create_test_logger_config("info"));

        let config = LoggerManagerConfig {
            default: create_test_logger_config("debug"),
            loggers,
        };

        let manager = LoggerManager::new(config)?;

        // 测试获取存在的 logger
        let logger = manager.get_logger("main");
        assert!(logger.is_some());
        assert_eq!(logger.unwrap().get_level().await, LogLevel::Info);

        // 测试获取不存在的 logger
        assert!(manager.get_logger("nonexistent").is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_add_logger() -> Result<()> {
        let config = LoggerManagerConfig {
            default: create_test_logger_config("info"),
            loggers: HashMap::new(),
        };

        let manager = LoggerManager::new(config)?;

        // 动态添加 logger
        let logger = Logger::new(create_test_logger_config("debug"))?;
        manager.add_logger("dynamic".to_string(), logger);

        // 验证添加成功
        assert!(manager.contains("dynamic"));
        assert!(manager.get_logger("dynamic").is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_keys() -> Result<()> {
        let mut loggers = HashMap::new();
        loggers.insert("a".to_string(), create_test_logger_config("info"));
        loggers.insert("b".to_string(), create_test_logger_config("debug"));
        loggers.insert("c".to_string(), create_test_logger_config("warn"));

        let config = LoggerManagerConfig {
            default: create_test_logger_config("info"),
            loggers,
        };

        let manager = LoggerManager::new(config)?;

        // 测试获取所有 keys
        let keys = manager.keys();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"a".to_string()));
        assert!(keys.contains(&"b".to_string()));
        assert!(keys.contains(&"c".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_remove_logger() -> Result<()> {
        let mut loggers = HashMap::new();
        loggers.insert("main".to_string(), create_test_logger_config("info"));

        let config = LoggerManagerConfig {
            default: create_test_logger_config("debug"),
            loggers,
        };

        let manager = LoggerManager::new(config)?;

        // 测试移除存在的 logger
        let removed = manager.remove_logger("main");
        assert!(removed.is_some());
        assert!(!manager.contains("main"));

        // 测试移除不存在的 logger
        assert!(manager.remove_logger("nonexistent").is_none());

        Ok(())
    }
}
