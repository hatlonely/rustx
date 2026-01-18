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
///     let logger = create_logger_from_config(config).await?;
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
        $logger.log(
            crate::log::LogRecord::new(crate::log::LogLevel::Info, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        $logger.log(
            crate::log::LogRecord::new(crate::log::LogLevel::Info, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await
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
        $logger.log(
            crate::log::LogRecord::new(crate::log::LogLevel::Debug, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        $logger.log(
            crate::log::LogRecord::new(crate::log::LogLevel::Debug, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await
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
        $logger.log(
            crate::log::LogRecord::new(crate::log::LogLevel::Warn, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        $logger.log(
            crate::log::LogRecord::new(crate::log::LogLevel::Warn, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await
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
        $logger.log(
            crate::log::LogRecord::new(crate::log::LogLevel::Error, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        $logger.log(
            crate::log::LogRecord::new(crate::log::LogLevel::Error, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await
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
        $logger.log(
            crate::log::LogRecord::new(crate::log::LogLevel::Trace, $msg.into())
                .with_location(file!().to_string(), line!())
        ).await
    };
    ($logger:expr, $msg:expr, $($key:expr => $value:expr),* $(,)?) => {
        $logger.log(
            crate::log::LogRecord::new(crate::log::LogLevel::Trace, $msg.into())
                .with_location(file!().to_string(), line!())
                $(.with_metadata($key, $value))*
        ).await
    };
}

#[cfg(test)]
mod tests {
    // 宏的测试需要实际的 Logger 实例
    // 测试应该在 logger.rs 的集成测试中进行
}
