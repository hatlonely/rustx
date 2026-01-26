use crate::log::logger_manager::LoggerManager;
use crate::log::logger_manager::LoggerManagerConfig;
use crate::log::Logger;
use anyhow::Result;
use std::sync::Arc;

/// 全局 LoggerManager 单例
///
/// 默认包含一个输出到终端的文本形式 logger
static GLOBAL_LOGGER_MANAGER: once_cell::sync::Lazy<Arc<LoggerManager>> =
    once_cell::sync::Lazy::new(|| {
        // 使用默认配置
        let default_config = LoggerManagerConfig::default();

        Arc::new(LoggerManager::new(default_config).expect("Failed to create global LoggerManager"))
    });

/// 初始化全局 LoggerManager
///
/// # 示例
///
/// ```ignore
/// fn example() -> anyhow::Result<()> {
///     let config = LoggerManagerConfig {
///         default: default_logger_config,
///         loggers: logger_map,
///     };
///     ::rustx::log::init(config)?;
///     Ok(())
/// }
/// ```
pub fn init(config: LoggerManagerConfig) -> Result<()> {
    let manager = LoggerManager::new(config)?;

    // 合并 loggers 到全局单例
    let global_loggers = manager.loggers.read().unwrap();
    let mut global = GLOBAL_LOGGER_MANAGER.loggers.write().unwrap();

    // 复制所有 logger
    for (key, logger) in global_loggers.iter() {
        global.insert(key.clone(), logger.clone());
    }

    // 设置默认 logger
    GLOBAL_LOGGER_MANAGER.set_default(manager.get_default());

    Ok(())
}

/// 获取全局 LoggerManager
pub fn global_logger_manager() -> Arc<LoggerManager> {
    Arc::clone(&GLOBAL_LOGGER_MANAGER)
}

/// 获取指定 key 的 logger（全局）
pub fn get(key: &str) -> Option<Arc<Logger>> {
    global_logger_manager().get(key)
}

/// 获取指定 key 的 logger，如果不存在则返回默认 logger（全局）
pub fn get_or_default(key: &str) -> Arc<Logger> {
    global_logger_manager().get_or_default(key)
}

/// 获取默认 logger（全局）
pub fn get_default() -> Arc<Logger> {
    global_logger_manager().get_default()
}

/// 设置默认 logger（全局）
pub fn set_default(logger: Arc<Logger>) {
    global_logger_manager().set_default(logger)
}

/// 动态添加 logger（全局）
pub fn add(key: String, logger: Logger) {
    global_logger_manager().add(key, logger)
}

/// 检查指定 key 的 logger 是否存在（全局）
pub fn contains(key: &str) -> bool {
    global_logger_manager().contains(key)
}

/// 获取所有 logger 的 key 列表（全局）
pub fn keys() -> Vec<String> {
    global_logger_manager().keys()
}

/// 移除指定 key 的 logger（全局）
pub fn remove(key: &str) -> Option<Arc<Logger>> {
    global_logger_manager().remove(key)
}

// ========== 默认 logger 的便捷 log 方法 ==========

/// 使用默认 logger 记录日志
pub async fn log(record: crate::log::LogRecord) -> Result<()> {
    get_default().log(record).await
}

/// 使用默认 logger 记录带 metadata 的日志
pub async fn logm(
    level: crate::log::LogLevel,
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
) -> Result<()> {
    get_default().logm(level, message, metadata).await
}

/// 使用默认 logger 记录 TRACE 级别日志
pub async fn trace(message: impl Into<String>) -> Result<()> {
    get_default().trace(message).await
}

/// 使用默认 logger 记录 DEBUG 级别日志
pub async fn debug(message: impl Into<String>) -> Result<()> {
    get_default().debug(message).await
}

/// 使用默认 logger 记录 INFO 级别日志
pub async fn info(message: impl Into<String>) -> Result<()> {
    get_default().info(message).await
}

/// 使用默认 logger 记录 WARN 级别日志
pub async fn warn(message: impl Into<String>) -> Result<()> {
    get_default().warn(message).await
}

/// 使用默认 logger 记录 ERROR 级别日志
pub async fn error(message: impl Into<String>) -> Result<()> {
    get_default().error(message).await
}

/// 使用默认 logger 记录 TRACE 级别日志（带 metadata）
pub async fn tracem(
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
) -> Result<()> {
    get_default().tracem(message, metadata).await
}

/// 使用默认 logger 记录 DEBUG 级别日志（带 metadata）
pub async fn debugm(
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
) -> Result<()> {
    get_default().debugm(message, metadata).await
}

/// 使用默认 logger 记录 INFO 级别日志（带 metadata）
pub async fn infom(
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
) -> Result<()> {
    get_default().infom(message, metadata).await
}

/// 使用默认 logger 记录 WARN 级别日志（带 metadata）
pub async fn warnm(
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
) -> Result<()> {
    get_default().warnm(message, metadata).await
}

