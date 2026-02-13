use anyhow::Result;
use std::hash::Hash;

use crate::cfg::register_trait;

use super::{
    DashMapStore, DashMapStoreConfig, LoadableSyncStore, LoadableSyncStoreConfig,
    UnsafeHashMapStore, UnsafeHashMapStoreConfig, RedisStore,
    RedisStoreConfig, RwLockHashMapStore, RwLockHashMapStoreConfig, Store, AsyncStore, SyncStore,
};

/// 注册所有内存存储实现到 cfg 注册表（统一接口）
///
/// 为指定的 K, V 类型组合注册所有可用的内存存储实现，同时注册三种 trait：
/// - `dyn Store<K, V>` - 统一接口，同时支持同步和异步方法
/// - `dyn SyncStore<K, V>` - 纯同步接口
/// - `dyn AsyncStore<K, V>` - 纯异步接口
///
/// 由于这些 trait 是泛型 trait，不同的 K, V 组合会产生不同的 TypeId，
/// 因此可以使用相同的类型名称注册，不会冲突。
///
/// # 类型参数
/// - `K`: 键类型，需要满足 `Clone + Send + Sync + Eq + Hash + 'static`
/// - `V`: 值类型，需要满足 `Clone + Send + Sync + 'static`
///
/// # 注册的类型
/// - `UnsafeHashMapStore` - 基于 HashMap 的非线程安全实现
/// - `RwLockHashMapStore` - 基于 RwLock + HashMap 的线程安全实现
/// - `DashMapStore` - 基于 DashMap 的线程安全实现（高并发性能更好）
/// - `LoadableSyncStore` - 可加载数据的同步存储装饰器
///
/// # 示例
/// ```ignore
/// use rustx::kv::store::{register_hash_stores, Store};
/// use rustx::cfg::{TypeOptions, create_trait_from_type_options};
///
/// // 一次性注册所有接口类型
/// register_hash_stores::<String, String>()?;
///
/// // 使用统一接口 Store
/// let opts = TypeOptions::from_json(r#"{
///     "type": "DashMapStore",
///     "options": { "initial_capacity": 100 }
/// }"#)?;
///
/// let store: Box<dyn Store<String, String>> = create_trait_from_type_options(&opts)?;
///
/// // 可以调用同步方法
/// store.set_sync(&"key".to_string(), &"value".to_string(), &SetOptions::new())?;
///
/// // 也可以调用异步方法
/// store.set(&"key2".to_string(), &"value2".to_string(), &SetOptions::new()).await?;
/// ```
pub fn register_hash_stores<K, V>() -> Result<()>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    // 注册统一接口 Store
    register_trait::<UnsafeHashMapStore<K, V>, dyn Store<K, V>, UnsafeHashMapStoreConfig>("UnsafeHashMapStore")?;
    register_trait::<RwLockHashMapStore<K, V>, dyn Store<K, V>, RwLockHashMapStoreConfig>(
        "RwLockHashMapStore",
    )?;
    register_trait::<DashMapStore<K, V>, dyn Store<K, V>, DashMapStoreConfig>(
        "DashMapStore",
    )?;
    register_trait::<LoadableSyncStore<K, V>, dyn Store<K, V>, LoadableSyncStoreConfig>(
        "LoadableSyncStore",
    )?;

    // 注册纯同步接口 SyncStore
    register_trait::<UnsafeHashMapStore<K, V>, dyn SyncStore<K, V>, UnsafeHashMapStoreConfig>("UnsafeHashMapStore")?;
    register_trait::<RwLockHashMapStore<K, V>, dyn SyncStore<K, V>, RwLockHashMapStoreConfig>(
        "RwLockHashMapStore",
    )?;
    register_trait::<DashMapStore<K, V>, dyn SyncStore<K, V>, DashMapStoreConfig>(
        "DashMapStore",
    )?;
    register_trait::<LoadableSyncStore<K, V>, dyn SyncStore<K, V>, LoadableSyncStoreConfig>(
        "LoadableSyncStore",
    )?;

    // 注册纯异步接口 AsyncStore
    register_trait::<UnsafeHashMapStore<K, V>, dyn AsyncStore<K, V>, UnsafeHashMapStoreConfig>("UnsafeHashMapStore")?;
    register_trait::<RwLockHashMapStore<K, V>, dyn AsyncStore<K, V>, RwLockHashMapStoreConfig>(
        "RwLockHashMapStore",
    )?;
    register_trait::<DashMapStore<K, V>, dyn AsyncStore<K, V>, DashMapStoreConfig>(
        "DashMapStore",
    )?;
    register_trait::<LoadableSyncStore<K, V>, dyn AsyncStore<K, V>, LoadableSyncStoreConfig>(
        "LoadableSyncStore",
    )?;

    Ok(())
}

