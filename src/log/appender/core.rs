use anyhow::Result;

/// 日志输出器 trait
///
/// 负责将格式化后的日志输出到目标介质
#[async_trait::async_trait]
pub trait LogAppender: Send + Sync {
    /// 输出日志
    async fn append(&self, formatted_message: &str) -> Result<()>;

    /// 刷新缓冲区（默认实现为空操作）
    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}
