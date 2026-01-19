use crate::log::appender::LogAppender;
use anyhow::Result;
use serde::Deserialize;
use smart_default::SmartDefault;
use std::io::{self, Write};

/// 输出目标
#[derive(Debug, Clone, Deserialize, PartialEq, SmartDefault)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// 标准输出
    #[default]
    Stdout,
    /// 标准错误
    Stderr,
}

/// ConsoleAppender 配置
#[derive(Debug, Clone, Deserialize, SmartDefault)]
#[serde(default)]
pub struct ConsoleAppenderConfig {
    /// 输出目标: "stdout" 或 "stderr"
    pub target: Target,

    /// 是否自动刷新缓冲区
    #[default = true]
    pub auto_flush: bool,
}

/// 终端输出器
///
/// 将日志输出到标准输出或标准错误
pub struct ConsoleAppender {
    config: ConsoleAppenderConfig,
}

impl ConsoleAppender {
    pub fn new(config: ConsoleAppenderConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl LogAppender for ConsoleAppender {
    async fn append(&self, formatted_message: &str) -> Result<()> {
        match self.config.target {
            Target::Stdout => {
                let mut stdout = io::stdout().lock();
                writeln!(stdout, "{}", formatted_message)?;
                if self.config.auto_flush {
                    stdout.flush()?;
                }
            }
            Target::Stderr => {
                let mut stderr = io::stderr().lock();
                writeln!(stderr, "{}", formatted_message)?;
                if self.config.auto_flush {
                    stderr.flush()?;
                }
            }
        }
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        match self.config.target {
            Target::Stdout => io::stdout().flush()?,
            Target::Stderr => io::stderr().flush()?,
        }
        Ok(())
    }
}

crate::impl_from!(ConsoleAppenderConfig => ConsoleAppender);
crate::impl_box_from!(ConsoleAppender => dyn LogAppender);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_console_appender_append() {
        let config = ConsoleAppenderConfig::default();
        let appender = ConsoleAppender::new(config);

        let result = appender.append("Test message").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_console_appender_flush() {
        let config = ConsoleAppenderConfig::default();
        let appender = ConsoleAppender::new(config);

        let result = appender.flush().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_console_appender_from_config() {
        let config = ConsoleAppenderConfig::default();
        let _appender = ConsoleAppender::from(config);
        // 验证能够正确创建
    }

    #[test]
    fn test_default_config() {
        let config = ConsoleAppenderConfig::default();
        assert_eq!(config.target, Target::Stdout);
        assert_eq!(config.auto_flush, true);
    }

    #[tokio::test]
    async fn test_stderr_appender() {
        let config = ConsoleAppenderConfig {
            target: Target::Stderr,
            auto_flush: true,
        };
        let appender = ConsoleAppender::new(config);

        let result = appender.append("Error message").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_no_auto_flush() {
        let config = ConsoleAppenderConfig {
            target: Target::Stdout,
            auto_flush: false,
        };
        let appender = ConsoleAppender::new(config);

        let result = appender.append("Message without flush").await;
        assert!(result.is_ok());

        // 手动刷新
        let flush_result = appender.flush().await;
        assert!(flush_result.is_ok());
    }
}
