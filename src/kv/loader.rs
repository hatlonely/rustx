use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use thiserror::Error;

use super::parser::ChangeType;

/// 加载策略（对应 Golang 常量）
pub const LOAD_STRATEGY_REPLACE: &str = "replace";
pub const LOAD_STRATEGY_INPLACE: &str = "inplace";

/// 加载器相关错误
#[derive(Error, Debug)]
pub enum LoaderError {
    #[error("Load failed: {0}")]
    LoadFailed(String),
}

/// KV 数据流：用于遍历 KV 数据（对应 Golang KVStream[K, V] interface）
pub trait KvStream<K, V>: Send + Sync {
    /// 遍历数据流中的每个元素（对应 Golang Each 方法）
    fn each(&self, callback: Box<dyn Fn(ChangeType, K, V) -> Result<(), LoaderError> + Send + Sync>) -> Pin<Box<dyn Future<Output = Result<(), LoaderError>> + Send + '_>>;
}

/// 监听器：处理 KV 数据变更的回调（对应 Golang Listener[K, V]）
pub type Listener<K, V> = Box<dyn Fn(Arc<dyn KvStream<K, V>>) -> Pin<Box<dyn Future<Output = Result<(), LoaderError>> + Send>> + Send + Sync>;

/// 核心加载器 trait（对应 Golang Loader[K, V] interface）
pub trait Loader<K, V>: Send + Sync {
    /// 注册数据变更监听器（对应 Golang OnChange）
    fn on_change(&mut self, listener: Listener<K, V>) -> Pin<Box<dyn Future<Output = Result<(), LoaderError>> + Send + '_>>;
    
    /// 关闭加载器（对应 Golang Close）
    fn close(&mut self) -> Pin<Box<dyn Future<Output = Result<(), LoaderError>> + Send + '_>>;
}


