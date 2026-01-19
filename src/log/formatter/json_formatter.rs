use crate::log::formatter::LogFormatter;
use crate::log::record::LogRecord;
use anyhow::Result;
use serde::Deserialize;
use smart_default::SmartDefault;

/// JsonFormatter 配置（保留扩展性）
#[derive(Debug, Clone, Deserialize, PartialEq, SmartDefault)]
#[serde(default)]
pub struct JsonFormatterConfig {}

/// JSON 格式化器
///
/// 将日志记录格式化为 JSON 格式
pub struct JsonFormatter {}

impl JsonFormatter {
    pub fn new(_: JsonFormatterConfig) -> Self {
        Self {  }
    }
}

impl LogFormatter for JsonFormatter {
    fn format(&self, record: &LogRecord) -> Result<String> {
        // 直接序列化 LogRecord，复用其 Serialize 实现
        Ok(serde_json::to_string(record)?)
    }
}

crate::impl_from!(JsonFormatterConfig => JsonFormatter);
crate::impl_box_from!(JsonFormatter => dyn LogFormatter);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::level::LogLevel;

    #[test]
    fn test_json_formatter_format() {
        let config = JsonFormatterConfig::default();
        let formatter = JsonFormatter::new(config);

        let record = LogRecord::new(
            LogLevel::Info,
            "test message".to_string(),
        );

        let formatted = formatter.format(&record).unwrap();
        println!("{}", formatted);

        // 验证是有效的 JSON
        let value: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(value["level"], "INFO");
        assert_eq!(value["message"], "test message");
        assert!(value["timestamp"].is_number());
        assert_eq!(value["metadata"], serde_json::Value::Null);
    }

    #[test]
    fn test_json_formatter_with_location() {
        let config = JsonFormatterConfig::default();
        let formatter = JsonFormatter::new(config);

        let record = LogRecord::new(
            LogLevel::Error,
            "error message".to_string(),
        )
        .with_location("file.rs".to_string(), 42);

        let formatted = formatter.format(&record).unwrap();
        let value: serde_json::Value = serde_json::from_str(&formatted).unwrap();

        assert_eq!(value["file"], "file.rs");
        assert_eq!(value["line"], 42);
    }

    #[test]
    fn test_json_formatter_with_module() {
        let config = JsonFormatterConfig::default();
        let formatter = JsonFormatter::new(config);

        let record = LogRecord::new(
            LogLevel::Debug,
            "debug message".to_string(),
        )
        .with_module("my_module".to_string());

        let formatted = formatter.format(&record).unwrap();
        let value: serde_json::Value = serde_json::from_str(&formatted).unwrap();

        assert_eq!(value["module"], "my_module");
    }

    #[test]
    fn test_json_formatter_with_metadata() {
        let config = JsonFormatterConfig::default();
        let formatter = JsonFormatter::new(config);

        let record = LogRecord::new(
            LogLevel::Info,
            "user logged in".to_string(),
        )
        .with_metadata("user_id", 12345)
        .with_metadata("username", "alice")
        .with_metadata("success", true);

        let formatted = formatter.format(&record).unwrap();
        println!("{}", formatted);

        let value: serde_json::Value = serde_json::from_str(&formatted).unwrap();

        // 验证 metadata 字段存在
        assert!(value["metadata"].is_object());
        assert_eq!(value["metadata"]["user_id"], 12345);
        assert_eq!(value["metadata"]["username"], "alice");
        assert_eq!(value["metadata"]["success"], true);
    }

    #[test]
    fn test_json_formatter_from_config() {
        let config = JsonFormatterConfig::default();
        let _ = JsonFormatter::from(config);
    }
}
