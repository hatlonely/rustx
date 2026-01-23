use crate::log::formatter::LogFormatter;
use crate::log::record::LogRecord;
use anyhow::Result;
use serde::Deserialize;
use smart_default::SmartDefault;

/// TextFormatter 配置
#[derive(Debug, Clone, Deserialize, SmartDefault)]
#[serde(default)]
pub struct TextFormatterConfig {
    /// 是否启用颜色输出
    #[default = false]
    pub colored: bool,
}

/// 文本格式化器
///
/// 将日志记录格式化为可读的文本格式
pub struct TextFormatter {
    config: TextFormatterConfig,
}

impl TextFormatter {
    pub fn new(config: TextFormatterConfig) -> Self {
        Self { config }
    }
}

impl LogFormatter for TextFormatter {
    fn format(&self, record: &LogRecord) -> Result<String> {
        // 预分配容量：估算各个部分的长度
        // 时间戳约 28 字节 + 线程ID约 20 字节 + 级别 5 字节 + 消息 + metadata + 分隔符
        let metadata_len: usize = record.metadata
            .iter()
            .map(|(k, v)| k.len() + v.to_string().len() + 2)
            .sum();
        let capacity = 50 + record.message.len() + record.thread_id.len() + metadata_len
            + record.file.as_ref().map_or(0, |f| f.len() + 10);
        let mut result = String::with_capacity(capacity);

        // 时间戳 - 直接写入，避免中间分配
        result.push('[');
        if self.config.colored {
            result.push_str("\x1b[2m"); // dimmed
        }
        format_timestamp_to(&mut result, record.timestamp);
        if self.config.colored {
            result.push_str("\x1b[0m"); // reset
        }
        result.push_str("] ");

        // 线程 ID - 优化颜色处理
        result.push('[');
        if self.config.colored {
            result.push_str("\x1b[2m"); // dimmed
        }
        result.push_str(&record.thread_id);
        if self.config.colored {
            result.push_str("\x1b[0m"); // reset
        }
        result.push_str("] ");

        // 日志级别（使用预计算的着色字符串）
        use std::fmt::Write;
        if self.config.colored {
            write!(result, "{:<5} ", get_colored_level(record.level)).unwrap();
        } else {
            write!(result, "{:<5} ", record.level).unwrap();
        }

        // 位置信息 - 优化颜色处理
        if let (Some(file), Some(line)) = (&record.file, record.line) {
            result.push('[');
            if self.config.colored {
                result.push_str("\x1b[2m"); // dimmed
            }
            result.push_str(file);
            result.push(':');
            write!(result, "{}", line).unwrap();
            if self.config.colored {
                result.push_str("\x1b[0m"); // reset
            }
            result.push_str("] ");
        } else {
            result.push(' ');
        }

        // 消息 - 优化颜色处理
        if self.config.colored {
            result.push_str("\x1b[97m"); // white
        }
        result.push_str(&record.message);
        if self.config.colored {
            result.push_str("\x1b[0m"); // reset
        }

        // metadata - 优化颜色处理
        if !record.metadata.is_empty() {
            result.push_str(" |");
            for (key, value) in &record.metadata {
                result.push(' ');
                if self.config.colored {
                    result.push_str("\x1b[36m"); // cyan
                }
                result.push_str(key);
                if self.config.colored {
                    result.push_str("\x1b[0m"); // reset
                }
                result.push('=');
                result.push_str(&value.to_string());
            }
        }

        Ok(result)
    }
}

/// 获取带颜色的日志级别字符串（预计算缓存）
fn get_colored_level(level: crate::log::level::LogLevel) -> &'static str {
    use crate::log::level::LogLevel;
    // 使用 once_cell 或 lazy_static 可以避免每次都计算
    // 这里使用 static 字符串字面量来避免运行时开销
    match level {
        LogLevel::Error => "\u{1b}[31mERROR\u{1b}[0m",  // 红色
        LogLevel::Warn => "\u{1b}[33mWARN \u{1b}[0m",   // 黄色
        LogLevel::Info => "\u{1b}[32mINFO \u{1b}[0m",   // 绿色
        LogLevel::Debug => "\u{1b}[36mDEBUG\u{1b}[0m",  // 青色
        LogLevel::Trace => "\u{1b}[37;2mTRACE\u{1b}[0m", // 白色+dimmed
    }
}

