/// 日志宏模块
///
/// 提供自动捕获文件和行号信息的日志宏
///
/// # 示例
///
/// ```ignore
/// use rustx::log::*;
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     let logger = Logger::new(config)?;
///
///     // 简单日志
///     info!(logger, "application started");
///
///     // 带 metadata 的日志
///     info!(logger, "user logged in", "user_id" => 12345, "username" => "alice");
///
///     Ok(())
/// }
/// ```

/// 记录 INFO 级别日志
///
/// # 示例
///
/// ```ignore
/// info!(logger, "user logged in");
/// info!(logger, "user action", "user_id" => 12345, "action" => "login");
/// ```
#[macro_export]
macro_rules! info {
    ($logger:expr, $msg:expr) => {
        let _ = $logger.log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Info, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await;
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $logger.log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Info, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await;
    };
}

/// 记录 DEBUG 级别日志
///
/// # 示例
///
/// ```ignore
/// debug!(logger, "processing request");
/// debug!(logger, "processing", "endpoint" => "/api/users", "method" => "GET");
/// ```
#[macro_export]
macro_rules! debug {
    ($logger:expr, $msg:expr) => {
        let _ = $logger.log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Debug, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await;
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $logger.log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Debug, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await;
    };
}

/// 记录 WARN 级别日志
///
/// # 示例
///
/// ```ignore
/// warn!(logger, "high memory usage");
/// warn!(logger, "slow query", "duration_ms" => 1500, "threshold_ms" => 1000);
/// ```
#[macro_export]
macro_rules! warn {
    ($logger:expr, $msg:expr) => {
        let _ = $logger.log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Warn, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await;
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $logger.log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Warn, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await;
    };
}

/// 记录 ERROR 级别日志
///
/// # 示例
///
/// ```ignore
/// error!(logger, "database connection failed");
/// error!(logger, "query failed", "error_code" => "CONN001", "retry_count" => 3);
/// ```
#[macro_export]
macro_rules! error {
    ($logger:expr, $msg:expr) => {
        let _ = $logger.log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Error, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await;
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $logger.log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Error, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await;
    };
}

/// 记录 TRACE 级别日志
///
/// # 示例
///
/// ```ignore
/// trace!(logger, "entering function");
/// trace!(logger, "function call", "function" => "process_user", "user_id" => 12345);
/// ```
#[macro_export]
macro_rules! trace {
    ($logger:expr, $msg:expr) => {
        let _ = $logger.log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Trace, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await;
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $logger.log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Trace, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await;
    };
}

// ========== 同步版本的日志宏 ==========
///
/// 这些宏用于同步上下文中，不需要 .await
///
/// # 示例
///
/// ```ignore
/// use rustx::log::*;
///
/// fn main() -> Result<()> {
///     let logger = Logger::new(config)?;
///
///     // 简单日志（同步）
///     sinfo!(logger, "application started");
///
///     // 带 metadata 的日志（同步）
///     sinfo!(logger, "user logged in", "user_id" => 12345, "username" => "alice");
///
///     Ok(())
/// }
/// ```

/// 同步记录 INFO 级别日志
///
/// # 示例
///
/// ```ignore
/// sinfo!(logger, "user logged in");
/// sinfo!(logger, "user action", "user_id" => 12345, "action" => "login");
/// ```
#[macro_export]
macro_rules! sinfo {
    ($logger:expr, $msg:expr) => {
        let _ = $logger.log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Info, $msg.into())
                .with_location(file!().to_string(), line!())
        );
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $logger.log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Info, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        );
    };
}

/// 同步记录 DEBUG 级别日志
///
/// # 示例
///
/// ```ignore
/// sdebug!(logger, "processing request");
/// sdebug!(logger, "processing", "endpoint" => "/api/users", "method" => "GET");
/// ```
#[macro_export]
macro_rules! sdebug {
    ($logger:expr, $msg:expr) => {
        let _ = $logger.log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Debug, $msg.into())
                .with_location(file!().to_string(), line!())
        );
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $logger.log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Debug, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        );
    };
}

/// 同步记录 WARN 级别日志
///
/// # 示例
///
/// ```ignore
/// swarn!(logger, "high memory usage");
/// swarn!(logger, "slow query", "duration_ms" => 1500, "threshold_ms" => 1000);
/// ```
#[macro_export]
macro_rules! swarn {
    ($logger:expr, $msg:expr) => {
        let _ = $logger.log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Warn, $msg.into())
                .with_location(file!().to_string(), line!())
        );
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $logger.log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Warn, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        );
    };
}

