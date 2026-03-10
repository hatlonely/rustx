use anyhow::Result;
use std::sync::OnceLock;

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

    /// 同步输出日志
    ///
    /// 默认实现使用 blocking runtime 调用异步方法，
    /// 具体实现类应该提供更高效的同步版本
    fn append_sync(&self, formatted_message: &str) -> Result<()> {
        let rt = get_blocking_runtime();
        rt.block_on(self.append(formatted_message))
    }

    /// 同步刷新缓冲区
    ///
    /// 默认实现使用 blocking runtime 调用异步方法，
    /// 具体实现类应该提供更高效的同步版本
    fn flush_sync(&self) -> Result<()> {
        let rt = get_blocking_runtime();
        rt.block_on(self.flush())
    }
}

/// 获取全局 blocking runtime（用于同步调用异步方法）
fn get_blocking_runtime() -> &'static tokio::runtime::Runtime {
    static BLOCKING_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    BLOCKING_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create blocking runtime")
    })
}