/// 注册 Store 到 cfg 注册表（统一接口）
///
/// 为指定的 K, V 类型组合注册存储实现，同时注册三种 trait：
/// - `dyn Store<K, V>` - 统一接口，同时支持同步和异步方法
/// - `dyn SyncStore<K, V>` - 纯同步接口
/// - `dyn AsyncStore<K, V>` - 纯异步接口
///
/// 用于注册不需要 Hash 约束的存储类型（如 Redis）。
/// 对于内存哈希存储，请使用 `register_hash_stores`。
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
/// use rustx::kv::store::register_stores;
///
/// // 先注册序列化器
/// register_serde_serializers::<String>()?;
/// register_serde_serializers::<MyValue>()?;
///
/// // 再注册 Store
/// register_stores::<String, MyValue>()?;
/// ```
///
/// # 示例
/// ```ignore
/// use rustx::kv::store::{register_stores, Store};
/// use rustx::kv::serializer::register_serde_serializers;
/// use rustx::cfg::{TypeOptions, create_trait_from_type_options};
///
/// // 注册序列化器
/// register_serde_serializers::<String>()?;
///
/// // 注册 Store（一次调用，三种接口都可用）
/// register_stores::<String, String>()?;
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
///
/// // 可以调用异步方法
/// store.set(&"key".to_string(), &"value".to_string(), &SetOptions::new()).await?;
///
/// // 也可以调用同步方法
/// let value = store.get_sync(&"key".to_string())?;
/// ```
pub fn register_stores<K, V>() -> Result<()>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    // 注册统一接口 Store
    register_trait::<RedisStore<K, V>, dyn Store<K, V>, RedisStoreConfig>("RedisStore")?;

    // 注册纯同步接口 SyncStore
    register_trait::<RedisStore<K, V>, dyn SyncStore<K, V>, RedisStoreConfig>("RedisStore")?;

    // 注册纯异步接口 AsyncStore
    register_trait::<RedisStore<K, V>, dyn AsyncStore<K, V>, RedisStoreConfig>("RedisStore")?;

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

        // 测试 UnsafeHashMapStore
        let opts = TypeOptions::from_json(
            r#"{
            "type": "UnsafeHashMapStore",
            "options": {}
        }"#,
        )?;

        let store: Box<dyn AsyncStore<String, String>> = create_trait_from_type_options(&opts)?;

        // 验证 store 可以正常使用
        store
            .set(&"key".to_string(), &"value".to_string(), &SetOptions::new())
            .await
            .unwrap();
        let value = store.get(&"key".to_string()).await.unwrap();
        assert_eq!(value, "value");

        Ok(())
    }

    #[tokio::test]
    async fn test_register_stores_safe_hash_map() -> Result<()> {
        register_hash_stores::<String, i32>()?;

        // 测试 RwLockHashMapStore
        let opts = TypeOptions::from_json(
            r#"{
            "type": "RwLockHashMapStore",
            "options": { "initial_capacity": 100 }
        }"#,
        )?;

        let store: Box<dyn AsyncStore<String, i32>> = create_trait_from_type_options(&opts)?;

        store
            .set(&"count".to_string(), &42, &SetOptions::new())
            .await
            .unwrap();
        let value = store.get(&"count".to_string()).await.unwrap();
        assert_eq!(value, 42);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_dash_map_store() -> Result<()> {
        register_hash_stores::<String, i32>()?;

        // 测试 DashMapStore
        let opts = TypeOptions::from_json(
            r#"{
            "type": "DashMapStore",
            "options": { "initial_capacity": 100 }
        }"#,
        )?;

        let store: Box<dyn AsyncStore<String, i32>> = create_trait_from_type_options(&opts)?;

        store
            .set(&"count".to_string(), &42, &SetOptions::new())
            .await
            .unwrap();
        let value = store.get(&"count".to_string()).await.unwrap();
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
        let opts_str_str = TypeOptions::from_json(r#"{"type": "UnsafeHashMapStore", "options": {}}"#)?;
        let opts_str_i64 =
            TypeOptions::from_json(r#"{"type": "RwLockHashMapStore", "options": {}}"#)?;

        let _store1: Box<dyn AsyncStore<String, String>> =
            create_trait_from_type_options(&opts_str_str)?;
        let _store2: Box<dyn AsyncStore<String, i64>> = create_trait_from_type_options(&opts_str_i64)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_register_stores_unified() -> Result<()> {
        // 使用统一的 register_hash_stores 方法
        register_hash_stores::<String, String>()?;

        // 测试创建 Store（统一接口）
        let opts = TypeOptions::from_json(
            r#"{
            "type": "DashMapStore",
            "options": {}
        }"#,
        )?;

        let store: Box<dyn Store<String, String>> = create_trait_from_type_options(&opts)?;

        // 测试异步方法
        store
            .set(&"key".to_string(), &"value".to_string(), &SetOptions::new())
            .await
            .unwrap();
        let value = store.get(&"key".to_string()).await.unwrap();
        assert_eq!(value, "value");

        // 测试同步方法
        let value_sync = store.get_sync(&"key".to_string()).unwrap();
        assert_eq!(value_sync, "value");

        Ok(())
    }

    #[tokio::test]
    async fn test_register_stores_all_three_traits() -> Result<()> {
        // 使用统一的 register_hash_stores 方法
        register_hash_stores::<String, i32>()?;

        let opts = TypeOptions::from_json(
            r#"{
            "type": "RwLockHashMapStore",
            "options": {}
        }"#,
        )?;

        // 可以创建 Store（统一接口）
        let store: Box<dyn Store<String, i32>> = create_trait_from_type_options(&opts)?;
        assert!(store.set(&"k".to_string(), &42, &SetOptions::new()).await.is_ok());

        // 可以创建 SyncStore（纯同步）
        let sync_store: Box<dyn SyncStore<String, i32>> = create_trait_from_type_options(&opts)?;
        assert!(sync_store.set_sync(&"k".to_string(), &42, &SetOptions::new()).is_ok());

        // 可以创建 AsyncStore（纯异步）
        let async_store: Box<dyn AsyncStore<String, i32>> = create_trait_from_type_options(&opts)?;
        assert!(async_store.set(&"k".to_string(), &42, &SetOptions::new()).await.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_register_redis_stores_unified() -> Result<()> {
        use crate::kv::serializer::register_serde_serializers;

        // 注册序列化器
        register_serde_serializers::<String>()?;

        // 使用统一的 register_stores 方法
        register_stores::<String, String>()?;

        let opts = TypeOptions::from_json(
            r#"{
            "type": "RedisStore",
            "options": {
                "endpoint": "localhost:6379"
            }
        }"#,
        )?;

        // 可以创建 Store（统一接口）
        let store: Box<dyn Store<String, String>> = create_trait_from_type_options(&opts)?;

        // 注意：这个测试需要 Redis 服务器运行，所以只是验证类型创建成功
        drop(store); // 避免未使用警告

        Ok(())
    }
}
