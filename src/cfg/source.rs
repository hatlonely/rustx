//! 配置源抽象
//!
//! 提供统一的配置来源接口，支持文件、数据库、配置中心等多种来源

use anyhow::Result;
use crossbeam::channel;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use std::thread::JoinHandle;

/// 配置值包装，提供类型转换能力
///
/// 用于统一 load 和 watch 的返回类型，支持灵活的类型转换
///
/// # 示例
/// ```no_run
/// use rustx::cfg::{ConfigSource, FileSource, FileSourceConfig, ConfigValue};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct DatabaseConfig {
///     host: String,
///     port: u16,
/// }
///
/// let source = FileSource::new(FileSourceConfig {
///     base_path: "config".to_string(),
/// });
///
/// // 直接转换为具体类型
/// let config: DatabaseConfig = source.load("database").unwrap().into_type().unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct ConfigValue(pub JsonValue);

impl ConfigValue {
    /// 创建新的 ConfigValue
    pub fn new(value: JsonValue) -> Self {
        Self(value)
    }

    /// 转换为指定类型（消费 self）
    pub fn into_type<T: DeserializeOwned>(self) -> Result<T> {
        Ok(serde_json::from_value(self.0)?)
    }

    /// 引用方式转换为指定类型
    pub fn as_type<T: DeserializeOwned>(&self) -> Result<T> {
        Ok(serde_json::from_value(self.0.clone())?)
    }

    /// 获取内部的 JsonValue 引用
    pub fn as_value(&self) -> &JsonValue {
        &self.0
    }

    /// 获取内部的 JsonValue（消费 self）
    pub fn into_value(self) -> JsonValue {
        self.0
    }
}

/// 配置变更事件
#[derive(Debug, Clone)]
pub enum ConfigChange {
    /// 配置更新
    Updated(ConfigValue),
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
    /// - 成功返回 ConfigValue，可通过 into_type() 转换为具体类型
    /// - 失败返回错误信息
    fn load(&self, key: &str) -> Result<ConfigValue>;

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
    /// use rustx::cfg::{ConfigSource, FileSource, FileSourceConfig, ConfigChange};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize, Debug)]
    /// struct DatabaseConfig {
    ///     host: String,
    ///     port: u16,
    /// }
    ///
    /// let source = FileSource::new(FileSourceConfig {
    ///     base_path: "config".to_string(),
    /// });
    /// source.watch("database", |change| {
    ///     match change {
    ///         ConfigChange::Updated(value) => {
    ///             match value.into_type::<DatabaseConfig>() {
    ///                 Ok(config) => println!("配置已更新: {:?}", config),
    ///                 Err(e) => eprintln!("解析失败: {}", e),
    ///             }
    ///         }
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
