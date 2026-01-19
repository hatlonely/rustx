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

        // 第一步：创建所有 Create 模式的 logger
        let mut reference_configs: Vec<(String, String)> = Vec::new();

        for (key, logger_config) in &config.loggers {
            match logger_config {
                LoggerConfig::Reference { instance } => {
                    // 记录引用关系，稍后处理
                    reference_configs.push((key.clone(), instance.clone()));
                }
                LoggerConfig::Create(create_config) => {
                    // 直接创建新的 logger
                    let logger = Arc::new(Logger::new(create_config.clone())?);
                    loggers_map.insert(key.clone(), logger);
                }
            }
        }

        // 第二步：处理所有 Reference 模式的配置
        for (key, instance) in reference_configs {
            let logger = Self::resolve_logger_config_by_name(&instance, &loggers_map)?;
            loggers_map.insert(key, logger);
        }

        // 创建默认 logger（始终存在）
        let default_logger = match &config.default {
            LoggerConfig::Reference { instance } => {
                Self::resolve_logger_config_by_name(instance, &loggers_map)?
            }
            LoggerConfig::Create(create_config) => {
                Arc::new(Logger::new(create_config.clone())?)
            }
        };

        Ok(Self {
            loggers: Arc::new(RwLock::new(loggers_map)),
            default: Arc::new(RwLock::new(default_logger)),
        })
    }

    /// 根据名称解析 Logger 实例
    ///
    /// 先从已创建的 loggers 中查找，再从全局管理器中查找
    fn resolve_logger_config_by_name(
        instance: &str,
        created_loggers: &HashMap<String, Arc<Logger>>,
    ) -> Result<Arc<Logger>> {
        // 先从当前已创建的 loggers 中查找
        if let Some(logger) = created_loggers.get(instance) {
            return Ok(Arc::clone(logger));
        }

        // 再从全局管理器中查找
        if let Some(logger) = crate::log::get_logger(instance) {
            return Ok(logger);
        }

        // 都找不到，返回错误
        Err(anyhow::anyhow!(
            "Logger instance '{}' not found (neither in current config nor in global manager)",
            instance
        ))
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

    /// 辅助函数：创建测试用的 LoggerCreateConfig
    fn create_test_logger_config(level: &str) -> crate::log::LoggerCreateConfig {
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

        json5::from_str(&config_json).expect("Failed to parse LoggerCreateConfig")
    }

    #[tokio::test]
    async fn test_manager_new() -> Result<()> {
        let mut loggers = HashMap::new();
        loggers.insert("main".to_string(), LoggerConfig::Create(create_test_logger_config("info")));
        loggers.insert("db".to_string(), LoggerConfig::Create(create_test_logger_config("debug")));

        let config = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("warn")),
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
        loggers.insert("main".to_string(), LoggerConfig::Create(create_test_logger_config("info")));

        let config = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("debug")),
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
            default: LoggerConfig::Create(create_test_logger_config("info")),
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
        loggers.insert("a".to_string(), LoggerConfig::Create(create_test_logger_config("info")));
        loggers.insert("b".to_string(), LoggerConfig::Create(create_test_logger_config("debug")));
        loggers.insert("c".to_string(), LoggerConfig::Create(create_test_logger_config("warn")));

        let config = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("info")),
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
        loggers.insert("main".to_string(), LoggerConfig::Create(create_test_logger_config("info")));

        let config = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("debug")),
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

    #[tokio::test]
    async fn test_manager_reference_instance() -> Result<()> {
        let mut loggers = HashMap::new();

        // 创建一个完整的 logger
        loggers.insert("main".to_string(), LoggerConfig::Create(create_test_logger_config("info")));

        // 引用 main logger
        loggers.insert("api".to_string(), LoggerConfig::Reference {
            instance: "main".to_string(),
        });

        // 也引用 main logger
        loggers.insert("service".to_string(), LoggerConfig::Reference {
            instance: "main".to_string(),
        });

        let config = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("debug")),
            loggers,
        };

        let manager = LoggerManager::new(config)?;

        // 验证所有 logger 都存在
        assert!(manager.contains("main"));
        assert!(manager.contains("api"));
        assert!(manager.contains("service"));

        // 验证 api 和 service 都指向同一个 logger 实例
        let main_logger = manager.get_logger("main").unwrap();
        let api_logger = manager.get_logger("api").unwrap();
        let service_logger = manager.get_logger("service").unwrap();

        // 使用 Arc::ptr_eq 检查是否是同一个实例
        assert!(Arc::ptr_eq(&main_logger, &api_logger));
        assert!(Arc::ptr_eq(&main_logger, &service_logger));
        assert!(Arc::ptr_eq(&api_logger, &service_logger));

        // 验证日志级别相同
        assert_eq!(main_logger.get_level().await, LogLevel::Info);
        assert_eq!(api_logger.get_level().await, LogLevel::Info);
        assert_eq!(service_logger.get_level().await, LogLevel::Info);

        Ok(())
    }
}
