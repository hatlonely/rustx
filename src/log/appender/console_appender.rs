use crate::log::appender::LogAppender;
use anyhow::Result;
use serde::Deserialize;
use std::io::{self, Write};

/// ConsoleAppender 配置
#[derive(Debug, Clone, Deserialize)]
pub struct ConsoleAppenderConfig {
    /// 是否使用颜色（预留功能）
    #[serde(default)]
    pub use_colors: bool,
}

impl Default for ConsoleAppenderConfig {
    fn default() -> Self {
        Self { use_colors: true }
    }
}

/// 终端输出器
///
/// 将日志输出到标准输出
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
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", formatted_message)?;
        stdout.flush()?;
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
        let config = ConsoleAppenderConfig { use_colors: false };
        let appender = ConsoleAppender::new(config);

        let result = appender.flush().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_console_appender_from_config() {
        let config = ConsoleAppenderConfig { use_colors: false };

        let appender = ConsoleAppender::from(config);
        assert_eq!(appender.config.use_colors, false);
    }
}
