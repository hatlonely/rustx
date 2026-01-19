use anyhow::Result;
use crate::cfg::{register_trait, TypeOptions, create_trait_from_type_options};
use crate::log::appender::LogAppender;
use crate::log::appender::{
    console_appender::{ConsoleAppender, ConsoleAppenderConfig},
    file_appender::{FileAppender, FileAppenderConfig},
};

/// 注册所有 Appender 实现
pub fn register_appenders() -> Result<()> {
    register_trait::<ConsoleAppender, dyn LogAppender, ConsoleAppenderConfig>("ConsoleAppender")?;
    register_trait::<FileAppender, dyn LogAppender, FileAppenderConfig>("FileAppender")?;
    Ok(())
}

/// 从 TypeOptions 创建 Appender
pub fn create_appender_from_options(options: &TypeOptions) -> Result<Box<dyn LogAppender>> {
    create_trait_from_type_options(options)
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let appender = create_appender_from_options(&opts)?;
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

        let appender = create_appender_from_options(&opts)?;
        // 验证能够成功创建 appender
        assert!(appender.append("test message").await.is_ok());

        Ok(())
    }
}
