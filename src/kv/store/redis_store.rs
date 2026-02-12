use async_trait::async_trait;
use redis::AsyncCommands;
use redis::{ExistenceCheck, SetExpiry, SetOptions as RedisSetOptions};
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::time::Duration;

use super::core::{IsAsyncStore, KvError, SetOptions, Store};
use crate::cfg::{create_trait_from_type_options, TypeOptions};
use crate::kv::serializer::Serializer;

/// Redis 连接错误
#[derive(thiserror::Error, Debug)]
pub enum RedisError {
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Redis 存储配置（简化版，与 Go 版本对齐）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, SmartDefault)]
#[serde(default)]
pub struct RedisStoreConfig {
    // ===== 连接配置 =====
    /// 单机模式：host:port 地址（如 "localhost:6379"）
    pub endpoint: Option<String>,

    /// 集群模式：节点地址列表（如 ["node1:6379", "node2:6379"]）
    pub endpoints: Option<Vec<String>>,

    // ===== 认证配置 =====
    /// Redis 6.0+ ACL 用户名
    pub username: Option<String>,

    /// 密码
    pub password: Option<String>,

    // ===== 数据库配置 =====
    /// 数据库编号（默认 0，仅单机模式有效）
    #[default = 0]
    pub db: i64,

    /// 默认 TTL（秒），0 表示不设置过期时间
    #[default = 0]
    pub default_ttl: u64,

    // ===== 超时配置 =====
    /// 连接超时（秒）
    #[default = 5]
    pub connection_timeout: u64,

    /// 命令执行超时（秒）
    #[default = 3]
    pub command_timeout: u64,

    // ===== 序列化器配置 =====
    /// 键序列化器配置（使用 TypeOptions 动态创建）
    /// 默认使用 "JsonSerializer"
    pub key_serializer: Option<TypeOptions>,

    /// 值序列化器配置（使用 TypeOptions 动态创建）
    /// 默认使用 "JsonSerializer"
    pub val_serializer: Option<TypeOptions>,
}

/// Redis 存储实现
///
/// # 类型参数
/// - `K`: 键类型，必须实现 Clone + Send + Sync
/// - `V`: 值类型，必须实现 Clone + Send + Sync
///
/// # 示例
/// ```ignore
/// use rustx::kv::store::{RedisStore, RedisStoreConfig, Store, SetOptions};
///
/// // 创建配置
/// let config = RedisStoreConfig {
///     endpoint: Some("localhost:6379".to_string()),
///     password: Some("secret".to_string()),
///     db: 0,
///     default_ttl: 3600,
///     ..Default::default()
/// };
///
/// // 创建存储实例
/// let store = RedisStore::<String, String>::new(config).unwrap();
///
/// // 使用存储
/// tokio::runtime::Runtime::new().unwrap().block_on(async {
///     store.set("key".to_string(), "value".to_string(), SetOptions::new()).await.unwrap();
///     let value = store.get("key".to_string()).await.unwrap();
///     assert_eq!(value, "value");
/// });
/// ```
pub struct RedisStore<K, V> {
    client: redis::Client,
    key_serializer: Box<dyn Serializer<K, Vec<u8>>>,
    val_serializer: Box<dyn Serializer<V, Vec<u8>>>,
    default_ttl: Duration,
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V> RedisStore<K, V>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// 创建 Redis 存储的唯一方法
    ///
    /// # 参数
    /// * `config` - Redis 存储配置
    ///
    /// # 返回
    /// 成功返回 RedisStore 实例，失败返回 RedisError
    pub fn new(config: RedisStoreConfig) -> Result<Self, RedisError> {
        // 1. 验证配置
        let is_cluster = config.endpoints.as_ref().is_some_and(|e| !e.is_empty());
        let is_single = config.endpoint.as_ref().is_some_and(|e| !e.is_empty());

        if !is_cluster && !is_single {
            return Err(RedisError::InvalidConfig(
                "Either endpoint or endpoints must be set".to_string(),
            ));
        }

        if is_cluster && is_single {
            return Err(RedisError::InvalidConfig(
                "Cannot set both endpoint and endpoints".to_string(),
            ));
        }

        // 2. 创建序列化器（默认使用 JsonSerializer）
        let key_serializer =
            Self::create_serializer::<K>(config.key_serializer.clone(), "JsonSerializer")?;
        let val_serializer =
            Self::create_serializer::<V>(config.val_serializer.clone(), "JsonSerializer")?;

        // 3. 构建 Redis 连接信息
        let client = if is_single {
            Self::create_single_client(&config)?
        } else {
            Self::create_cluster_client(&config)?
        };

        // 4. 测试连接（使用同步方式）
        let mut con = client
            .get_connection()
            .map_err(|e| RedisError::ConnectionFailed(e.to_string()))?;

        redis::cmd("PING")
            .query::<String>(&mut con)
            .map_err(|e| RedisError::ConnectionFailed(format!("PING failed: {}", e)))?;

        // 5. 转换 TTL
        let default_ttl = Duration::from_secs(config.default_ttl);

        Ok(Self {
            client,
            key_serializer,
            val_serializer,
            default_ttl,
            _phantom: std::marker::PhantomData,
        })
    }

