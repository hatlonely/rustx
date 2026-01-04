use std::future::Future;
use std::time::Duration;
use thiserror::Error;

/// KV 存储相关错误类型（对应 Golang 版本的错误）
#[derive(Error, Debug)]
pub enum KvError {
    #[error("Key not found")]
    KeyNotFound,
    #[error("Condition failed")]
    ConditionFailed,
    #[error("Other error: {0}")]
    Other(String),
}

/// 设置选项（对应 Golang 版本的 setOptions）
#[derive(Default, Clone)]
pub struct SetOptions {
    /// 过期时间
    pub expiration: Option<Duration>,
    /// 仅在键不存在时设置
    pub if_not_exist: bool,
}

impl SetOptions {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 设置过期时间（对应 WithExpiration）
    pub fn with_expiration(mut self, expiration: Duration) -> Self {
        self.expiration = Some(expiration);
        self
    }
    
    /// 仅在键不存在时设置（对应 WithIfNotExist）
    pub fn with_if_not_exist(mut self) -> Self {
        self.if_not_exist = true;
        self
    }
}

/// 核心 KV 存储 trait（严格对应 Golang Store[K, V] interface）
pub trait Store<K, V>: Send + Sync
where
    K: Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// 设置键值对，WithIfNotExist 时键存在则返回 ErrConditionFailed
    fn set(&self, key: K, value: V, options: SetOptions) -> impl Future<Output = Result<(), KvError>> + Send;
    
    /// 获取键对应的值，键不存在时返回 ErrKeyNotFound
    fn get(&self, key: K) -> impl Future<Output = Result<V, KvError>> + Send;
    
    /// 删除键，键不存在时也返回成功
    fn del(&self, key: K) -> impl Future<Output = Result<(), KvError>> + Send;
    
    /// 批量设置，返回每个键的操作结果
    fn batch_set(&self, keys: Vec<K>, vals: Vec<V>, options: SetOptions) -> impl Future<Output = Result<Vec<Result<(), KvError>>, KvError>> + Send;
    
    /// 批量获取，返回每个键的值和错误
    fn batch_get(&self, keys: Vec<K>) -> impl Future<Output = Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError>> + Send;
    
    /// 批量删除，返回每个键的操作结果
    fn batch_del(&self, keys: Vec<K>) -> impl Future<Output = Result<Vec<Result<(), KvError>>, KvError>> + Send;
    
    /// 关闭存储
    fn close(&self) -> impl Future<Output = Result<(), KvError>> + Send;
}