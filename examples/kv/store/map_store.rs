use anyhow::Result;
use rustx::kv::store::{MapStore, MapStoreConfig, Store, SetOptions};
use rustx::cfg::*;

#[tokio::main]
async fn main() -> Result<()> {
    // 零耦合自动注册！MapStore 完全不需要知道配置系统的存在
    register::<MapStore<String, String>, MapStoreConfig>()?;

    println!("=== MapStore JSON 配置示例 ===");

    // JSON 配置示例 - 使用已知的类型名
    let json_config = r#"{
        "type": "rustx::kv::store::map_store::MapStore<alloc::string::String, alloc::string::String>",
        "options": {
            "initial_capacity": 1000,
            "enable_stats": true
        }
    }"#;

    let type_options = TypeOptions::from_json(&json_config)?;
    println!("🔍 使用的类型名: {}", type_options.type_name);
    let store_obj = create_from_type_options(&type_options)?;

    if let Some(store) = store_obj.downcast_ref::<MapStore<String, String>>() {
        println!("✅ JSON配置创建MapStore成功");

        // 测试基本操作
        store.set("key1".to_string(), "val1".to_string(), SetOptions::new()).await?;
        store.set("key2".to_string(), "val2".to_string(), SetOptions::new()).await?;

        let val1 = store.get("key1".to_string()).await?;
        let val2 = store.get("key2".to_string()).await?;
        println!("📦 key1 value: {}", val1);
        println!("🔖 key2 value: {}", val2);

        // 测试批量操作
        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let values = vec!["value1".to_string(), "value2".to_string(), "value3".to_string()];

        let batch_results = store.batch_set(keys.clone(), values, SetOptions::new()).await?;
        println!("📝 批量设置结果: {:?}", batch_results);

        let (batch_values, batch_errors) = store.batch_get(keys).await?;
        println!("📖 批量获取值: {:?}", batch_values);
        println!("⚠️  批量获取错误: {:?}", batch_errors);
    }

    println!("\n🎉 MapStore JSON 配置示例完成!");

    Ok(())
}
