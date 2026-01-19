use anyhow::Result;
use crate::cfg::register_trait;
use crate::log::appender::LogAppender;
use crate::log::appender::{
    console_appender::{ConsoleAppender, ConsoleAppenderConfig},
    file_appender::{FileAppender, FileAppenderConfig},
    rolling_file_appender::{RollingFileAppender, RollingFileAppenderConfig},
};

/// 注册所有 Appender 实现
pub fn register_appenders() -> Result<()> {
    register_trait::<ConsoleAppender, dyn LogAppender, ConsoleAppenderConfig>("ConsoleAppender")?;
    register_trait::<FileAppender, dyn LogAppender, FileAppenderConfig>("FileAppender")?;
    register_trait::<RollingFileAppender, dyn LogAppender, RollingFileAppenderConfig>("RollingFileAppender")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{TypeOptions, create_trait_from_type_options};

    #[tokio::test]
    async fn test_register_appenders() -> Result<()> {
        register_appenders()?;

        // 测试创建 ConsoleAppender
        let opts = TypeOptions::from_json(
            r#"
            {
                "type": "ConsoleAppender",
                "options": {
                    "target": "stdout",
                    "auto_flush": true
                }
            }
        "#,
        )?;

        let appender: Box<dyn LogAppender> = create_trait_from_type_options(&opts)?;
        // 验证能够成功创建 appender
        assert!(appender.append("test message").await.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_create_file_appender() -> Result<()> {
        register_appenders()?;

        let temp_file = tempfile::NamedTempFile::new()?;
        let opts = TypeOptions::from_json(&format!(
            r#"
            {{
                "type": "FileAppender",
                "options": {{
                    "file_path": "{}"
                }}
            }}
        "#,
            temp_file.path().display()
        ))?;

        let appender: Box<dyn LogAppender> = create_trait_from_type_options(&opts)?;
        // 验证能够成功创建 appender
        assert!(appender.append("test message").await.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_create_rolling_file_appender() -> Result<()> {
        register_appenders()?;

        let temp_dir = tempfile::TempDir::new()?;
        let log_path = temp_dir.path().join("test.log");
        let opts = TypeOptions::from_json(&format!(
            r#"
            {{
                "type": "RollingFileAppender",
                "options": {{
                    "file_path": "{}",
                    "max_files": 5
                }}
            }}
        "#,
            log_path.display()
        ))?;

        let appender: Box<dyn LogAppender> = create_trait_from_type_options(&opts)?;
        // 验证能够成功创建 appender
        assert!(appender.append("test message").await.is_ok());

        Ok(())
    }
}
