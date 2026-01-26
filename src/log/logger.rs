use crate::cfg::{create_trait_from_type_options, TypeOptions};
use crate::log::{
    appender::LogAppender, formatter::LogFormatter, log_record::LogLevel, log_record::LogRecord,
};
use anyhow::Result;
use serde::Deserialize;
use smart_default::SmartDefault;
use std::sync::{Arc, Once};
use tokio::sync::RwLock;

/// Logger 创建配置（用于创建新的 Logger 实例）
#[derive(Debug, Clone, Deserialize, SmartDefault, PartialEq)]
#[serde(default)]
pub struct LoggerCreateConfig {
    /// 日志级别
    #[default = "info"]
    pub level: String,

    /// Formatter 配置
    #[default(TypeOptions { type_name: "TextFormatter".to_string(), options: serde_json::json!({}) })]
    pub formatter: TypeOptions,

    /// Appender 配置
    #[default(TypeOptions { type_name: "ConsoleAppender".to_string(), options: serde_json::json!({}) })]
    pub appender: TypeOptions,
}

/// Logger 配置
///
/// 支持两种模式：
/// - Reference: 引用已存在的 logger 实例（通过 $instance 字段）
/// - Create: 创建新的 logger 实例
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum LoggerConfig {
    /// 引用一个已存在的 logger 实例
    Reference {
        /// 引用的 logger 实例名称
        #[serde(rename = "$instance")]
        instance: String,
    },

    /// 创建新的 logger 实例
    Create(LoggerCreateConfig),
}

impl Default for LoggerConfig {
    fn default() -> Self {
        LoggerConfig::Create(LoggerCreateConfig::default())
    }
}

/// 注册所有日志组件（只执行一次）
static REGISTER_ONCE: Once = Once::new();

/// 核心日志器
///
/// 负责日志的级别控制、格式化和输出
pub struct Logger {
    level: Arc<RwLock<LogLevel>>,
    formatter: Arc<dyn LogFormatter>,
    appender: Arc<dyn LogAppender>,
}

impl Logger {
    /// 从创建配置创建 Logger
    pub fn new(config: LoggerCreateConfig) -> Result<Self> {
        // 注册所有日志组件（只执行一次）
        REGISTER_ONCE.call_once(|| {
            let _ = crate::log::register_formatters();
            let _ = crate::log::register_appenders();
        });

        // 解析日志级别
        let level = config.level.parse::<LogLevel>().unwrap_or(LogLevel::Info);

        // 创建 formatter
        let formatter_box = create_trait_from_type_options(&config.formatter)?;
        let formatter: Arc<dyn LogFormatter> = Arc::from(formatter_box);

        // 创建 appender
        let appender_box = create_trait_from_type_options(&config.appender)?;
        let appender: Arc<dyn LogAppender> = Arc::from(appender_box);

        Ok(Self {
            level: Arc::new(RwLock::new(level)),
            formatter,
            appender,
        })
    }

    /// 从配置解析 Logger
    ///
    /// 如果配置是 Reference 模式，从全局管理器获取已存在的 logger
    /// 如果配置是 Create 模式，创建新的 logger
    pub fn resolve(config: LoggerConfig) -> Result<Arc<Self>> {
        match config {
            LoggerConfig::Reference { instance } => {
                // 从全局管理器获取已存在的 logger
                crate::log::get(&instance).ok_or_else(|| {
                    anyhow::anyhow!("Logger instance '{}' not found in global manager", instance)
                })
            }

            LoggerConfig::Create(create_config) => {
                // 创建新的 logger
                Ok(Arc::new(Logger::new(create_config)?))
            }
        }
    }

    /// 设置日志级别
    pub async fn set_level(&self, level: LogLevel) {
        *self.level.write().await = level;
    }

    /// 获取当前日志级别
    pub async fn get_level(&self) -> LogLevel {
        *self.level.read().await
    }

    /// 记录日志
    pub async fn log(&self, record: LogRecord) -> Result<()> {
        // 检查日志级别
        let current_level = *self.level.read().await;
        if record.level < current_level {
            return Ok(());
        }

        // 格式化日志
        let formatted = self.formatter.format(&record)?;

        // 输出日志
        self.appender.append(&formatted).await?;

        Ok(())
    }

    /// 记录带 metadata 的日志（通用方法）
    ///
    /// # 示例
    ///
    /// ```ignore
    /// logger.logm(
    ///     LogLevel::Info,
    ///     "user logged in",
    ///     vec![
    ///         ("user_id", 12345.into()),
    ///         ("username", "alice".into())
    ///     ]
    /// ).await?;
    /// ```
    pub async fn logm(
        &self,
        level: LogLevel,
        message: impl Into<String>,
        metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
    ) -> Result<()> {
        let mut record = LogRecord::new(level, message.into());
        for (key, value) in metadata.into_iter() {
            record.metadata.push((key.into(), value));
        }
        self.log(record).await
    }

