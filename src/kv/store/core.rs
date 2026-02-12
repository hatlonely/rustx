use async_trait::async_trait;
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

/// 标记 trait：标识同步存储类型
///
/// 只有实现此 trait 的 SyncStore 才会自动获得 Store trait 的异步包装
/// 这样可以避免与 RedisStore 等远程存储实现产生冲突
pub trait IsSyncStore {}

/// 标记 trait：标识异步存储类型
///
/// 只有实现此 trait 的 Store 才会自动获得 SyncStore trait 的同步包装
/// 这样可以避免与内存存储实现产生冲突
pub trait IsAsyncStore {}

/// 同步 KV 存储接口
///
/// 内存存储实现只需实现此 trait，会自动获得异步能力
pub trait SyncStore<K, V>: Send + Sync
where
    K: Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// 设置键值对，WithIfNotExist 时键存在则返回 ErrConditionFailed
    fn set_sync(&self, key: &K, value: &V, options: &SetOptions) -> Result<(), KvError>;

    /// 获取键对应的值，键不存在时返回 ErrKeyNotFound
    fn get_sync(&self, key: &K) -> Result<V, KvError>;

    /// 删除键，键不存在时也返回成功
    fn del_sync(&self, key: &K) -> Result<(), KvError>;

    /// 批量设置，返回每个键的操作结果
    fn batch_set_sync(
        &self,
        keys: &[K],
        vals: &[V],
        options: &SetOptions,
    ) -> Result<Vec<Result<(), KvError>>, KvError>;

    /// 批量获取，返回每个键的值和错误
    fn batch_get_sync(
        &self,
        keys: &[K],
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError>;

    /// 批量删除，返回每个键的操作结果
    fn batch_del_sync(&self, keys: &[K]) -> Result<Vec<Result<(), KvError>>, KvError>;

    /// 关闭存储
    fn close_sync(&self) -> Result<(), KvError>;
}

/// 异步 KV 存储接口（原有接口，保持不变）
///
/// 用于远程存储实现（如 Redis、云存储等）
/// 同时为所有 SyncStore 实现者自动提供异步包装
#[async_trait]
pub trait Store<K, V>: Send + Sync
where
    K: Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// 设置键值对，WithIfNotExist 时键存在则返回 ErrConditionFailed
    async fn set(&self, key: &K, value: &V, options: &SetOptions) -> Result<(), KvError>;

    /// 获取键对应的值，键不存在时返回 ErrKeyNotFound
    async fn get(&self, key: &K) -> Result<V, KvError>;

    /// 删除键，键不存在时也返回成功
    async fn del(&self, key: &K) -> Result<(), KvError>;

    /// 批量设置，返回每个键的操作结果
    async fn batch_set(
        &self,
        keys: &[K],
        vals: &[V],
        options: &SetOptions,
    ) -> Result<Vec<Result<(), KvError>>, KvError>;

    /// 批量获取，返回每个键的值和错误
    async fn batch_get(
        &self,
        keys: &[K],
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError>;

    /// 批量删除，返回每个键的操作结果
    async fn batch_del(&self, keys: &[K]) -> Result<Vec<Result<(), KvError>>, KvError>;

    /// 关闭存储
    async fn close(&self) -> Result<(), KvError>;
}

/// 为所有同步存储（SyncStore + IsSyncStore）自动提供 Store trait 的异步包装
///
/// 这样同步存储只需实现 SyncStore 和 IsSyncStore，就会自动获得异步能力
/// 而不会与 RedisStore 等远程存储产生冲突
#[async_trait]
impl<K, V, T> Store<K, V> for T
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    T: SyncStore<K, V> + IsSyncStore,
{
    async fn set(&self, key: &K, value: &V, options: &SetOptions) -> Result<(), KvError> {
        self.set_sync(key, value, options)
    }

    async fn get(&self, key: &K) -> Result<V, KvError> {
        self.get_sync(key)
    }

    async fn del(&self, key: &K) -> Result<(), KvError> {
        self.del_sync(key)
    }

    async fn batch_set(
        &self,
        keys: &[K],
        vals: &[V],
        options: &SetOptions,
    ) -> Result<Vec<Result<(), KvError>>, KvError> {
        self.batch_set_sync(keys, vals, options)
    }

    async fn batch_get(
        &self,
        keys: &[K],
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError> {
        self.batch_get_sync(keys)
    }

    async fn batch_del(&self, keys: &[K]) -> Result<Vec<Result<(), KvError>>, KvError> {
        self.batch_del_sync(keys)
    }

    async fn close(&self) -> Result<(), KvError> {
        self.close_sync()
    }
}

/// 为所有异步存储（Store + IsAsyncStore）自动提供 SyncStore trait 的同步包装
///
/// 这样异步存储只需实现 Store 和 IsAsyncStore，就会自动获得同步能力
/// 而不会与 DashMapStore 等内存存储产生冲突
impl<K, V, T> SyncStore<K, V> for T
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    T: Store<K, V> + IsAsyncStore,
{
    fn set_sync(&self, key: &K, value: &V, options: &SetOptions) -> Result<(), KvError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::try_current()
                .map_err(|e| KvError::Other(format!("no runtime: {}", e)))?
                .block_on(self.set(key, value, options))
        })
    }

    fn get_sync(&self, key: &K) -> Result<V, KvError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::try_current()
                .map_err(|e| KvError::Other(format!("no runtime: {}", e)))?
                .block_on(self.get(key))
        })
    }

    fn del_sync(&self, key: &K) -> Result<(), KvError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::try_current()
                .map_err(|e| KvError::Other(format!("no runtime: {}", e)))?
                .block_on(self.del(key))
        })
    }

    fn batch_set_sync(
        &self,
        keys: &[K],
        vals: &[V],
        options: &SetOptions,
    ) -> Result<Vec<Result<(), KvError>>, KvError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::try_current()
                .map_err(|e| KvError::Other(format!("no runtime: {}", e)))?
                .block_on(self.batch_set(keys, vals, options))
        })
    }

    fn batch_get_sync(
        &self,
        keys: &[K],
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::try_current()
                .map_err(|e| KvError::Other(format!("no runtime: {}", e)))?
                .block_on(self.batch_get(keys))
        })
    }

    fn batch_del_sync(&self, keys: &[K]) -> Result<Vec<Result<(), KvError>>, KvError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::try_current()
                .map_err(|e| KvError::Other(format!("no runtime: {}", e)))?
                .block_on(self.batch_del(keys))
        })
    }

    fn close_sync(&self) -> Result<(), KvError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::try_current()
                .map_err(|e| KvError::Other(format!("no runtime: {}", e)))?
                .block_on(self.close())
        })
    }
}