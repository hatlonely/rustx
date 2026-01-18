use anyhow::Result;
use crate::log::{Logger, LoggerConfig};

/// 从 LoggerConfig 创建 Logger
///
/// 这是一个辅助函数，用于从配置创建 Logger
pub fn create_logger_from_config(config: LoggerConfig) -> Result<Logger> {
    Logger::new(config)
}

/// 注册所有日志组件
///
/// 包括 formatter 和 appender
pub fn register_log_components() -> Result<()> {
    crate::log::register_formatters()?;
    crate::log::register_appenders()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::*;
    use crate::cfg::TypeOptions;

    #[tokio::test]
    async fn test_create_logger_from_config() -> Result<()> {
        register_log_components()?;

        let config = LoggerConfig {
            level: "debug".to_string(),
            formatter: TypeOptions::from_json(
                r#"
                {
                    "type": "TextFormatter",
                    "options": {
                        "colored": false
                    }
                }
            "#,
            )?,
            appender: TypeOptions::from_json(
                r#"
                {
                    "type": "ConsoleAppender",
                    "options": {
                        "use_colors": true
                    }
                }
            "#,
            )?,
        };

        let logger = create_logger_from_config(config)?;

        logger
            .info("Logger created successfully".to_string())
            .await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_create_logger_with_file_appender() -> Result<()> {
        register_log_components()?;

        let temp_file = tempfile::NamedTempFile::new()?;

        let config = LoggerConfig {
            level: "info".to_string(),
            formatter: TypeOptions::from_json(
                r#"
                {
                    "type": "JsonFormatter",
                    "options": {}
                }
            "#,
            )?,
            appender: TypeOptions::from_json(&format!(
                r#"
                {{
                    "type": "FileAppender",
                    "options": {{
                        "file_path": "{}"
                    }}
                }}
            "#,
                temp_file.path().display()
            ))?,
        };

        let logger = create_logger_from_config(config)?;

        logger
            .info("Test message".to_string())
            .await?;

        // 验证文件内容
        let contents = tokio::fs::read_to_string(temp_file.path()).await?;
        assert!(contents.contains("Test message"));

        Ok(())
    }

    #[tokio::test]
    async fn test_logger_with_json_formatter() -> Result<()> {
        register_log_components()?;

        let config = LoggerConfig {
            level: "error".to_string(),
            formatter: TypeOptions::from_json(
                r#"
                {
                    "type": "JsonFormatter",
                    "options": {}
                }
            "#,
            )?,
            appender: TypeOptions::from_json(
                r#"
                {
                    "type": "ConsoleAppender",
                    "options": {}
                }
            "#,
            )?,
        };

        let logger = create_logger_from_config(config)?;

        logger
            .error("Error occurred".to_string())
            .await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_register_log_components() -> Result<()> {
        let result = register_log_components();
        assert!(result.is_ok());
        Ok(())
    }
}
