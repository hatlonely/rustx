use crate::log::Logger;
use crate::log::manager::LoggerManager;
use crate::log::manager::LoggerManagerConfig;
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
/// use rustx::log::init_logger_manager;
///
/// fn example() -> anyhow::Result<()> {
///     let config = LoggerManagerConfig {
///         default: default_logger_config,
///         loggers: logger_map,
///     };
///     init_logger_manager(config)?;
///     Ok(())
/// }
/// ```
pub fn init_logger_manager(config: LoggerManagerConfig) -> Result<()> {
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
pub fn get_logger(key: &str) -> Option<Arc<Logger>> {
    global_logger_manager().get_logger(key)
}

/// 获取默认 logger（全局）
pub fn get_default_logger() -> Arc<Logger> {
    global_logger_manager().get_default()
}

/// 动态添加 logger（全局）
pub fn add_logger(key: String, logger: Logger) {
    global_logger_manager().add_logger(key, logger)
}

// ========== 默认 logger 的便捷 log 方法 ==========

/// 使用默认 logger 记录日志
pub async fn log(record: crate::log::LogRecord) -> Result<()> {
    get_default_logger().log(record).await
}

/// 使用默认 logger 记录带 metadata 的日志
pub async fn logm(
    level: crate::log::LogLevel,
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::record::MetadataValue)>,
) -> Result<()> {
    get_default_logger().logm(level, message, metadata).await
}

/// 使用默认 logger 记录 TRACE 级别日志
pub async fn trace(message: impl Into<String>) -> Result<()> {
    get_default_logger().trace(message).await
}

/// 使用默认 logger 记录 DEBUG 级别日志
pub async fn debug(message: impl Into<String>) -> Result<()> {
    get_default_logger().debug(message).await
}

/// 使用默认 logger 记录 INFO 级别日志
pub async fn info(message: impl Into<String>) -> Result<()> {
    get_default_logger().info(message).await
}

/// 使用默认 logger 记录 WARN 级别日志
pub async fn warn(message: impl Into<String>) -> Result<()> {
    get_default_logger().warn(message).await
}

/// 使用默认 logger 记录 ERROR 级别日志
pub async fn error(message: impl Into<String>) -> Result<()> {
    get_default_logger().error(message).await
}

/// 使用默认 logger 记录 TRACE 级别日志（带 metadata）
pub async fn tracem(
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::record::MetadataValue)>,
) -> Result<()> {
    get_default_logger().tracem(message, metadata).await
}

/// 使用默认 logger 记录 DEBUG 级别日志（带 metadata）
pub async fn debugm(
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::record::MetadataValue)>,
) -> Result<()> {
    get_default_logger().debugm(message, metadata).await
}

/// 使用默认 logger 记录 INFO 级别日志（带 metadata）
pub async fn infom(
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::record::MetadataValue)>,
) -> Result<()> {
    get_default_logger().infom(message, metadata).await
}

/// 使用默认 logger 记录 WARN 级别日志（带 metadata）
pub async fn warnm(
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::record::MetadataValue)>,
) -> Result<()> {
    get_default_logger().warnm(message, metadata).await
}

/// 使用默认 logger 记录 ERROR 级别日志（带 metadata）
pub async fn errorm(
    message: impl Into<String>,
    metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::record::MetadataValue)>,
) -> Result<()> {
    get_default_logger().errorm(message, metadata).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::logger::LoggerConfig;

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
    async fn test_global_logger_manager() -> Result<()> {
        // 测试获取全局单例
        let manager1 = global_logger_manager();
        let manager2 = global_logger_manager();

        // 验证是同一个实例
        assert!(Arc::ptr_eq(&manager1, &manager2));

        // 测试全局函数
        let logger = Logger::new(create_test_logger_config("info"))?;
        add_logger("test_global".to_string(), logger);

        assert!(get_logger("test_global").is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_init_logger_manager() -> Result<()> {
        let mut loggers = std::collections::HashMap::new();
        loggers.insert("main".to_string(), create_test_logger_config("info"));
        loggers.insert("db".to_string(), create_test_logger_config("debug"));

        let config = LoggerManagerConfig {
            default: create_test_logger_config("warn"),
            loggers,
        };

        init_logger_manager(config)?;

        // 验证 logger 已添加到全局
        assert!(get_logger("main").is_some());
        assert!(get_logger("db").is_some());
        let _default = get_default_logger();

        Ok(())
    }

    #[tokio::test]
    async fn test_default_logger_available() -> Result<()> {
        // 获取默认 logger（全局单例初始化时创建的）
        let default_logger = get_default_logger();

        // 测试默认 logger 可以正常工作
        let result = default_logger.info("Test default logger message").await;
        assert!(result.is_ok(), "Default logger should be able to log messages");

        Ok(())
    }

    #[tokio::test]
    async fn test_convenience_functions() -> Result<()> {
        // 测试全局便捷函数
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
    async fn test_convenience_functions_with_metadata() -> Result<()> {
        // 测试全局便捷函数（带 metadata）
        let result = infom(
            "user logged in",
            vec![
                ("user_id", 12345i64.into()),
                ("username", "alice".into())
            ]
        ).await;
        assert!(result.is_ok(), "Global infom function should work");

        let result = debugm(
            "processing request",
            vec![
                ("endpoint", "/api/users".into()),
                ("method", "GET".into())
            ]
        ).await;
        assert!(result.is_ok(), "Global debugm function should work");

        let result = warnm(
            "high memory usage",
            vec![
                ("usage_mb", 512i64.into()),
                ("threshold_mb", 400i64.into())
            ]
        ).await;
        assert!(result.is_ok(), "Global warnm function should work");

        let result = errorm(
            "database connection failed",
            vec![
                ("host", "localhost".into()),
                ("port", 5432i64.into()),
                ("error_code", "CONN001".into())
            ]
        ).await;
        assert!(result.is_ok(), "Global errorm function should work");

        Ok(())
    }
}