    /// 创建单机模式客户端
    fn create_single_client(config: &RedisStoreConfig) -> Result<redis::Client, RedisError> {
        let endpoint = config.endpoint.as_ref().unwrap();

        // 构建 Redis 连接 URL
        let url = if let Some(password) = &config.password {
            let username = config.username.as_deref().unwrap_or("default");
            format!(
                "redis://{}:{}@{}/{}",
                username, password, endpoint, config.db
            )
        } else {
            format!("redis://{}/{}", endpoint, config.db)
        };

        redis::Client::open(url)
            .map_err(|e| RedisError::InvalidConfig(format!("Invalid connection URL: {}", e)))
    }

    /// 创建集群模式客户端
    fn create_cluster_client(config: &RedisStoreConfig) -> Result<redis::Client, RedisError> {
        let endpoints = config.endpoints.as_ref().unwrap();

        // 集群模式使用第一个节点作为入口
        let endpoint = &endpoints[0];

        let url = if let Some(password) = &config.password {
            let username = config.username.as_deref().unwrap_or("default");
            format!("redis://{}:{}@{}", username, password, endpoint)
        } else {
            format!("redis://{}", endpoint)
        };

        redis::Client::open(url)
            .map_err(|e| RedisError::InvalidConfig(format!("Invalid cluster URL: {}", e)))
    }

    /// 创建序列化器（支持动态配置）
    ///
    /// 注意：调用此方法前必须先注册相应的序列化器类型
    /// 例如：register_serde_serializers::<String>()?
    fn create_serializer<T>(
        type_options: Option<TypeOptions>,
        default_type: &str,
    ) -> Result<Box<dyn Serializer<T, Vec<u8>>>, RedisError>
    where
        T: Clone + Send + Sync + 'static,
    {
        match type_options {
            Some(opts) => create_trait_from_type_options(&opts).map_err(|e| {
                RedisError::InvalidConfig(format!("Failed to create serializer: {}", e))
            }),
            None => {
                // 使用默认序列化器
                let default_opts = TypeOptions {
                    type_name: default_type.to_string(),
                    options: serde_json::json!({}),
                };
                create_trait_from_type_options(&default_opts).map_err(|e| {
                    RedisError::InvalidConfig(format!(
                        "Failed to create default serializer ({}): {}. \
                         Make sure to register the serializer first using register_serde_serializers::<T>()?",
                        default_type, e
                    ))
                })
            }
        }
    }

    /// 获取默认 TTL
    pub fn default_ttl(&self) -> Duration {
        self.default_ttl
    }
}

// 实现 Store trait
#[async_trait]
impl<K, V> Store<K, V> for RedisStore<K, V>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    async fn set(&self, key: &K, value: &V, options: &SetOptions) -> Result<(), KvError> {
        // 1. 序列化键值
        let key_bytes = self
            .key_serializer
            .serialize(key.clone())
            .map_err(|e| KvError::Other(format!("Key serialization failed: {}", e)))?;

        let val_bytes = self
            .val_serializer
            .serialize(value.clone())
            .map_err(|e| KvError::Other(format!("Value serialization failed: {}", e)))?;

        // 2. 转换键为字符串（Redis 键必须是字符串）
        let key_str = String::from_utf8(key_bytes)
            .map_err(|e| KvError::Other(format!("Invalid key UTF-8: {}", e)))?;

        // 3. 构建 Redis SET 命令选项
        let mut redis_set_opts = RedisSetOptions::default();

