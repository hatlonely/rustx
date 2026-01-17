use anyhow::Result;
use rustx::cfg::{
    create_trait_from_type_options, ConfigSource, FileSource, FileSourceConfig, TypeOptions,
};
use rustx::kv::store::{register_hash_stores, SetOptions, Store};

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== register_stores + FileSource 配置示例 ===\n");

    // 1. 注册所有 Store 实现
    println!("1. 注册 Store 实现");
    register_hash_stores::<String, String>()?;
    println!("   已注册 HashMapStore 和 SafeHashMapStore\n");

    // 2. 使用 FileSource 加载配置
    println!("2. 从文件加载 Store 配置");
    let source = FileSource::new(FileSourceConfig {
        base_path: "examples/kv/store/configs".to_string(),
    });

    let type_options: TypeOptions = source.load("safe_hash_map_store")?.into_type()?;
    println!("   类型: {}", type_options.type_name);
    println!("   配置: {}\n", type_options.options);

    // 3. 使用 create_trait_from_type_options 创建 Store
    println!("3. 创建 Store 实例");
    let store: Box<dyn Store<String, String>> = create_trait_from_type_options(&type_options)?;
    println!("   Store 创建成功\n");

    // 4. 测试基本操作
    println!("=== 测试基本操作 ===");
    store
        .set("key1".to_string(), "val1".to_string(), SetOptions::new())
        .await?;
    store
        .set("key2".to_string(), "val2".to_string(), SetOptions::new())
        .await?;

    let val1 = store.get("key1".to_string()).await?;
    let val2 = store.get("key2".to_string()).await?;
    println!("key1 value: {}", val1);
    println!("key2 value: {}", val2);

    // 5. 测试条件设置
    println!("\n=== 测试 if_not_exist 条件 ===");
    let result = store
        .set(
            "key1".to_string(),
            "new_val1".to_string(),
            SetOptions::new().with_if_not_exist(),
        )
        .await;

    match result {
        Err(_) => println!("key1 已存在，条件设置失败（符合预期）"),
        Ok(_) => println!("key1 不存在时才能设置，但设置成功了？"),
    }

    let unchanged_val = store.get("key1".to_string()).await?;
    println!("key1 值未改变: {}", unchanged_val);

    // 6. 测试批量操作
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
    println!("批量设置结果: {:?}", batch_results);

    let (batch_values, batch_errors) = store.batch_get(keys.clone()).await?;
    println!("批量获取值: {:?}", batch_values);
    println!("批量获取错误: {:?}", batch_errors);

    // 7. 测试批量删除
    println!("\n=== 测试批量删除 ===");
    let del_results = store.batch_del(keys.clone()).await?;
    println!("批量删除结果: {:?}", del_results);

    // 验证删除结果
    let (empty_values, not_found_errors) = store.batch_get(keys).await?;
    println!("删除后获取值: {:?}", empty_values);
    println!("删除后获取错误: {:?}", not_found_errors);

    // 8. 性能测试
    println!("\n=== 性能测试 ===");
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
    println!("设置 10000 个键值对耗时: {:?}", set_duration);

    let start = std::time::Instant::now();
    for i in 0..10000 {
        let _ = store.get(format!("perf_key_{}", i)).await?;
    }
    let get_duration = start.elapsed();
    println!("获取 10000 个键值对耗时: {:?}", get_duration);

    // 9. 清理
    store.close().await?;
    println!("\n存储已关闭");

    println!("\n=== 示例完成 ===");
    println!("提示: 修改 examples/kv/store/configs/store.json 中的 type 字段");
    println!("      可切换为 \"HashMapStore\" 使用非线程安全版本");

    Ok(())
}
