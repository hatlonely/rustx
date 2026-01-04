use rustx::kv::store::{MapStore, Store, SetOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MapStore 示例 ===");

    // 创建 MapStore 实例
    let store = MapStore::<String, i32>::new();
    
    println!("\n1. 基本操作：Set, Get, Del");
    
    // 设置键值对
    store.set("age".to_string(), 25, SetOptions::new()).await?;
    store.set("score".to_string(), 95, SetOptions::new()).await?;
    
    // 获取值
    let age = store.get("age".to_string()).await?;
    let score = store.get("score".to_string()).await?;
    println!("age: {}, score: {}", age, score);
    
    // 删除键
    store.del("score".to_string()).await?;
    
    // 尝试获取已删除的键
    match store.get("score".to_string()).await {
        Ok(value) => println!("score: {}", value),
        Err(e) => println!("获取 score 失败: {}", e),
    }
    
    println!("\n2. 条件设置：if_not_exist");
    
    // 使用 if_not_exist 选项
    let result1 = store.set("age".to_string(), 30, SetOptions::new().with_if_not_exist()).await;
    match result1 {
        Ok(_) => println!("设置 age = 30 成功"),
        Err(e) => println!("设置 age = 30 失败: {}", e),
    }
    
    // 验证值没有被修改
    let current_age = store.get("age".to_string()).await?;
    println!("当前 age: {}", current_age);
    
    // 设置新键
    store.set("height".to_string(), 175, SetOptions::new().with_if_not_exist()).await?;
    let height = store.get("height".to_string()).await?;
    println!("新设置的 height: {}", height);
    
    println!("\n3. 批量操作");
    
    // 批量设置
    let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
    let values = vec![100, 200, 300];
    
    let batch_results = store.batch_set(keys.clone(), values, SetOptions::new()).await?;
    println!("批量设置结果: {:?}", batch_results);
    
    // 批量获取
    let (batch_values, batch_errors) = store.batch_get(keys.clone()).await?;
    println!("批量获取值: {:?}", batch_values);
    println!("批量获取错误: {:?}", batch_errors);
    
    // 批量删除
    let del_results = store.batch_del(keys.clone()).await?;
    println!("批量删除结果: {:?}", del_results);
    
    // 验证删除
    let (empty_values, not_found_errors) = store.batch_get(keys).await?;
    println!("删除后的值: {:?}", empty_values);
    println!("删除后的错误: {:?}", not_found_errors);
    
    println!("\n4. 关闭 Store");
    store.close().await?;
    println!("Store 已关闭");
    
    Ok(())
}