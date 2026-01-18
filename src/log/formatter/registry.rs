use anyhow::Result;
use crate::cfg::{register_trait, TypeOptions, create_trait_from_type_options};
use crate::log::formatter::LogFormatter;
use crate::log::formatter::{
    text_formatter::{TextFormatter, TextFormatterConfig},
    json_formatter::{JsonFormatter, JsonFormatterConfig},
};

/// 注册所有 Formatter 实现
pub fn register_formatters() -> Result<()> {
    register_trait::<TextFormatter, dyn LogFormatter, TextFormatterConfig>("TextFormatter")?;
    register_trait::<JsonFormatter, dyn LogFormatter, JsonFormatterConfig>("JsonFormatter")?;
    Ok(())
}

/// 从 TypeOptions 创建 Formatter
pub fn create_formatter_from_options(options: &TypeOptions) -> Result<Box<dyn LogFormatter>> {
    create_trait_from_type_options(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::{LogRecord, LogLevel};

    #[test]
    fn test_register_formatters() -> Result<()> {
        register_formatters()?;

        // 测试创建 TextFormatter
        let opts = TypeOptions::from_json(
            r#"
            {
                "type": "TextFormatter",
                "options": {
                    "colored": false
                }
            }
        "#,
        )?;

        let formatter = create_formatter_from_options(&opts)?;
        // 验证能够成功创建 formatter
        assert!(formatter.format(&LogRecord::new(LogLevel::Info, "msg".to_string())).is_ok());

        Ok(())
    }

    #[test]
    fn test_create_json_formatter() -> Result<()> {
        register_formatters()?;

        let opts = TypeOptions::from_json(
            r#"
            {
                "type": "JsonFormatter",
                "options": {}
            }
        "#,
        )?;

        let formatter = create_formatter_from_options(&opts)?;
        // 验证能够成功创建 formatter
        assert!(formatter.format(&LogRecord::new(LogLevel::Info, "msg".to_string())).is_ok());

        Ok(())
    }

    #[test]
    fn test_create_text_formatter_with_custom_config() -> Result<()> {
        register_formatters()?;

        let opts = TypeOptions::from_json(
            r#"
            {
                "type": "TextFormatter",
                "options": {
                    "colored": true
                }
            }
        "#,
        )?;

        let formatter = create_formatter_from_options(&opts)?;
        // 验证能够成功创建 formatter
        assert!(formatter.format(&LogRecord::new(LogLevel::Info, "msg".to_string())).is_ok());

        Ok(())
    }
}
