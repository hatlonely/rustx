//! 配置源抽象
//!
//! 提供统一的配置来源接口，支持文件、数据库、配置中心等多种来源

use anyhow::Result;
use crossbeam::channel;
use std::thread::JoinHandle;

use super::type_options::TypeOptions;

/// 配置变更事件
#[derive(Debug, Clone)]
pub enum ConfigChange {
    /// 配置更新
    Updated(TypeOptions),
    /// 配置删除
    Deleted,
    /// 监听错误
    Error(String),
}

/// 配置来源抽象
///
/// 所有配置源（文件、数据库、配置中心等）都实现此 trait
pub trait ConfigSource: Send + Sync {
    /// 加载配置
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回
    /// - 成功返回 TypeOptions
    /// - 失败返回错误信息
    fn load(&self, key: &str) -> Result<TypeOptions>;

    /// 监听配置变化
    ///
    /// # 参数
    /// - `key`: 配置键
    /// - `handler`: 配置变化时的回调函数
    ///
    /// # 生命周期
    /// - 监听在 Source drop 时自动停止
    /// - 用户无需管理监听生命周期
    ///
    /// # 示例
    /// ```no_run
    /// use rustx::cfg::{ConfigSource, FileSource, ConfigChange};
    ///
    /// let source = FileSource::new("config");
    /// source.watch("database", |change| {
    ///     match change {
    ///         ConfigChange::Updated(config) => println!("配置已更新"),
    ///         ConfigChange::Deleted => println!("配置已删除"),
    ///         ConfigChange::Error(msg) => eprintln!("错误: {}", msg),
    ///     }
    /// }).unwrap();
    /// ```
    fn watch<F>(&self, key: &str, handler: F) -> Result<()>
    where
        F: Fn(ConfigChange) + Send + 'static;
}

/// 监听句柄（内部使用，不对外暴露）
pub(crate) struct WatchHandle {
    pub(crate) stop_sender: Option<channel::Sender<()>>,
    pub(crate) thread_handle: Option<JoinHandle<()>>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        // 发送停止信号
        if let Some(sender) = self.stop_sender.take() {
            let _ = sender.send(());
        }

        // 等待线程结束
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}