        // 设置条件检查（NX）
        if options.if_not_exist {
            redis_set_opts = redis_set_opts.conditional_set(ExistenceCheck::NX);
        }

        // 设置过期时间
        let expiration = options.expiration.unwrap_or(self.default_ttl);
        if expiration > Duration::ZERO {
            redis_set_opts = redis_set_opts.with_expiration(SetExpiry::EX(expiration.as_secs()));
        }

        // 4. 执行 SET 命令
        let mut con = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| KvError::Other(format!("Failed to get connection: {}", e)))?;

        // 使用 set_options 命令，返回值是 Option<String>：
        // - Some(_) 表示设置成功
        // - None 表示设置失败（key 已存在且设置了 NX 选项）
        let result: Option<String> = con
            .set_options(&key_str, val_bytes.as_slice(), redis_set_opts)
            .await
            .map_err(|e| KvError::Other(format!("SET failed: {}", e)))?;

        // 如果使用了 NX 选项但 key 已存在，返回 ConditionFailed
        if options.if_not_exist && result.is_none() {
            return Err(KvError::ConditionFailed);
        }

        Ok(())
    }

    async fn get(&self, key: &K) -> Result<V, KvError> {
        // 1. 序列化键
        let key_bytes = self
            .key_serializer
            .serialize(key.clone())
            .map_err(|e| KvError::Other(format!("Key serialization failed: {}", e)))?;

        let key_str = String::from_utf8(key_bytes)
            .map_err(|e| KvError::Other(format!("Invalid key UTF-8: {}", e)))?;

        // 2. 执行 GET 命令
        let mut con = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| KvError::Other(format!("Failed to get connection: {}", e)))?;

        let result: Option<Vec<u8>> = con
            .get(&key_str)
            .await
            .map_err(|e| KvError::Other(format!("GET failed: {}", e)))?;

        match result {
            Some(val_bytes) => {
                // 3. 反序列化值
                self.val_serializer
                    .deserialize(val_bytes)
                    .map_err(|e| KvError::Other(format!("Value deserialization failed: {}", e)))
            }
            None => Err(KvError::KeyNotFound),
        }
    }

    async fn del(&self, key: &K) -> Result<(), KvError> {
        // 1. 序列化键
        let key_bytes = self
            .key_serializer
            .serialize(key.clone())
            .map_err(|e| KvError::Other(format!("Key serialization failed: {}", e)))?;

        let key_str = String::from_utf8(key_bytes)
            .map_err(|e| KvError::Other(format!("Invalid key UTF-8: {}", e)))?;

        // 2. 执行 DEL 命令
        let mut con = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| KvError::Other(format!("Failed to get connection: {}", e)))?;

        let _: () = con
            .del(&key_str)
            .await
            .map_err(|e| KvError::Other(format!("DEL failed: {}", e)))?;

        Ok(())
    }

    async fn batch_set(
        &self,
        keys: &[K],
        vals: &[V],
        options: &SetOptions,
    ) -> Result<Vec<Result<(), KvError>>, KvError> {
        if keys.len() != vals.len() {
            return Err(KvError::Other(
                "Keys and values length mismatch".to_string(),
            ));
        }

        // 1. 序列化所有键值对
        let mut items = Vec::with_capacity(keys.len());
        for (key, val) in keys.iter().zip(vals.iter()) {
            let key_bytes = self
                .key_serializer
                .serialize(key.clone())
                .map_err(|e| KvError::Other(format!("Key serialization failed: {}", e)))?;

            let val_bytes = self
                .val_serializer
                .serialize(val.clone())
                .map_err(|e| KvError::Other(format!("Value serialization failed: {}", e)))?;

            let key_str = String::from_utf8(key_bytes)
                .map_err(|e| KvError::Other(format!("Invalid key UTF-8: {}", e)))?;

            items.push((key_str, val_bytes));
        }

        // 2. 确定 TTL
        let expiration = options.expiration.unwrap_or(self.default_ttl);

        // 3. 构建 Redis SET 命令选项（所有键共享相同的选项）
        let mut redis_set_opts = RedisSetOptions::default();
        if options.if_not_exist {
            redis_set_opts = redis_set_opts.conditional_set(ExistenceCheck::NX);
        }
        if expiration > Duration::ZERO {
            redis_set_opts = redis_set_opts.with_expiration(SetExpiry::EX(expiration.as_secs()));
        }

        // 4. 使用 Pipeline 批量执行
        let mut con = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| KvError::Other(format!("Failed to get connection: {}", e)))?;

        let mut pipe = redis::pipe();
        // 注意：不使用 atomic()，因为我们需要每个命令独立执行并收集结果

        for (key_str, val_bytes) in &items {
            pipe.set_options(key_str, val_bytes.as_slice(), redis_set_opts.clone());
        }

        // 5. 执行 Pipeline 并收集返回值
        // set_options 返回 Option<String>，所以 pipeline 返回 Vec<Option<String>>
        let results: Vec<Option<String>> = pipe
            .query_async(&mut con)
            .await
            .map_err(|e| KvError::Other(format!("Pipeline SET failed: {}", e)))?;

        // 6. 将 Redis 返回值转换为 KvError 结果
        let final_results: Vec<Result<(), KvError>> = results
            .into_iter()
            .map(|result| {
                // 如果使用了 NX 选项且返回 None，表示 key 已存在
                if options.if_not_exist && result.is_none() {
                    Err(KvError::ConditionFailed)
                } else {
                    Ok(())
                }
            })
            .collect();

        Ok(final_results)
    }

    async fn batch_get(
        &self,
        keys: &[K],
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError> {
        if keys.is_empty() {
            return Ok((vec![], vec![]));
        }

        // 1. 序列化所有键
        let mut key_strs = Vec::with_capacity(keys.len());
        for key in keys {
            let key_bytes = self
                .key_serializer
                .serialize(key.clone())
                .map_err(|e| KvError::Other(format!("Key serialization failed: {}", e)))?;

            let key_str = String::from_utf8(key_bytes)
                .map_err(|e| KvError::Other(format!("Invalid key UTF-8: {}", e)))?;

            key_strs.push(key_str);
        }

        // 2. 使用 MGET 批量获取
        let mut con = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| KvError::Other(format!("Failed to get connection: {}", e)))?;

        let results: Option<Vec<Option<Vec<u8>>>> = con
            .mget(&key_strs)
            .await
            .map_err(|e| KvError::Other(format!("MGET failed: {}", e)))?;

        let results = results.unwrap_or_default();

        // 3. 反序列化结果
        let mut values = Vec::with_capacity(results.len());
        let mut errors = Vec::with_capacity(results.len());

        for result in results {
            match result {
                Some(val_bytes) => match self.val_serializer.deserialize(val_bytes) {
                    Ok(val) => {
                        values.push(Some(val));
                        errors.push(None);
                    }
                    Err(e) => {
                        values.push(None);
                        errors.push(Some(KvError::Other(format!(
                            "Deserialization failed: {}",
                            e
                        ))));
                    }
                },
                None => {
                    values.push(None);
                    errors.push(Some(KvError::KeyNotFound));
                }
            }
        }

        Ok((values, errors))
    }

    async fn batch_del(&self, keys: &[K]) -> Result<Vec<Result<(), KvError>>, KvError> {
        if keys.is_empty() {
            return Ok(vec![]);
        }

        // 1. 序列化所有键
        let mut key_strs = Vec::with_capacity(keys.len());
        for key in keys {
            let key_bytes = self
                .key_serializer
                .serialize(key.clone())
                .map_err(|e| KvError::Other(format!("Key serialization failed: {}", e)))?;

            let key_str = String::from_utf8(key_bytes)
                .map_err(|e| KvError::Other(format!("Invalid key UTF-8: {}", e)))?;

            key_strs.push(key_str);
        }

        // 2. 使用 Pipeline 批量删除
        let mut con = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| KvError::Other(format!("Failed to get connection: {}", e)))?;

        let mut pipe = redis::pipe();
        pipe.atomic();

        for key_str in &key_strs {
            pipe.del(key_str);
        }

        // 3. 执行 Pipeline
        let _: () = pipe
            .query_async(&mut con)
            .await
            .map_err(|e| KvError::Other(format!("Pipeline DEL failed: {}", e)))?;

        // 4. 返回成功结果
        Ok((0..key_strs.len()).map(|_| Ok(())).collect())
    }

    async fn close(&self) -> Result<(), KvError> {
        // Redis Client 会自动管理连接，这里不需要特殊处理
        Ok(())
    }
}

