use anyhow::Result;
use std::hash::Hash;

use crate::cfg::register_trait;

use super::{
    HashMapStore, HashMapStoreConfig, RedisStore, RedisStoreConfig, SafeHashMapStore,
    SafeHashMapStoreConfig, Store,
};

/// 注册所有 Store 实现到 cfg 注册表
///
/// 为指定的 K, V 类型组合注册所有可用的 Store 实现。
/// 由于 `Store<K, V>` 是泛型 trait，不同的 K, V 组合会产生不同的 TypeId，
/// 因此可以使用相同的类型名称注册，不会冲突。
///
/// # 类型参数
/// - `K`: 键类型，需要满足 `Clone + Send + Sync + Eq + Hash + 'static`
/// - `V`: 值类型，需要满足 `Clone + Send + Sync + 'static`
///
/// # 注册的类型
/// - `HashMapStore` - 基于 HashMap 的非线程安全实现
/// - `SafeHashMapStore` - 基于 RwLock + HashMap 的线程安全实现
///
/// # 示例
/// ```ignore
/// use rustx::kv::store::{register_stores, Store};
/// use rustx::cfg::{TypeOptions, create_trait_from_type_options};
///
/// // 注册 String -> String 类型的 Store
/// register_stores::<String, String>()?;
///
/// // 通过配置创建实例
/// let opts = TypeOptions::from_json(r#"{
///     "type": "HashMapStore",
///     "options": { "initial_capacity": 100 }
/// }"#)?;
///
/// let store: Box<dyn Store<String, String>> = create_trait_from_type_options(&opts)?;
/// ```
pub fn register_hash_stores<K, V>() -> Result<()>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    register_trait::<HashMapStore<K, V>, dyn Store<K, V>, HashMapStoreConfig>("HashMapStore")?;
    register_trait::<SafeHashMapStore<K, V>, dyn Store<K, V>, SafeHashMapStoreConfig>(
        "SafeHashMapStore",
    )?;
    Ok(())
}

/// 注册 Redis Store 到 cfg 注册表
///
/// 为指定的 K, V 类型组合注册 RedisStore 实现。
///
/// # 类型参数
/// - `K`: 键类型，需要满足 `Clone + Send + Sync + 'static`
/// - `V`: 值类型，需要满足 `Clone + Send + Sync + 'static`
///
/// # 注册的类型
/// - `RedisStore` - 基于 Redis 的分布式存储实现
///
/// # 前置条件
/// 在调用此函数之前，必须先注册键和值类型的序列化器：
/// ```ignore
/// use rustx::kv::serializer::register_serde_serializers;
/// use rustx::kv::store::register_redis_stores;
///
/// // 先注册序列化器
/// register_serde_serializers::<String>()?;
/// register_serde_serializers::<MyValue>()?;
///
/// // 再注册 Store
/// register_redis_stores::<String, MyValue>()?;
/// ```
///
/// # 示例
/// ```ignore
/// use rustx::kv::store::{register_redis_stores, Store};
/// use rustx::cfg::{TypeOptions, create_trait_from_type_options};
///
/// // 注册序列化器
/// register_serde_serializers::<String>()?;
///
/// // 注册 Store
/// register_redis_stores::<String, String>()?;
///
/// // 通过配置创建实例
/// let opts = TypeOptions::from_json(r#"{
///     "type": "RedisStore",
///     "options": {
///         "endpoint": "localhost:6379",
///         "password": "secret",
///         "db": 0,
///         "default_ttl": 3600
///     }
/// }"#)?;
///
/// let store: Box<dyn Store<String, String>> = create_trait_from_type_options(&opts)?;
/// ```
pub fn register_redis_stores<K, V>() -> Result<()>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    register_trait::<RedisStore<K, V>, dyn Store<K, V>, RedisStoreConfig>("RedisStore")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{create_trait_from_type_options, TypeOptions};
    use crate::kv::store::SetOptions;

    #[tokio::test]
    async fn test_register_stores_string_string() -> Result<()> {
        register_hash_stores::<String, String>()?;

        // 测试 HashMapStore
        let opts = TypeOptions::from_json(
            r#"{
            "type": "HashMapStore",
            "options": {}
        }"#,
        )?;

        let store: Box<dyn Store<String, String>> = create_trait_from_type_options(&opts)?;

        // 验证 store 可以正常使用
        store
            .set("key".to_string(), "value".to_string(), SetOptions::new())
            .await
            .unwrap();
        let value = store.get("key".to_string()).await.unwrap();
        assert_eq!(value, "value");

        Ok(())
    }

    #[tokio::test]
    async fn test_register_stores_safe_hash_map() -> Result<()> {
        register_hash_stores::<String, i32>()?;

        // 测试 SafeHashMapStore
        let opts = TypeOptions::from_json(
            r#"{
            "type": "SafeHashMapStore",
            "options": { "initial_capacity": 100 }
        }"#,
        )?;

        let store: Box<dyn Store<String, i32>> = create_trait_from_type_options(&opts)?;

        store
            .set("count".to_string(), 42, SetOptions::new())
            .await
            .unwrap();
        let value = store.get("count".to_string()).await.unwrap();
        assert_eq!(value, 42);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_multiple_type_combinations() -> Result<()> {
        // 注册多种类型组合
        register_hash_stores::<String, String>()?;
        register_hash_stores::<String, i64>()?;
        register_hash_stores::<i32, String>()?;

        // 验证各类型组合都能正常工作
        let opts_str_str = TypeOptions::from_json(r#"{"type": "HashMapStore", "options": {}}"#)?;
        let opts_str_i64 =
            TypeOptions::from_json(r#"{"type": "SafeHashMapStore", "options": {}}"#)?;

        let _store1: Box<dyn Store<String, String>> =
            create_trait_from_type_options(&opts_str_str)?;
        let _store2: Box<dyn Store<String, i64>> = create_trait_from_type_options(&opts_str_i64)?;

        Ok(())
    }
}