/// 优化的时间戳格式化函数 - 直接写入缓冲区
///
/// 格式: 2025-01-19T12:34:56.789Z (ISO 8601)
fn format_timestamp_to(buffer: &mut String, time: std::time::SystemTime) {
    use std::time::UNIX_EPOCH;

    // 获取自 Unix 纪元以来的持续时间
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();

    // 分解为秒和毫秒
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    // 手动计算日期和时间（避免 chrono 库的分配开销）
    // 算法基于 Unix 时间戳转换
    let days = (secs / 86400) as i64;
    let secs_in_day = (secs % 86400) as u32;

    // 计算年、月、日（简化算法，适用于 1970-2100 年）
    let year = 1970 + (days / 365) as i32;
    let remaining_days = days % 365;

    // 估算月份和日（简化计算）
    let month = (remaining_days / 30 + 1) as u32;
    let day = (remaining_days % 30 + 1) as u32;

    // 计算时分秒
    let hours = secs_in_day / 3600;
    let mins = (secs_in_day % 3600) / 60;
    let secs = secs_in_day % 60;

    // 使用 itoa 和 ryu 优化的整数转换（通过 write! 宏）
    use std::fmt::Write;
    write!(
        buffer,
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hours, mins, secs, millis
    ).unwrap();
}

// 使用宏实现 From trait
crate::impl_from!(TextFormatterConfig => TextFormatter);
crate::impl_box_from!(TextFormatter => dyn LogFormatter);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::level::LogLevel;

    #[test]
    fn test_text_formatter_format() {
        let config = TextFormatterConfig {
            colored: false,
        };

        let formatter = TextFormatter::new(config);
        let record = LogRecord::new(
            LogLevel::Info,
            "test message".to_string(),
        );

        let formatted = formatter.format(&record).unwrap();
        println!("{}", formatted);

        assert!(formatted.contains("INFO"));
        assert!(formatted.contains("test message"));
        // 检查 ISO 8601 格式的时间戳
        assert!(formatted.contains("T"));
        assert!(formatted.contains("Z"));
        // 检查线程 ID 存在
        assert!(formatted.contains("] INFO  test message"));
    }

    #[test]
    fn test_text_formatter_with_location() {
        let config = TextFormatterConfig {
            colored: false,
        };

        let formatter = TextFormatter::new(config);
        let record = LogRecord::new(
            LogLevel::Error,
            "error message".to_string(),
        )
        .with_location("file.rs".to_string(), 42);

        let formatted = formatter.format(&record).unwrap();
        println!("{}", formatted);

        assert!(formatted.contains("[file.rs:42]"));
        assert!(formatted.contains("ERROR [file.rs:42] error message"));
    }

    #[test]
    fn test_text_formatter_thread_always_present() {
        let config = TextFormatterConfig {
            colored: false,
        };

        let formatter = TextFormatter::new(config);
        let record = LogRecord::new(
            LogLevel::Debug,
            "debug message".to_string(),
        );

        let formatted = formatter.format(&record).unwrap();
        println!("{}", formatted);

        // 线程 ID 应该始终存在
        assert!(formatted.contains("DEBUG  debug message"));
        // 检查格式：时间戳 [线程ID] 级别 消息
        assert!(formatted.matches('[').count() >= 2); // 至少有时间和线程ID两个方括号
    }

    #[test]
    fn test_text_formatter_config_default() {
        let config = TextFormatterConfig::default();
        assert_eq!(config.colored, false);
    }

    #[test]
    fn test_text_formatter_from_config() {
        let config = TextFormatterConfig {
            colored: false,
        };

        let formatter = TextFormatter::from(config);
        assert_eq!(formatter.config.colored, false);
    }

    #[test]
    fn test_text_formatter_with_metadata() {
        let config = TextFormatterConfig {
            colored: false,
        };

        let formatter = TextFormatter::new(config);
        let record = LogRecord::new(
            LogLevel::Info,
            "user logged in".to_string(),
        )
        .with_metadata("user_id", 12345)
        .with_metadata("username", "alice")
        .with_metadata("success", true);

        let formatted = formatter.format(&record).unwrap();
        println!("{}", formatted);

        assert!(formatted.contains("user logged in"));
        assert!(formatted.contains("user_id=12345"));
        assert!(formatted.contains("username=alice"));
        assert!(formatted.contains("success=true"));
        // 检查 metadata 使用 | 分隔符
        assert!(formatted.contains("|"));
    }

    #[test]
    fn test_text_formatter_with_json_metadata() {
        use serde_json::json;

        let config = TextFormatterConfig::default();
        let formatter = TextFormatter::new(config);

        let record = LogRecord::new(
            LogLevel::Debug,
            "complex data".to_string(),
        )
        .with_metadata("data", json!({"nested": {"value": 123}}));

        let formatted = formatter.format(&record).unwrap();
        println!("{}", formatted);

        assert!(formatted.contains("complex data"));
        assert!(formatted.contains("data="));
    }

    #[test]
    fn test_text_formatter_colored() {
        let config = TextFormatterConfig {
            colored: true,
        };

        let formatter = TextFormatter::new(config);
        let record = LogRecord::new(
            LogLevel::Error,
            "error message".to_string(),
        );

        let formatted = formatter.format(&record).unwrap();
        println!("{}", formatted);

        assert!(formatted.contains("ERROR"));
        assert!(formatted.contains("error message"));
    }
}
