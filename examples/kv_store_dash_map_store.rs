use anyhow::Result;
use rustx::cfg::*;
use rustx::kv::store::{register_hash_stores, SetOptions, Store};

#[tokio::main]
async fn main() -> Result<()> {
    // 注册 DashMapStore
    register_hash_stores::<String, String>()?;

    // 通过 JSON5 配置创建 DashMapStore
    // DashMapStore 基于分片锁实现，高并发场景性能优于 RwLockHashMapStore
    let config = r#"{
        type: "DashMapStore",
        options: {
            initial_capacity: 1000,
        }
    }"#;

    let opts = TypeOptions::from_json(config)?;
    let store: Box<dyn Store<String, String>> = create_trait_from_type_options(&opts)?;

    // 基本操作
    store.set(&"key1".to_string(), &"value1".to_string(), &SetOptions::new()).await?;
    let value = store.get(&"key1".to_string()).await?;
    println!("get key1: {}", value);

    // 条件设置：仅在键不存在时设置
    store.set(&"key2".to_string(), &"value2".to_string(), &SetOptions::new().with_if_not_exist()).await?;
    let result = store.set(&"key2".to_string(), &"new_value2".to_string(), &SetOptions::new().with_if_not_exist()).await;
    println!("set with if_not_exist on existing key: {:?}", result.is_err());

    // 批量操作
    let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
    let values = vec!["val1".to_string(), "val2".to_string(), "val3".to_string()];
    store.batch_set(&keys, &values, &SetOptions::new()).await?;

    let (vals, errs) = store.batch_get(&keys).await?;
    println!("batch_get: {:?}, errors: {:?}", vals, errs);

    store.batch_del(&keys).await?;
    store.close().await?;

    Ok(())
}