/// 同步记录 ERROR 级别日志
///
/// # 示例
///
/// ```ignore
/// serror!(logger, "database connection failed");
/// serror!(logger, "query failed", "error_code" => "CONN001", "retry_count" => 3);
/// ```
#[macro_export]
macro_rules! serror {
    ($logger:expr, $msg:expr) => {
        let _ = $logger.log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Error, $msg.into())
                .with_location(file!().to_string(), line!())
        );
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $logger.log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Error, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        );
    };
}

/// 同步记录 TRACE 级别日志
///
/// # 示例
///
/// ```ignore
/// strace!(logger, "entering function");
/// strace!(logger, "function call", "function" => "process_user", "user_id" => 12345);
/// ```
#[macro_export]
macro_rules! strace {
    ($logger:expr, $msg:expr) => {
        let _ = $logger.log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Trace, $msg.into())
                .with_location(file!().to_string(), line!())
        );
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $logger.log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Trace, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        );
    };
}

// ========== 全局默认 logger 的同步宏 ==========
///
/// 这些宏不需要传递 logger 参数，直接使用全局默认 logger（同步版本）
///
/// # 示例
///
/// ```ignore
/// use rustx::log::*;
///
/// fn main() -> Result<()> {
///     // 简单日志（同步）
///     sginfo!("application started");
///
///     // 带 metadata 的日志（同步）
///     sginfo!("user logged in", "user_id" => 12345, "username" => "alice");
///
///     Ok(())
/// }
/// ```

/// 使用全局默认 logger 同步记录 INFO 级别日志
///
/// # 示例
///
/// ```ignore
/// sginfo!("user logged in");
/// sginfo!("user action", "user_id" => 12345, "action" => "login");
/// ```
#[macro_export]
macro_rules! sginfo {
    ($msg:expr) => {
        let _ = $crate::log::log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Info, $msg.into())
                .with_location(file!().to_string(), line!())
        );
    };
    ($msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $crate::log::log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Info, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        );
    };
}

/// 使用全局默认 logger 同步记录 DEBUG 级别日志
///
/// # 示例
///
/// ```ignore
/// sgdebug!("processing request");
/// sgdebug!("processing", "endpoint" => "/api/users", "method" => "GET");
/// ```
#[macro_export]
macro_rules! sgdebug {
    ($msg:expr) => {
        let _ = $crate::log::log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Debug, $msg.into())
                .with_location(file!().to_string(), line!())
        );
    };
    ($msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $crate::log::log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Debug, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        );
    };
}

/// 使用全局默认 logger 同步记录 WARN 级别日志
///
/// # 示例
///
/// ```ignore
/// sgwarn!("high memory usage");
/// sgwarn!("slow query", "duration_ms" => 1500, "threshold_ms" => 1000);
/// ```
#[macro_export]
macro_rules! sgwarn {
    ($msg:expr) => {
        let _ = $crate::log::log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Warn, $msg.into())
                .with_location(file!().to_string(), line!())
        );
    };
    ($msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $crate::log::log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Warn, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        );
    };
}

/// 使用全局默认 logger 同步记录 ERROR 级别日志
///
/// # 示例
///
/// ```ignore
/// sgerror!("database connection failed");
/// sgerror!("query failed", "error_code" => "CONN001", "retry_count" => 3);
/// ```
#[macro_export]
macro_rules! sgerror {
    ($msg:expr) => {
        let _ = $crate::log::log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Error, $msg.into())
                .with_location(file!().to_string(), line!())
        );
    };
    ($msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $crate::log::log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Error, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        );
    };
}

/// 使用全局默认 logger 同步记录 TRACE 级别日志
///
/// # 示例
///
/// ```ignore
/// sgtrace!("entering function");
/// sgtrace!("function call", "function" => "process_user", "user_id" => 12345);
/// ```
#[macro_export]
macro_rules! sgtrace {
    ($msg:expr) => {
        let _ = $crate::log::log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Trace, $msg.into())
                .with_location(file!().to_string(), line!())
        );
    };
    ($msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $crate::log::log_sync(
            $crate::log::LogRecord::new($crate::log::LogLevel::Trace, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        );
    };
}

