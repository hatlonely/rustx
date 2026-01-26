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
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // 使用 json5::from_str 构建 LoggerConfig
//!     let config: LoggerConfig = json5::from_str(r#"
//!         {
//!             level: "info",
//!             formatter: {
//!                 type: "TextFormatter",
//!                 options: {
//!                     colored: false
//!                 }
//!             },
//!             appender: {
//!                 type: "ConsoleAppender",
//!                 options: {
//!                     target: "stdout",
//!                     auto_flush: true
//!                 }
//!             }
//!         }
//!     "#)?;
//!
//!     // 创建Logger（组件会自动注册）
//!     let logger = Logger::new(config)?;
//!
//!     // 使用Logger
//!     logger.info("Application started".to_string()).await?;
//!     logger.error("Connection failed".to_string()).await?;
//!
//!     Ok(())
//! }
//! ```

pub mod appender;
pub mod formatter;
pub mod global_logger_manager;
pub mod log_record;
pub mod logger;
pub mod logger_manager;
pub mod macros;

// 重新导出核心类型
pub use appender::LogAppender;
pub use formatter::LogFormatter;
pub use logger::{Logger, LoggerConfig, LoggerCreateConfig};
pub use logger_manager::{LoggerManager, LoggerManagerConfig};

pub use global_logger_manager::{
    add_logger,
    debug,
    debugm,
    error,
    errorm,
    get_default_logger,
    get_logger,
    global_logger_manager,
    info,
    infom,
    init_logger_manager,
    // 默认 logger 的便捷 log 方法
    log,
    logm,
    trace,
    tracem,
    warn,
    warnm,
};

// 重新导出宏（宏通过 #[macro_export] 在 crate root 定义，这里重新导入以方便使用）
// 注意：info, debug, warn, error, trace 宏需要从 crate root 导入
pub use log_record::{LogLevel, LogRecord, MetadataValue};

// 重新导出子模块的注册函数
pub use appender::{
    register_appenders, ConsoleAppender, ConsoleAppenderConfig, FileAppender, FileAppenderConfig,
    Target,
};
pub use formatter::{
    register_formatters, JsonFormatter, JsonFormatterConfig, TextFormatter, TextFormatterConfig,
};
