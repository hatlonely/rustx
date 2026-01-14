use anyhow::Result;
use rustx::cfg::*;
use rustx::kv::store::{SafeHashMapStore, SafeHashMapStoreConfig, SetOptions, Store};

#[tokio::main]
async fn main() -> Result<()> {
    // 零耦合自动注册！线程安全 SafeHashMapStore 完全不需要知道配置系统的存在
    register_auto::<SafeHashMapStore<String, String>, SafeHashMapStoreConfig>()?;

    println!("=== 线程安全 SafeHashMapStore JSON 配置示例 ===");

    // JSON 配置示例 - 使用已知的类型名
    let json_config = r#"{
        "type": "rustx::kv::store::safe_hash_map_store::SafeHashMapStore<alloc::string::String, alloc::string::String>",
        "options": {
            "initial_capacity": 10000
        }
    }"#;

    let type_options = TypeOptions::from_json(&json_config)?;
    println!("🔍 使用的类型名: {}", type_options.type_name);
    let store_obj = create_from_type_options(&type_options)?;

    if let Some(store) = store_obj.downcast_ref::<SafeHashMapStore<String, String>>() {
        println!("✅ JSON配置创建线程安全SafeHashMapStore成功");

        // 测试基本操作
        store
            .set("key1".to_string(), "val1".to_string(), SetOptions::new())
            .await?;
        store
            .set("key2".to_string(), "val2".to_string(), SetOptions::new())
            .await?;

        let val1 = store.get("key1".to_string()).await?;
        let val2 = store.get("key2".to_string()).await?;
        println!("📦 key1 value: {}", val1);
        println!("🔖 key2 value: {}", val2);

        // 测试条件设置
        println!("\n=== 测试 if_not_exist 条件 ===");
        let result = store
            .set(
                "key1".to_string(),
                "new_val1".to_string(),
                SetOptions::new().with_if_not_exist(),
            )
            .await;

        match result {
            Err(_) => println!("🚫 key1 已存在，条件设置失败（符合预期）"),
            Ok(_) => println!("⚠️  key1 不存在时才能设置，但设置成功了？"),
        }

        let unchanged_val = store.get("key1".to_string()).await?;
        println!("🔄 key1 值未改变: {}", unchanged_val);

        // 测试批量操作
        println!("\n=== 测试批量操作 ===");
        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let values = vec![
            "value1".to_string(),
            "value2".to_string(),
            "value3".to_string(),
        ];

        let batch_results = store
            .batch_set(keys.clone(), values, SetOptions::new())
            .await?;
        println!("📝 批量设置结果: {:?}", batch_results);

        let (batch_values, batch_errors) = store.batch_get(keys.clone()).await?;
        println!("📖 批量获取值: {:?}", batch_values);
        println!("⚠️  批量获取错误: {:?}", batch_errors);

        // 测试批量删除
        println!("\n=== 测试批量删除 ===");
        let del_results = store.batch_del(keys.clone()).await?;
        println!("🗑️  批量删除结果: {:?}", del_results);

        // 验证删除结果
        let (empty_values, not_found_errors) = store.batch_get(keys).await?;
        println!("🔍 删除后获取值: {:?}", empty_values);
        println!("❌ 删除后获取错误: {:?}", not_found_errors);

        // 注意：SafeHashMapStore 内部使用 RwLock 提供线程安全
        println!("\n=== 测试线程安全特性 ===");
        println!("💡 SafeHashMapStore 使用 RwLock<HashMap> 实现，天然支持多线程安全");

        // 测试性能对比示例
        println!("\n=== 性能测试示例 ===");
        let start = std::time::Instant::now();

        for i in 0..10000 {
            store
                .set(
                    format!("perf_key_{}", i),
                    format!("perf_value_{}", i),
                    SetOptions::new(),
                )
                .await?;
        }

        let set_duration = start.elapsed();
        println!("⚡ 设置 10000 个键值对耗时: {:?}", set_duration);

        let start = std::time::Instant::now();
        for i in 0..10000 {
            let _ = store.get(format!("perf_key_{}", i)).await?;
        }
        let get_duration = start.elapsed();
        println!("🔍 获取 10000 个键值对耗时: {:?}", get_duration);

        // 清理测试数据
        store.close().await?;
        println!("🧹 存储已关闭和清理");
    }

    println!("\n🎉 线程安全 SafeHashMapStore JSON 配置示例完成!");
    println!("💡 注意：SafeHashMapStore 内置线程安全保护，适合在多线程环境下使用，确保数据一致性");

    Ok(())
}