// ========== 全局默认 logger 的宏（异步）==========
///
/// ```ignore
/// ginfo!("user logged in");
/// ginfo!("user action", "user_id" => 12345, "action" => "login");
/// ```
#[macro_export]
macro_rules! ginfo {
    ($msg:expr) => {
        let _ = $crate::log::log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Info, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await;
    };
    ($msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $crate::log::log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Info, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await;
    };
}

/// 使用全局默认 logger 记录 DEBUG 级别日志
///
/// # 示例
///
/// ```ignore
/// gdebug!("processing request");
/// gdebug!("processing", "endpoint" => "/api/users", "method" => "GET");
/// ```
#[macro_export]
macro_rules! gdebug {
    ($msg:expr) => {
        let _ = $crate::log::log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Debug, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await;
    };
    ($msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $crate::log::log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Debug, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await;
    };
}

/// 使用全局默认 logger 记录 WARN 级别日志
///
/// # 示例
///
/// ```ignore
/// gwarn!("high memory usage");
/// gwarn!("slow query", "duration_ms" => 1500, "threshold_ms" => 1000);
/// ```
#[macro_export]
macro_rules! gwarn {
    ($msg:expr) => {
        let _ = $crate::log::log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Warn, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await;
    };
    ($msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $crate::log::log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Warn, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await;
    };
}

/// 使用全局默认 logger 记录 ERROR 级别日志
///
/// # 示例
///
/// ```ignore
/// gerror!("database connection failed");
/// gerror!("query failed", "error_code" => "CONN001", "retry_count" => 3);
/// ```
#[macro_export]
macro_rules! gerror {
    ($msg:expr) => {
        let _ = $crate::log::log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Error, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await;
    };
    ($msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $crate::log::log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Error, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await;
    };
}

/// 使用全局默认 logger 记录 TRACE 级别日志
///
/// # 示例
///
/// ```ignore
/// gtrace!("entering function");
/// gtrace!("function call", "function" => "process_user", "user_id" => 12345);
/// ```
#[macro_export]
macro_rules! gtrace {
    ($msg:expr) => {
        let _ = $crate::log::log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Trace, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await;
    };
    ($msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        let _ = $crate::log::log(
            $crate::log::LogRecord::new($crate::log::LogLevel::Trace, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await;
    };
}

#[cfg(test)]
mod tests {
    // 宏的测试需要实际的 Logger 实例
    // 测试应该在 logger.rs 的集成测试中进行
}

#[cfg(test)]
mod sync_tests {
    // 同步宏的测试
    use crate::log::logger::LoggerCreateConfig;

    fn create_test_logger() -> crate::log::Logger {
        let config_json = r#"{
            level: "debug",
            formatter: {
                type: "TextFormatter",
                options: {}
            },
            appender: {
                type: "ConsoleAppender",
                options: {}
            }
        }"#;

        let config: LoggerCreateConfig = json5::from_str(config_json).unwrap();
        crate::log::Logger::new(config).unwrap()
    }

    #[test]
    fn test_sync_macros() {
        let logger = create_test_logger();

        // 测试所有同步宏
        sinfo!(&logger, "sync info test");
        sdebug!(&logger, "sync debug test");
        swarn!(&logger, "sync warn test");
        serror!(&logger, "sync error test");
        strace!(&logger, "sync trace test");

        // 测试带 metadata 的同步宏
        sinfo!(&logger, "user action", "user_id" => 12345, "action" => "login");
        sdebug!(&logger, "processing", "endpoint" => "/api/users", "method" => "GET");
        swarn!(&logger, "slow query", "duration_ms" => 1500);
        serror!(&logger, "db error", "code" => "CONN001");
        strace!(&logger, "function call", "function" => "process");
    }

    #[test]
    fn test_global_sync_macros() {
        // 测试全局同步宏（使用默认 logger）
        sginfo!("global sync info test");
        sgdebug!("global sync debug test");
        sgwarn!("global sync warn test");
        sgerror!("global sync error test");
        sgtrace!("global sync trace test");

        // 测试带 metadata 的全局同步宏
        sginfo!("global user action", "user_id" => 12345, "action" => "login");
        sgdebug!("global processing", "endpoint" => "/api/users", "method" => "GET");
        sgwarn!("global slow query", "duration_ms" => 1500);
        sgerror!("global db error", "code" => "CONN001");
        sgtrace!("global function call", "function" => "process");
    }
}