    /// 记录 TRACE 级别日志
    pub async fn trace(&self, message: impl Into<String>) -> Result<()> {
        self.log(LogRecord::new(LogLevel::Trace, message.into()))
            .await
    }

    /// 记录 DEBUG 级别日志
    pub async fn debug(&self, message: impl Into<String>) -> Result<()> {
        self.log(LogRecord::new(LogLevel::Debug, message.into()))
            .await
    }

    /// 记录 INFO 级别日志
    pub async fn info(&self, message: impl Into<String>) -> Result<()> {
        self.log(LogRecord::new(LogLevel::Info, message.into()))
            .await
    }

    /// 记录 WARN 级别日志
    pub async fn warn(&self, message: impl Into<String>) -> Result<()> {
        self.log(LogRecord::new(LogLevel::Warn, message.into()))
            .await
    }

    /// 记录 ERROR 级别日志
    pub async fn error(&self, message: impl Into<String>) -> Result<()> {
        self.log(LogRecord::new(LogLevel::Error, message.into()))
            .await
    }

    /// 记录 TRACE 级别日志（带 metadata）
    ///
    /// # 示例
    ///
    /// ```ignore
    /// logger.tracem("entering function", vec![
    ///     ("function", "process".into()),
    ///     ("user_id", 12345.into())
    /// ]).await?;
    /// ```
    pub async fn tracem(
        &self,
        message: impl Into<String>,
        metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
    ) -> Result<()> {
        self.logm(LogLevel::Trace, message, metadata).await
    }

    /// 记录 DEBUG 级别日志（带 metadata）
    ///
    /// # 示例
    ///
    /// ```ignore
    /// logger.debugm("processing request", vec![
    ///     ("endpoint", "/api/users".into()),
    ///     ("method", "GET".into())
    /// ]).await?;
    /// ```
    pub async fn debugm(
        &self,
        message: impl Into<String>,
        metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
    ) -> Result<()> {
        self.logm(LogLevel::Debug, message, metadata).await
    }

    /// 记录 INFO 级别日志（带 metadata）
    ///
    /// # 示例
    ///
    /// ```ignore
    /// logger.infom("user logged in", vec![
    ///     ("user_id", 12345.into()),
    ///     ("username", "alice".into())
    /// ]).await?;
    /// ```
    pub async fn infom(
        &self,
        message: impl Into<String>,
        metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
    ) -> Result<()> {
        self.logm(LogLevel::Info, message, metadata).await
    }

    /// 记录 WARN 级别日志（带 metadata）
    ///
    /// # 示例
    ///
    /// ```ignore
    /// logger.warnm("high memory usage", vec![
    ///     ("usage_mb", 512.into()),
    ///     ("threshold_mb", 400.into())
    /// ]).await?;
    /// ```
    pub async fn warnm(
        &self,
        message: impl Into<String>,
        metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
    ) -> Result<()> {
        self.logm(LogLevel::Warn, message, metadata).await
    }

    /// 记录 ERROR 级别日志（带 metadata）
    ///
    /// # 示例
    ///
    /// ```ignore
    /// logger.errorm("database connection failed", vec![
    ///     ("host", "localhost".into()),
    ///     ("port", 5432.into()),
    ///     ("error_code", "CONN001".into())
    /// ]).await?;
    /// ```
    pub async fn errorm(
        &self,
        message: impl Into<String>,
        metadata: impl IntoIterator<Item = (impl Into<String>, crate::log::log_record::MetadataValue)>,
    ) -> Result<()> {
        self.logm(LogLevel::Error, message, metadata).await
    }
}

