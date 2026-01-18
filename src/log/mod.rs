//! 日志模块
//!
//! 提供灵活的日志功能，支持多种日志级别、格式和输出方式。
//!
//! # 特性
//!
//! - 多种日志级别：Trace, Debug, Info, Warn, Error
//! - 可扩展的格式化器：TextFormatter、JsonFormatter
//! - 多种输出目标：ConsoleAppender、FileAppender
//! - 基于配置的动态创建
//! - 完全异步支持
//!
//! # 快速开始
//!
//! ```rust,no_run
//! use rustx::log::*;
//! use rustx::cfg::TypeOptions;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // 1. 注册所有组件
//!     register_log_components()?;
//!
//!     // 2. 构建 LoggerConfig
//!     let config = LoggerConfig {
//!         level: "info".to_string(),
//!         formatter: TypeOptions::from_json(r#"
//!             {
//!                 "type": "TextFormatter",
//!                 "options": {
//!                     "colored": false
//!                 }
//!             }
//!         "#)?,
//!         appender: TypeOptions::from_json(r#"
//!             {
//!                 "type": "ConsoleAppender",
//!                 "options": {
//!                     "use_colors": true
//!                 }
//!             }
//!         "#)?,
//!     };
//!
//!     // 3. 创建 Logger
//!     let logger = create_logger_from_config(config).await?;
//!
//!     // 4. 使用 Logger
//!     logger.info("Application started".to_string()).await?;
//!     logger.error("Connection failed".to_string()).await?;
//!
//!     Ok(())
//! }
//! ```

pub mod appender;
pub mod formatter;
pub mod level;
pub mod logger;
pub mod macros;
pub mod record;
pub mod registry;

// 重新导出核心类型
pub use appender::LogAppender;
pub use formatter::LogFormatter;
pub use level::LogLevel;
pub use logger::{Logger, LoggerConfig};
pub use record::{LogRecord, MetadataValue};

// 重新导出注册函数
pub use registry::{create_logger_from_config, register_log_components};

// 重新导出子模块的注册函数
pub use appender::{register_appenders, ConsoleAppender, ConsoleAppenderConfig, FileAppender, FileAppenderConfig};
pub use formatter::{register_formatters, JsonFormatter, JsonFormatterConfig, TextFormatter, TextFormatterConfig};