/// 使用默认 logger 记录 ERROR 级别日志（带 metadata）
pub async fn errorm(
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
) -> Result<()> {
    get_default().errorm(message, metadata).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::logger::LoggerConfig;

    /// 辅助函数：创建测试用的 LoggerCreateConfig
    fn create_test_logger_config(level: &str) -> crate::log::logger::LoggerCreateConfig {
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
    async fn test_global_logger_manager() -> Result<()> {
        // 测试获取全局单例
        let manager1 = global_logger_manager();
        let manager2 = global_logger_manager();

        // 验证是同一个实例
        assert!(Arc::ptr_eq(&manager1, &manager2));

        // 测试全局函数
        let logger = Logger::new(create_test_logger_config("info"))?;
        add("test_global".to_string(), logger);

        assert!(get("test_global").is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_init() -> Result<()> {
        let mut loggers = std::collections::HashMap::new();
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

        init(config)?;

        // 验证 logger 已添加到全局
        assert!(get("main").is_some());
        assert!(get("db").is_some());
        let _default = get_default();

        Ok(())
    }

    #[tokio::test]
    async fn test_default_logger_available() -> Result<()> {
        // 创建一个新的 LoggerManager 进行测试，避免受全局单例影响
        let config = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("info")),
            loggers: std::collections::HashMap::new(),
        };

        let manager = LoggerManager::new(config).unwrap();
        let default_logger = manager.get_default();

        // 验证默认 logger 存在
        assert!(Arc::ptr_eq(&default_logger, &manager.get_default()));

        // 测试可以正常使用
        let result = default_logger.info("Test default logger message").await;
        assert!(
            result.is_ok(),
            "Default logger should be able to log messages"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_default_logger_available_global() -> Result<()> {
        // 获取全局默认 logger（全局单例初始化时创建的）
        let default_logger = get_default();

        // 测试默认 logger 可以正常工作
        let result = default_logger.info("Test default logger message").await;
        assert!(
            result.is_ok(),
            "Default logger should be able to log messages"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_convenience_functions() -> Result<()> {
        // 测试全局便捷函数
        let logger = Logger::new(create_test_logger_config("info"))?;
        add("test".to_string(), logger);

        assert!(contains("test"));
        assert!(!contains("nonexistent"));

        let keys = keys();
        assert!(keys.contains(&"test".to_string()));

        let removed = remove("test");
        assert!(removed.is_some());
        assert!(!contains("test"));

        let removed_none = remove("nonexistent");
        assert!(removed_none.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_log_functions() -> Result<()> {
        // 测试全局日志便捷函数
        let result = info("test info message").await;
        assert!(result.is_ok(), "Global info function should work");

        let result = debug("test debug message").await;
        assert!(result.is_ok(), "Global debug function should work");

        let result = warn("test warn message").await;
        assert!(result.is_ok(), "Global warn function should work");

        let result = error("test error message").await;
        assert!(result.is_ok(), "Global error function should work");

        let result = trace("test trace message").await;
        assert!(result.is_ok(), "Global trace function should work");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_or_default_logger() -> Result<()> {
        // 创建独立的 LoggerManager 进行测试，避免受全局单例影响
        let config = LoggerManagerConfig {
            default: LoggerConfig::Create(create_test_logger_config("warn")),
            loggers: {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    "existing".to_string(),
                    LoggerConfig::Create(create_test_logger_config("info")),
                );
                map
            },
        };

        let manager = LoggerManager::new(config)?;

        // 存在的 key
        let result = manager.get_or_default("existing");
        assert_eq!(result.get_level().await, crate::log::LogLevel::Info);

        // 不存在的 key 返回默认
        let result = manager.get_or_default("nonexistent");
        assert_eq!(result.get_level().await, crate::log::LogLevel::Warn);

        Ok(())
    }

    #[tokio::test]
    async fn test_set_default_logger() -> Result<()> {
        // 创建新的 logger
        let new_logger = Arc::new(Logger::new(create_test_logger_config("debug"))?);

        // 设置为默认
        set_default(new_logger.clone());

        // 验证默认 logger 已更新
        let default = get_default();
        assert!(Arc::ptr_eq(&new_logger, &default));

        Ok(())
    }

    #[tokio::test]
    async fn test_convenience_functions_with_metadata() -> Result<()> {
        // 测试全局便捷函数（带 metadata）
        let result = infom(
            "user logged in",
            vec![("user_id", 12345i64.into()), ("username", "alice".into())],
        )
        .await;
        assert!(result.is_ok(), "Global infom function should work");

        let result = debugm(
            "processing request",
            vec![("endpoint", "/api/users".into()), ("method", "GET".into())],
        )
        .await;
        assert!(result.is_ok(), "Global debugm function should work");

        let result = warnm(
            "high memory usage",
            vec![("usage_mb", 512i64.into()), ("threshold_mb", 400i64.into())],
        )
        .await;
        assert!(result.is_ok(), "Global warnm function should work");

        let result = errorm(
            "database connection failed",
            vec![
                ("host", "localhost".into()),
                ("port", 5432i64.into()),
                ("error_code", "CONN001".into()),
            ],
        )
        .await;
        assert!(result.is_ok(), "Global errorm function should work");

        Ok(())
    }
}