impl From<LoggerCreateConfig> for Logger {
    fn from(config: LoggerCreateConfig) -> Self {
        Logger::new(config).expect("Failed to create Logger")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 辅助函数：为测试创建一个简单的 Logger
    fn create_test_logger(level: &str) -> Logger {
        // 使用 json5::from_str 创建 config
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

        let config: LoggerCreateConfig =
            json5::from_str(&config_json).expect("Failed to parse LoggerCreateConfig");

        Logger::new(config).unwrap()
    }

    #[tokio::test]
    async fn test_logger_new() {
        let logger = create_test_logger("info");
        assert_eq!(logger.get_level().await, LogLevel::Info);
    }

    #[tokio::test]
    async fn test_logger_set_level() {
        let logger = create_test_logger("info");

        logger.set_level(LogLevel::Debug).await;
        assert_eq!(logger.get_level().await, LogLevel::Debug);
    }

    #[tokio::test]
    async fn test_logger_log_level_filtering() -> Result<()> {
        let logger = create_test_logger("info");

        // DEBUG 级别低于 INFO，应该被过滤
        let result = logger.debug("debug msg").await;
        assert!(result.is_ok());

        // INFO 级别应该被记录
        let result = logger.info("info msg").await;
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_logger_from_config() -> Result<()> {
        // 使用 json5::from_str 创建 config
        let config: LoggerCreateConfig = json5::from_str(
            r#"
            {
                level: "debug",
                formatter: {
                    type: "TextFormatter",
                    options: {
                        colored: false
                    }
                },
                appender: {
                    type: "ConsoleAppender",
                    options: {
                        target: "stdout",
                        auto_flush: true
                    }
                }
            }
            "#,
        )?;

        let logger = Logger::new(config)?;
        assert_eq!(logger.get_level().await, LogLevel::Debug);

        Ok(())
    }

    #[tokio::test]
    async fn test_logger_logm_with_metadata() -> Result<()> {
        let logger = create_test_logger("debug");

        // 测试 logm 方法
        let result = logger
            .logm(
                LogLevel::Info,
                "user action",
                vec![("user_id", 12345i64.into()), ("action", "login".into())],
            )
            .await;

        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_logger_infom_with_metadata() -> Result<()> {
        let logger = create_test_logger("info");

        // 测试 infom 方法
        let result = logger
            .infom(
                "user logged in",
                vec![("user_id", 12345i64.into()), ("username", "alice".into())],
            )
            .await;

        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_logger_debugm_with_metadata() -> Result<()> {
        let logger = create_test_logger("debug");

        // 测试 debugm 方法
        let result = logger
            .debugm(
                "processing request",
                vec![("endpoint", "/api/users".into()), ("method", "GET".into())],
            )
            .await;

        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_logger_warnm_with_metadata() -> Result<()> {
        let logger = create_test_logger("warn");

        // 测试 warnm 方法
        let result = logger
            .warnm(
                "high memory usage",
                vec![("usage_mb", 512i64.into()), ("threshold_mb", 400i64.into())],
            )
            .await;

        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_logger_errorm_with_metadata() -> Result<()> {
        let logger = create_test_logger("error");

        // 测试 errorm 方法
        let result = logger
            .errorm(
                "database connection failed",
                vec![
                    ("host", "localhost".into()),
                    ("port", 5432i64.into()),
                    ("error_code", "CONN001".into()),
                ],
            )
            .await;

        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_logger_infom_with_mixed_types() -> Result<()> {
        let logger = create_test_logger("info");

        // 测试 infom 方法（混合类型）
        let result = logger
            .infom(
                "user logged in",
                vec![
                    ("user_id", 12345i64.into()),
                    ("username", "alice".into()),
                    ("success", true.into()),
                ],
            )
            .await;

        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_logger_metadata_methods_with_filtering() -> Result<()> {
        let logger = create_test_logger("info");

        // DEBUG 级别低于 INFO，应该被过滤但不应报错
        let result = logger
            .debugm("debug message", vec![("key1", "value1".into())])
            .await;
        assert!(result.is_ok());

        // INFO 级别应该被记录
        let result = logger
            .infom("info message", vec![("key2", "value2".into())])
            .await;
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_logger_with_struct_metadata() -> Result<()> {
        use crate::log::log_record::MetadataValue;
        use serde::Serialize;

        #[derive(Serialize)]
        struct UserInfo {
            user_id: i64,
            username: String,
            email: String,
            role: String,
        }

        #[derive(Serialize)]
        struct RequestContext {
            endpoint: String,
            method: String,
            client_ip: String,
        }

        let logger = create_test_logger("info");

        // 测试使用自定义结构体作为 metadata
        let user_info = UserInfo {
            user_id: 12345,
            username: "alice".to_string(),
            email: "alice@example.com".to_string(),
            role: "admin".to_string(),
        };

        let request_context = RequestContext {
            endpoint: "/api/users".to_string(),
            method: "GET".to_string(),
            client_ip: "192.168.1.100".to_string(),
        };

        let result = logger
            .infom(
                "user action completed",
                vec![
                    ("action", "get_profile".into()),
                    ("success", true.into()),
                    ("user", MetadataValue::from_struct(user_info)),
                    ("request", MetadataValue::from_struct(request_context)),
                ],
            )
            .await;

        assert!(result.is_ok());

        Ok(())
    }
}
