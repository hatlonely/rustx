use async_trait::async_trait;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::time::Duration;

use super::core::{KvError, SetOptions, Store};
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
        let is_cluster = config.endpoints.as_ref().map_or(false, |e| !e.is_empty());
        let is_single = config.endpoint.as_ref().map_or(false, |e| !e.is_empty());

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
    async fn set(&self, key: K, value: V, options: SetOptions) -> Result<(), KvError> {
        // 1. 序列化键值
        let key_bytes = self
            .key_serializer
            .serialize(key)
            .await
            .map_err(|e| KvError::Other(format!("Key serialization failed: {}", e)))?;

        let val_bytes = self
            .val_serializer
            .serialize(value)
            .await
            .map_err(|e| KvError::Other(format!("Value serialization failed: {}", e)))?;

        // 2. 转换键为字符串（Redis 键必须是字符串）
        let key_str = String::from_utf8(key_bytes)
            .map_err(|e| KvError::Other(format!("Invalid key UTF-8: {}", e)))?;

        // 3. 检查 IfNotExist 条件
        if options.if_not_exist {
            let mut con = self
                .client
                .get_multiplexed_async_connection()
                .await
                .map_err(|e| KvError::Other(format!("Failed to get connection: {}", e)))?;

            let exists: bool = redis::cmd("EXISTS")
                .arg(&key_str)
                .query_async(&mut con)
                .await
                .map_err(|e| KvError::Other(format!("EXISTS failed: {}", e)))?;

            if exists {
                return Err(KvError::ConditionFailed);
            }
        }

        // 4. 确定 TTL
        let expiration = options.expiration.unwrap_or(self.default_ttl);

        // 5. 执行 SET 命令
        let mut con = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| KvError::Other(format!("Failed to get connection: {}", e)))?;

        if expiration > Duration::ZERO {
            // 使用 SETEX 命令设置带过期时间的键
            let _: () = con
                .set_ex(&key_str, val_bytes.as_slice(), expiration.as_secs())
                .await
                .map_err(|e| KvError::Other(format!("SET failed: {}", e)))?;
        } else {
            // 使用 SET 命令
            let _: () = con
                .set(&key_str, val_bytes.as_slice())
                .await
                .map_err(|e| KvError::Other(format!("SET failed: {}", e)))?;
        }

        Ok(())
    }

    async fn get(&self, key: K) -> Result<V, KvError> {
        // 1. 序列化键
        let key_bytes = self
            .key_serializer
            .serialize(key)
            .await
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
                    .await
                    .map_err(|e| KvError::Other(format!("Value deserialization failed: {}", e)))
            }
            None => Err(KvError::KeyNotFound),
        }
    }

    async fn del(&self, key: K) -> Result<(), KvError> {
        // 1. 序列化键
        let key_bytes = self
            .key_serializer
            .serialize(key)
            .await
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
        keys: Vec<K>,
        vals: Vec<V>,
        options: SetOptions,
    ) -> Result<Vec<Result<(), KvError>>, KvError> {
        if keys.len() != vals.len() {
            return Err(KvError::Other(
                "Keys and values length mismatch".to_string(),
            ));
        }

        // 1. 序列化所有键值对
        let mut items = Vec::with_capacity(keys.len());
        for (key, val) in keys.into_iter().zip(vals.into_iter()) {
            let key_bytes = self
                .key_serializer
                .serialize(key)
                .await
                .map_err(|e| KvError::Other(format!("Key serialization failed: {}", e)))?;

            let val_bytes = self
                .val_serializer
                .serialize(val)
                .await
                .map_err(|e| KvError::Other(format!("Value serialization failed: {}", e)))?;

            let key_str = String::from_utf8(key_bytes)
                .map_err(|e| KvError::Other(format!("Invalid key UTF-8: {}", e)))?;

            items.push((key_str, val_bytes));
        }

        // 2. 确定 TTL
        let expiration = options.expiration.unwrap_or(self.default_ttl);

        // 3. 使用 Pipeline 批量执行
        let mut con = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| KvError::Other(format!("Failed to get connection: {}", e)))?;

        let mut pipe = redis::pipe();
        pipe.atomic();

        for (key_str, val_bytes) in &items {
            if expiration > Duration::ZERO {
                pipe.set_ex(key_str, val_bytes.as_slice(), expiration.as_secs());
            } else {
                pipe.set(key_str, val_bytes.as_slice());
            }
        }

        // 4. 执行 Pipeline
        let _: () = pipe
            .query_async(&mut con)
            .await
            .map_err(|e| KvError::Other(format!("Pipeline SET failed: {}", e)))?;

        // 5. 返回成功结果
        Ok((0..items.len()).map(|_| Ok(())).collect())
    }

    async fn batch_get(
        &self,
        keys: Vec<K>,
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError> {
        if keys.is_empty() {
            return Ok((vec![], vec![]));
        }

        // 1. 序列化所有键
        let mut key_strs = Vec::with_capacity(keys.len());
        for key in keys {
            let key_bytes = self
                .key_serializer
                .serialize(key)
                .await
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
                Some(val_bytes) => match self.val_serializer.deserialize(val_bytes).await {
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

    async fn batch_del(&self, keys: Vec<K>) -> Result<Vec<Result<(), KvError>>, KvError> {
        if keys.is_empty() {
            return Ok(vec![]);
        }

        // 1. 序列化所有键
        let mut key_strs = Vec::with_capacity(keys.len());
        for key in keys {
            let key_bytes = self
                .key_serializer
                .serialize(key)
                .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::serializer::register_serde_serializers;

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

    // 以下测试需要实际的 Redis 服务器，使用 #[ignore] 跳过
    // 运行时使用: cargo test -- --ignored

    #[tokio::test]
    #[ignore]
    async fn test_redis_store_basic_operations() {
        // 先注册序列化器
        register_serde_serializers::<String>().unwrap();

        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            ..Default::default()
        };

        let store = RedisStore::<String, String>::new(config).unwrap();

        // 测试 SET 和 GET
        store
            .set("key1".to_string(), "value1".to_string(), SetOptions::new())
            .await
            .unwrap();

        let value = store.get("key1".to_string()).await.unwrap();
        assert_eq!(value, "value1");

        // 测试 GET 不存在的键
        let result = store.get("nonexistent".to_string()).await;
        assert!(matches!(result, Err(KvError::KeyNotFound)));

        // 测试 DEL
        store.del("key1".to_string()).await.unwrap();
        let result = store.get("key1".to_string()).await;
        assert!(matches!(result, Err(KvError::KeyNotFound)));
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_store_ttl() {
        register_serde_serializers::<String>().unwrap();

        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            default_ttl: 1, // 1 秒过期
            ..Default::default()
        };

        let store = RedisStore::<String, String>::new(config).unwrap();

        store
            .set(
                "ttl_key".to_string(),
                "ttl_value".to_string(),
                SetOptions::new(),
            )
            .await
            .unwrap();

        // 立即获取应该成功
        let value = store.get("ttl_key".to_string()).await.unwrap();
        assert_eq!(value, "ttl_value");

        // 等待 2 秒后应该过期
        tokio::time::sleep(Duration::from_secs(2)).await;
        let result = store.get("ttl_key".to_string()).await;
        assert!(matches!(result, Err(KvError::KeyNotFound)));
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_store_if_not_exist() {
        register_serde_serializers::<String>().unwrap();

        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            ..Default::default()
        };

        let store = RedisStore::<String, String>::new(config).unwrap();

        // 第一次设置应该成功
        store
            .set(
                "nx_key".to_string(),
                "value1".to_string(),
                SetOptions::new().with_if_not_exist(),
            )
            .await
            .unwrap();

        // 第二次设置应该失败
        let result = store
            .set(
                "nx_key".to_string(),
                "value2".to_string(),
                SetOptions::new().with_if_not_exist(),
            )
            .await;
        assert!(matches!(result, Err(KvError::ConditionFailed)));

        // 验证值没有被覆盖
        let value = store.get("nx_key".to_string()).await.unwrap();
        assert_eq!(value, "value1");
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_store_batch_operations() {
        register_serde_serializers::<String>().unwrap();
        register_serde_serializers::<i32>().unwrap();

        let config = RedisStoreConfig {
            endpoint: Some("localhost:6379".to_string()),
            ..Default::default()
        };

        let store = RedisStore::<String, i32>::new(config).unwrap();

        // 测试批量设置
        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let values = vec![10, 20, 30];

        let results = store
            .batch_set(keys.clone(), values.clone(), SetOptions::new())
            .await
            .unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_ok()));

        // 测试批量获取
        let (retrieved_values, errors) = store.batch_get(keys.clone()).await.unwrap();
        assert_eq!(retrieved_values.len(), 3);
        assert_eq!(retrieved_values[0], Some(10));
        assert_eq!(retrieved_values[1], Some(20));
        assert_eq!(retrieved_values[2], Some(30));
        assert!(errors.iter().all(|e| e.is_none()));

        // 测试批量删除
        let del_results = store.batch_del(keys.clone()).await.unwrap();
        assert_eq!(del_results.len(), 3);
        assert!(del_results.iter().all(|r| r.is_ok()));

        // 验证删除后不存在
        let (empty_values, not_found_errors) = store.batch_get(keys).await.unwrap();
        assert!(empty_values.iter().all(|v| v.is_none()));
        assert!(not_found_errors
            .iter()
            .all(|e| matches!(e, Some(KvError::KeyNotFound))));
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_store_custom_serializer() {
        // 注册 JSON 和 MsgPack 序列化器
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

        let user = User {
            name: "Alice".to_string(),
            age: 30,
        };

        store
            .set("user:1".to_string(), user.clone(), SetOptions::new())
            .await
            .unwrap();

        let retrieved = store.get("user:1".to_string()).await.unwrap();
        assert_eq!(retrieved, user);
    }
}
