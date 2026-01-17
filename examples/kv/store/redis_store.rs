use anyhow::Result;
use rustx::cfg::*;
use rustx::kv::serializer::register_serde_serializers;
use rustx::kv::store::{RedisStore, RedisStoreConfig, SetOptions, Store};

#[tokio::main]
async fn main() -> Result<()> {
    // 零耦合自动注册！RedisStore 完全不需要知道配置系统的存在
    register_serde_serializers::<String>()?;
    register_auto::<RedisStore<String, String>, RedisStoreConfig>()?;

    println!("=== RedisStore JSON 配置示例 ===");
    println!("⚠️  注意：此示例需要本地 Redis 服务器运行在 localhost:6379");
    println!("💡 启动 Redis: docker run -d -p 6379:6379 redis:latest");

    // JSON 配置示例 - 使用简短类型名
    let json_config = r#"{
        "type": "RedisStore<String, String>",
        "options": {
            "endpoint": "localhost:6379",
            "password": "",
            "db": 0,
            "default_ttl": 3600
        }
    }"#;

    let type_options = TypeOptions::from_json(&json_config)?;
    println!("🔍 使用的类型名: {}", type_options.type_name);
    let store_obj = create_from_type_options(&type_options)?;

    if let Some(store) = store_obj.downcast_ref::<RedisStore<String, String>>() {
        println!("✅ JSON配置创建 RedisStore 成功");

        // 测试基本操作
        println!("\n=== 测试基本操作 ===");
        store
            .set("user:1".to_string(), "Alice".to_string(), SetOptions::new())
            .await?;
        store
            .set("user:2".to_string(), "Bob".to_string(), SetOptions::new())
            .await?;

        let user1 = store.get("user:1".to_string()).await?;
        let user2 = store.get("user:2".to_string()).await?;
        println!("👤 user:1 = {}", user1);
        println!("👤 user:2 = {}", user2);

        // 测试条件设置
        println!("\n=== 测试 if_not_exist 条件 ===");
        let result = store
            .set(
                "user:1".to_string(),
                "Charlie".to_string(),
                SetOptions::new().with_if_not_exist(),
            )
            .await;

        match result {
            Err(_) => println!("🚫 user:1 已存在，条件设置失败（符合预期）"),
            Ok(_) => println!("⚠️  user:1 不存在时才能设置，但设置成功了？"),
        }

        let unchanged_user = store.get("user:1".to_string()).await?;
        println!("🔄 user:1 值未改变: {}", unchanged_user);

        // 测试 TTL 设置
        println!("\n=== 测试过期时间设置 ===");
        use std::time::Duration;
        store
            .set(
                "temp:session".to_string(),
                "temporary_data".to_string(),
                SetOptions::new().with_expiration(Duration::from_secs(60)),
            )
            .await?;
        println!("⏰ 设置 temp:session，过期时间=60秒");
        let session = store.get("temp:session".to_string()).await?;
        println!("📦 temp:session = {}", session);

        // 测试批量操作
        println!("\n=== 测试批量操作 ===");
        let keys = vec![
            "batch:1".to_string(),
            "batch:2".to_string(),
            "batch:3".to_string(),
            "batch:4".to_string(),
            "batch:5".to_string(),
        ];
        let values = vec![
            "value1".to_string(),
            "value2".to_string(),
            "value3".to_string(),
            "value4".to_string(),
            "value5".to_string(),
        ];

        let batch_results = store
            .batch_set(keys.clone(), values, SetOptions::new())
            .await?;
        println!("📝 批量设置 {} 个键成功", batch_results.len());

        let (batch_values, batch_errors) = store.batch_get(keys.clone()).await?;
        println!("📖 批量获取 {} 个值", batch_values.len());
        for (key, value) in keys.iter().zip(batch_values.iter()) {
            println!("  {} = {}", key, value.as_ref().unwrap());
        }
        if !batch_errors.is_empty() {
            println!("⚠️  批量获取错误: {:?}", batch_errors);
        }

        // 测试批量删除
        println!("\n=== 测试批量删除 ===");
        let del_results = store.batch_del(keys.clone()).await?;
        println!("🗑️  批量删除 {} 个键成功", del_results.len());

        // 验证删除结果
        let (empty_values, not_found_errors) = store.batch_get(keys).await?;
        println!("🔍 删除后获取: {} 个值", empty_values.len());
        println!("❌ 删除后错误: {} 个", not_found_errors.len());

        // 测试性能对比示例
        println!("\n=== 性能测试示例 ===");
        let test_count = 1000;

        let start = std::time::Instant::now();
        for i in 0..test_count {
            store
                .set(
                    format!("perf:key:{}", i),
                    format!("perf:value:{}", i),
                    SetOptions::new(),
                )
                .await?;
        }
        let set_duration = start.elapsed();
        println!(
            "⚡ 设置 {} 个键值对耗时: {:?} ({:.2} ops/sec)",
            test_count,
            set_duration,
            test_count as f64 / set_duration.as_secs_f64()
        );

        let start = std::time::Instant::now();
        for i in 0..test_count {
            let _ = store.get(format!("perf:key:{}", i)).await?;
        }
        let get_duration = start.elapsed();
        println!(
            "🔍 获取 {} 个键值对耗时: {:?} ({:.2} ops/sec)",
            test_count,
            get_duration,
            test_count as f64 / get_duration.as_secs_f64()
        );

        // 清理测试数据
        println!("\n=== 清理测试数据 ===");
        store.close().await?;
        println!("🧹 Redis 连接已关闭");
    }

    println!("\n🎉 RedisStore JSON 配置示例完成!");
    println!("💡 提示：Redis multiplexed 连接会自动管理连接池，无需手动配置");

    Ok(())
}
