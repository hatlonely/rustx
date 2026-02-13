use anyhow::Result;
use rustx::cfg::*;
use rustx::kv::store::{register_hash_stores, SetOptions, Store};

#[tokio::main]
async fn main() -> Result<()> {
    // 注册 Store 类型
    register_hash_stores::<String, String>()?;

    println!("=== Store 统一接口示例 ===\n");

    // 通过 JSON 配置创建 DashMapStore
    let config = r#"{
        type: "DashMapStore",
        options: {
            initial_capacity: 1000,
        }
    }"#;

    let opts = TypeOptions::from_json(config)?;
    let store: Box<dyn Store<String, String>> = create_trait_from_type_options(&opts)?;

    println!("✅ Store 创建成功\n");

    // 演示异步方法
    println!("=== 异步方法 ===");
    store.set(&"key1".to_string(), &"async_value".to_string(), &SetOptions::new()).await?;
    let value = store.get(&"key1".to_string()).await?;
    println!("异步获取: key1 = {}", value);

    // 演示同步方法
    println!("\n=== 同步方法 ===");
    store.set_sync(&"key2".to_string(), &"sync_value".to_string(), &SetOptions::new())?;
    let value2 = store.get_sync(&"key2".to_string())?;
    println!("同步获取: key2 = {}", value2);

    // 演示批量操作（异步）
    println!("\n=== 批量操作（异步）===");
    let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
    let values = vec!["val1".to_string(), "val2".to_string(), "val3".to_string()];
    store.batch_set(&keys, &values, &SetOptions::new()).await?;
    let (vals, errs) = store.batch_get(&keys).await?;
    println!("批量获取（异步）: {:?}", vals);
    println!("批量错误: {:?}", errs);

    // 演示批量操作（同步）
    println!("\n=== 批量操作（同步）===");
    let (vals, errs) = store.batch_get_sync(&keys)?;
    println!("批量获取（同步）: {:?}", vals);
    println!("批量错误: {:?}", errs);

    // 清理
    store.close().await?;
    println!("\n🎉 Store 统一接口示例完成！");
    println!("💡 提示：Store trait 同时提供同步和异步方法，灵活适配不同场景");

    Ok(())
}
