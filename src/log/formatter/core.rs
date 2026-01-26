use crate::log::log_record::LogRecord;
use anyhow::Result;

/// 日志格式化器 trait
///
/// 负责将 LogRecord 格式化为字符串
pub trait LogFormatter: Send + Sync {
    /// 格式化日志记录
    fn format(&self, record: &LogRecord) -> Result<String>;
}