// 实现 cfg 模块要求的 From trait
impl<K, V> From<RedisStoreConfig> for RedisStore<K, V>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(config: RedisStoreConfig) -> Self {
        RedisStore::new(config).unwrap()
    }
}

impl<K, V> From<Box<RedisStore<K, V>>> for Box<dyn Store<K, V>>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<RedisStore<K, V>>) -> Self {
        source as Box<dyn Store<K, V>>
    }
}

/// 实现 IsAsyncStore 标记，让 RedisStore 自动获得 SyncStore 能力
impl<K, V> IsAsyncStore for RedisStore<K, V>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::serializer::register_serde_serializers;
    use crate::kv::store::common_tests::*;
    use crate::kv::store::core::SyncStore;
    use serial_test::serial;

    // 清理测试数据的辅助函数
    async fn cleanup_test_keys<V>(store: &RedisStore<String, V>, keys: Vec<&str>)
    where
        V: Clone + Send + Sync + 'static,
    {
        for key in keys {
            let _ = store.del(&key.to_string()).await;
        }
    }

    // 辅助函数：创建用于测试的 RedisStore<String, String>
    // RedisStore 实现了 Store (异步) 并标记为 IsAsyncStore，
    // 所以会自动获得 SyncStore 实现，同一个 store 可用于异步和同步测试
    async fn make_store_string() -> RedisStore<String, String> {
        register_serde_serializers::<String>().unwrap();
        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            ..Default::default()
        };
        let store = RedisStore::<String, String>::new(config).unwrap();

        // 清理可能的测试残留数据
        cleanup_test_keys(&store, vec!["test_key", "test_key2"]).await;

        store
    }

    // 辅助函数：创建用于测试的 RedisStore<String, i32>
    async fn make_store_i32() -> RedisStore<String, i32> {
        register_serde_serializers::<String>().unwrap();
        register_serde_serializers::<i32>().unwrap();
        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            ..Default::default()
        };
        let store = RedisStore::<String, i32>::new(config).unwrap();

        // 清理可能的测试残留数据
        cleanup_test_keys(&store, vec!["key1", "key2", "key3", "key4", "key5"]).await;

        store
    }

    // ========== 公共测试 ==========

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_store_set() {
        let store = make_store_string().await;
        test_set(store).await;
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_store_get() {
        let store = make_store_string().await;
        test_get(store).await;
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_store_del() {
        let store = make_store_string().await;
        test_del(store).await;
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_store_batch_set() {
        let store = make_store_i32().await;
        test_batch_set(store).await;
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_store_batch_get() {
        let store = make_store_i32().await;
        test_batch_get(store).await;
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_store_batch_del() {
        let store = make_store_i32().await;
        test_batch_del(store).await;
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_store_close() {
        let store = make_store_i32().await;
        test_close(store).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    #[serial]
    async fn test_store_set_sync() {
        let store = make_store_string().await;
        test_set_sync(store);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    #[serial]
    async fn test_store_get_sync() {
        let store = make_store_string().await;
        test_get_sync(store);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    #[serial]
    async fn test_store_del_sync() {
        let store = make_store_string().await;
        test_del_sync(store);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    #[serial]
    async fn test_store_batch_set_sync() {
        let store = make_store_i32().await;
        test_batch_set_sync(store);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    #[serial]
    async fn test_store_batch_get_sync() {
        let store = make_store_i32().await;
        test_batch_get_sync(store);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    #[serial]
    async fn test_store_batch_del_sync() {
        let store = make_store_i32().await;
        test_batch_del_sync(store);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    #[serial]
    async fn test_store_close_sync() {
        let store = make_store_i32().await;
        test_close_sync(store);
    }

    // ========== 场景测试 ==========

    #[test]
    fn test_redis_store_config_default() {
        let config = RedisStoreConfig::default();
        assert_eq!(config.endpoint, None);
        assert_eq!(config.endpoints, None);
        assert_eq!(config.db, 0);
        assert_eq!(config.default_ttl, 0);
        assert_eq!(config.connection_timeout, 5);
        assert_eq!(config.command_timeout, 3);
    }

    #[test]
    fn test_redis_store_config_serialization() {
        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            password: Some("secret".to_string()),
            db: 1,
            default_ttl: 3600,
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        println!("RedisStoreConfig JSON:\n{}", json);

        let deserialized: RedisStoreConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_redis_store_config_validation() {
        // 测试既没有 endpoint 也没有 endpoints
        let config = RedisStoreConfig::default();
        let result = RedisStore::<String, String>::new(config);
        assert!(result.is_err());
        if let Err(RedisError::InvalidConfig(msg)) = result {
            assert!(msg.contains("Either endpoint or endpoints must be set"));
        } else {
            panic!("Expected InvalidConfig error");
        }

        // 测试同时设置 endpoint 和 endpoints
        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            endpoints: Some(vec!["node1:6379".to_string()]),
            ..Default::default()
        };
        let result = RedisStore::<String, String>::new(config);
        assert!(result.is_err());
        if let Err(RedisError::InvalidConfig(msg)) = result {
            assert!(msg.contains("Cannot set both endpoint and endpoints"));
        } else {
            panic!("Expected InvalidConfig error");
        }
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_store_ttl() {
        register_serde_serializers::<String>().unwrap();

        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            default_ttl: 1, // 1 秒过期
            ..Default::default()
        };

        let store = RedisStore::<String, String>::new(config).unwrap();

        // 使用唯一的 key（基于测试名称）
        let prefix = "test_store_ttl";
        let ttl_key = format!("{}:ttl_key", prefix);

        store
            .set(&ttl_key, &"ttl_value".to_string(), &SetOptions::new())
            .await
            .unwrap();

        // 立即获取应该成功
        let value = store.get(&ttl_key).await.unwrap();
        assert_eq!(value, "ttl_value");

        // 等待 2 秒后应该过期
        tokio::time::sleep(Duration::from_secs(2)).await;
        let result = store.get(&ttl_key).await;
        assert!(matches!(result, Err(KvError::KeyNotFound)));
    }

    #[tokio::test]
    #[ignore]
    #[serial]
    async fn test_store_custom_serializer() {
        // 注册 JSON 序列化器
        register_serde_serializers::<String>().unwrap();

        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
        struct User {
            name: String,
            age: u32,
        }

        register_serde_serializers::<User>().unwrap();

        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            ..Default::default()
        };

        let store = RedisStore::<String, User>::new(config).unwrap();

        // 使用唯一的 key（基于测试名称）
        let prefix = "test_store_custom_serializer";
        let user_key = format!("{}:user:1", prefix);

        let user = User {
            name: "Alice".to_string(),
            age: 30,
        };

        store
            .set(&user_key, &user, &SetOptions::new())
            .await
            .unwrap();

        let retrieved = store.get(&user_key).await.unwrap();
        assert_eq!(retrieved, user);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    #[serial]
    async fn test_redis_store_sync_ttl() {
        register_serde_serializers::<String>().unwrap();

        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            default_ttl: 1, // 1 秒过期
            ..Default::default()
        };

        let store = RedisStore::<String, String>::new(config).unwrap();

        // 使用唯一的 key（基于测试名称）
        let prefix = "test_redis_store_sync_ttl";
        let ttl_key = format!("{}:ttl_key", prefix);

        store
            .set_sync(&ttl_key, &"ttl_value".to_string(), &SetOptions::new())
            .unwrap();

        // 立即获取应该成功
        let value = store.get_sync(&ttl_key).unwrap();
        assert_eq!(value, "ttl_value");

        // 等待 2 秒后应该过期
        std::thread::sleep(Duration::from_secs(2));
        let result = store.get_sync(&ttl_key);
        assert!(matches!(result, Err(KvError::KeyNotFound)));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    #[serial]
    async fn test_store_custom_serializer_sync() {
        // 注册 JSON 序列化器
        register_serde_serializers::<String>().unwrap();

        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
        struct User {
            name: String,
            age: u32,
        }

        register_serde_serializers::<User>().unwrap();

        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            ..Default::default()
        };

        let store = RedisStore::<String, User>::new(config).unwrap();

        // 使用唯一的 key（基于测试名称）
        let prefix = "test_store_custom_serializer_sync";
        let user_key = format!("{}:user:1", prefix);

        let user = User {
            name: "Alice".to_string(),
            age: 30,
        };

        store
            .set_sync(&user_key, &user, &SetOptions::new())
            .unwrap();

        let retrieved = store.get_sync(&user_key).unwrap();
        assert_eq!(retrieved, user);
    }
}
