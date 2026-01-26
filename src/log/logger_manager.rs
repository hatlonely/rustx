use crate::cfg::ConfigReloader;
use crate::log::logger::LoggerConfig;
use crate::log::Logger;
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
    config: Arc<RwLock<LoggerManagerConfig>>,
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
            LoggerConfig::Create(create_config) => Arc::new(Logger::new(create_config.clone())?),
        };

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
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
        if let Some(logger) = crate::log::get(instance) {
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
    pub fn get(&self, key: &str) -> Option<Arc<Logger>> {
        let loggers = self.loggers.read().unwrap();
        loggers.get(key).cloned()
    }

    /// 获取指定 key 的 logger，如果不存在则返回默认 logger
    pub fn get_or_default(&self, key: &str) -> Arc<Logger> {
        self.get(key).unwrap_or_else(|| self.get_default())
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
    pub fn add(&self, key: String, logger: Logger) {
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
    pub fn remove(&self, key: &str) -> Option<Arc<Logger>> {
        let mut loggers = self.loggers.write().unwrap();
        loggers.remove(key)
    }
}

/// 为 LoggerManager 实现 ConfigReloader trait
///
/// 支持配置热更新，采用增量更新策略：
/// - 配置未变化的 Logger 实例会被保留
/// - 配置变化的 Logger 实例会被重新创建
/// - 新配置中不存在的 key 会被删除
impl ConfigReloader<LoggerManagerConfig> for LoggerManager {
    fn reload_config(&mut self, new_config: LoggerManagerConfig) -> Result<()> {
        // 锁定旧配置和实例
        let old_config = self.config.read().unwrap();
        let old_loggers = self.loggers.read().unwrap();

        // 创建新的 loggers map
        let mut new_loggers = HashMap::new();
        let mut reference_configs: Vec<(String, String)> = Vec::new();

        // 第一步：处理所有 Create 模式的 logger
        for (key, new_logger_config) in &new_config.loggers {
            match new_logger_config {
                LoggerConfig::Reference { instance } => {
                    // 记录引用关系，稍后处理
                    reference_configs.push((key.clone(), instance.clone()));
                }
                LoggerConfig::Create(new_create_config) => {
                    // 检查旧配置是否存在且相同
                    let should_reuse = match old_config.loggers.get(key) {
                        Some(LoggerConfig::Create(old_create_config)) => {
                            old_create_config == new_create_config
                        }
                        _ => false,
                    };

                    if should_reuse {
                        // 配置未变化，复用旧实例
                        if let Some(old_logger) = old_loggers.get(key) {
                            new_loggers.insert(key.clone(), Arc::clone(old_logger));
                            continue;
                        }
                    }

                    // 配置变化或不存在，创建新实例
                    let logger = Arc::new(Logger::new(new_create_config.clone())?);
                    new_loggers.insert(key.clone(), logger);
                }
            }
        }

        // 第二步：处理所有 Reference 模式的配置
        for (key, instance) in reference_configs {
            let logger = Self::resolve_logger_config_by_name(&instance, &new_loggers)?;
            new_loggers.insert(key, logger);
        }

        // 第三步：处理默认 logger
        let new_default_logger = match &new_config.default {
            LoggerConfig::Reference { instance } => {
                Self::resolve_logger_config_by_name(instance, &new_loggers)?
            }
            LoggerConfig::Create(new_create_config) => {
                // 检查旧 default 配置是否存在且相同
                let should_reuse = match &old_config.default {
                    LoggerConfig::Create(old_create_config) => {
                        old_create_config == new_create_config
                    }
                    _ => false,
                };

                if should_reuse {
                    // 配置未变化，复用旧实例
                    let default_logger = self.default.read().unwrap();
                    let old_default = (*default_logger).clone();
                    drop(default_logger);
                    old_default
                } else {
                    // 配置变化，创建新实例
                    Arc::new(Logger::new(new_create_config.clone())?)
                }
            }
        };

        // 释放旧锁
        drop(old_config);
        drop(old_loggers);

        // 第四步：更新内部状态
        {
            let mut config_write = self.config.write().unwrap();
            *config_write = new_config;
        }

        {
            let mut loggers_write = self.loggers.write().unwrap();
            *loggers_write = new_loggers;
        }

        {
            let mut default_write = self.default.write().unwrap();
            *default_write = new_default_logger;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::LogLevel;

    /// 辅助函数：创建测试用的 LoggerCreateConfig
    fn create_test_logger_config(level: &str) -> crate::log::LoggerCreateConfig {
        let config_json = format!(
            r#"{{
            level: "{}",
            formatter: {{
                type: "TextFormatter",
                options: {{}}
            }},
            appender: {{
                type: "ConsoleAppender",
                options: {{}}
            }}
        }}"#,
            level
        );

        json5::from_str(&config_json).expect("Failed to parse LoggerCreateConfig")
    }

    #[tokio::test]
    async fn test_manager_new() -> Result<()> {
        let mut loggers = HashMap::new();
        loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("info")),
        );
        loggers.insert(
            "db".to_string(),
            LoggerConfig::Create(create_test_logger_config("debug")),
        );

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
    async fn test_manager_get() -> Result<()> {
        let mut loggers = HashMap::new();
        loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("info")),
        );

        let config = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("debug")),
            loggers,
        };

        let manager = LoggerManager::new(config)?;

        // 测试获取存在的 logger
        let logger = manager.get("main");
        assert!(logger.is_some());
        assert_eq!(logger.unwrap().get_level().await, LogLevel::Info);

        // 测试获取不存在的 logger
        assert!(manager.get("nonexistent").is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_get_or_default() -> Result<()> {
        let mut loggers = HashMap::new();
        loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("info")),
        );

        let config = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("debug")),
            loggers,
        };

        let manager = LoggerManager::new(config)?;

        // 测试获取存在的 logger
        let logger = manager.get_or_default("main");
        assert_eq!(logger.get_level().await, LogLevel::Info);

        // 测试获取不存在的 logger 返回默认
        let logger = manager.get_or_default("nonexistent");
        assert_eq!(logger.get_level().await, LogLevel::Debug);

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_add() -> Result<()> {
        let config = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("info")),
            loggers: HashMap::new(),
        };

        let manager = LoggerManager::new(config)?;

        // 动态添加 logger
        let logger = Logger::new(create_test_logger_config("debug"))?;
        manager.add("dynamic".to_string(), logger);

        // 验证添加成功
        assert!(manager.contains("dynamic"));
        assert!(manager.get("dynamic").is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_keys() -> Result<()> {
        let mut loggers = HashMap::new();
        loggers.insert(
            "a".to_string(),
            LoggerConfig::Create(create_test_logger_config("info")),
        );
        loggers.insert(
            "b".to_string(),
            LoggerConfig::Create(create_test_logger_config("debug")),
        );
        loggers.insert(
            "c".to_string(),
            LoggerConfig::Create(create_test_logger_config("warn")),
        );

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
    async fn test_manager_remove() -> Result<()> {
        let mut loggers = HashMap::new();
        loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("info")),
        );

        let config = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("debug")),
            loggers,
        };

        let manager = LoggerManager::new(config)?;

        // 测试移除存在的 logger
        let removed = manager.remove("main");
        assert!(removed.is_some());
        assert!(!manager.contains("main"));

        // 测试移除不存在的 logger
        assert!(manager.remove("nonexistent").is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_reference_instance() -> Result<()> {
        let mut loggers = HashMap::new();

        // 创建一个完整的 logger
        loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("info")),
        );

        // 引用 main logger
        loggers.insert(
            "api".to_string(),
            LoggerConfig::Reference {
                instance: "main".to_string(),
            },
        );

        // 也引用 main logger
        loggers.insert(
            "service".to_string(),
            LoggerConfig::Reference {
                instance: "main".to_string(),
            },
        );

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
        let main_logger = manager.get("main").unwrap();
        let api_logger = manager.get("api").unwrap();
        let service_logger = manager.get("service").unwrap();

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

    #[tokio::test]
    async fn test_config_reloader_keep_unchanged() -> Result<()> {
        // 创建初始配置
        let mut loggers = HashMap::new();
        loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("info")),
        );
        loggers.insert(
            "db".to_string(),
            LoggerConfig::Create(create_test_logger_config("debug")),
        );

        let config1 = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("warn")),
            loggers: loggers.clone(),
        };

        let mut manager = LoggerManager::new(config1)?;

        // 保存旧实例引用
        let old_main = manager.get("main").unwrap();
        let old_db = manager.get("db").unwrap();
        let old_default = manager.get_default();

        // 重载相同配置
        let config2 = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("warn")),
            loggers,
        };

        manager.reload_config(config2)?;

        // 验证实例未被替换（配置未变化）
        let new_main = manager.get("main").unwrap();
        let new_db = manager.get("db").unwrap();
        let new_default = manager.get_default();

        assert!(Arc::ptr_eq(&old_main, &new_main));
        assert!(Arc::ptr_eq(&old_db, &new_db));
        assert!(Arc::ptr_eq(&old_default, &new_default));

        Ok(())
    }

    #[tokio::test]
    async fn test_config_reloader_change_level() -> Result<()> {
        // 创建初始配置
        let mut loggers = HashMap::new();
        loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("info")),
        );
        loggers.insert(
            "db".to_string(),
            LoggerConfig::Create(create_test_logger_config("debug")),
        );

        let config1 = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("warn")),
            loggers,
        };

        let mut manager = LoggerManager::new(config1)?;

        // 保存旧实例引用
        let old_main = manager.get("main").unwrap();
        let old_db = manager.get("db").unwrap();

        // 重载配置（修改 main 的日志级别）
        let mut new_loggers = HashMap::new();
        new_loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("error")), // 改为 error
        );
        new_loggers.insert(
            "db".to_string(),
            LoggerConfig::Create(create_test_logger_config("debug")), // 保持不变
        );

        let config2 = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("warn")),
            loggers: new_loggers,
        };

        manager.reload_config(config2)?;

        // 验证实例是否被替换
        let new_main = manager.get("main").unwrap();
        let new_db = manager.get("db").unwrap();

        // main 的配置变了，应该被重建
        assert!(!Arc::ptr_eq(&old_main, &new_main));
        assert_eq!(new_main.get_level().await, LogLevel::Error);

        // db 的配置未变，应该复用旧实例
        assert!(Arc::ptr_eq(&old_db, &new_db));

        Ok(())
    }

    #[tokio::test]
    async fn test_config_reloader_add_remove_logger() -> Result<()> {
        // 创建初始配置
        let mut loggers = HashMap::new();
        loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("info")),
        );
        loggers.insert(
            "db".to_string(),
            LoggerConfig::Create(create_test_logger_config("debug")),
        );

        let config1 = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("warn")),
            loggers,
        };

        let mut manager = LoggerManager::new(config1)?;

        // 保存旧实例引用
        let old_main = manager.get("main").unwrap();

        // 重载配置（删除 db，添加 api）
        let mut new_loggers = HashMap::new();
        new_loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("info")), // 保持不变
        );
        new_loggers.insert(
            "api".to_string(),
            LoggerConfig::Create(create_test_logger_config("trace")), // 新增
        );

        let config2 = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("warn")),
            loggers: new_loggers,
        };

        manager.reload_config(config2)?;

        // 验证结果
        assert!(manager.contains("main"));
        assert!(!manager.contains("db")); // db 被删除
        assert!(manager.contains("api")); // api 被添加

        // main 应该复用旧实例
        let new_main = manager.get("main").unwrap();
        assert!(Arc::ptr_eq(&old_main, &new_main));

        // db 已不存在
        assert!(manager.get("db").is_none());

        // api 是新创建的
        let new_api = manager.get("api").unwrap();
        assert_eq!(new_api.get_level().await, LogLevel::Trace);

        Ok(())
    }

    #[tokio::test]
    async fn test_config_reloader_with_reference() -> Result<()> {
        // 创建初始配置
        let mut loggers = HashMap::new();
        loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("info")),
        );
        loggers.insert(
            "api".to_string(),
            LoggerConfig::Reference {
                instance: "main".to_string(),
            },
        );

        let config1 = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("warn")),
            loggers,
        };

        let mut manager = LoggerManager::new(config1)?;

        // 验证初始引用关系
        let main1 = manager.get("main").unwrap();
        let api1 = manager.get("api").unwrap();
        assert!(Arc::ptr_eq(&main1, &api1));

        // 重载配置（修改 main 的级别）
        let mut new_loggers = HashMap::new();
        new_loggers.insert(
            "main".to_string(),
            LoggerConfig::Create(create_test_logger_config("error")), // 改变配置
        );
        new_loggers.insert(
            "api".to_string(),
            LoggerConfig::Reference {
                instance: "main".to_string(),
            },
        );

        let config2 = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("warn")),
            loggers: new_loggers,
        };

        manager.reload_config(config2)?;

        // 验证引用关系仍然保持
        let main2 = manager.get("main").unwrap();
        let api2 = manager.get("api").unwrap();
        assert!(Arc::ptr_eq(&main2, &api2));

        // 验证 main 被重建
        assert!(!Arc::ptr_eq(&main1, &main2));
        assert_eq!(main2.get_level().await, LogLevel::Error);

        Ok(())
    }
}
