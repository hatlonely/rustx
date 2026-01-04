use anyhow::Result;
use rustx::kv::store::{MapStore, MapStoreConfig, Store, SetOptions};
use rustx::cfg::*;

#[tokio::main]
async fn main() -> Result<()> {
    // 零耦合自动注册！MapStore 完全不需要知道配置系统的存在
    register_auto_with_type::<MapStore<String, String>, MapStoreConfig>()?;
    register_auto_with_type::<MapStore<String, i32>, MapStoreConfig>()?;
    register_auto_with_type::<MapStore<String, i64>, MapStoreConfig>()?;

    println!("=== MapStore 配置示例 ===");

    // 获取实际的类型名
    use rustx::cfg::registry::generate_auto_type_name;
    let actual_type_name = generate_auto_type_name::<MapStore<String, String>>();
    println!("🔧 实际类型名: {}", actual_type_name);

    // JSON 配置示例 - 使用实际的类型名
    let json_config = format!(r#"
    {{
        "type": "{}",
        "options": {{
            "initial_capacity": 1000,
            "enable_stats": true
        }}
    }}"#, actual_type_name);

    let type_options = TypeOptions::from_json(&json_config)?;
    println!("🔍 使用的类型名: {}", type_options.type_name);
    let store_obj = create_from_type_options(&type_options)?;

    if let Some(store) = store_obj.downcast_ref::<MapStore<String, String>>() {
        println!("✅ JSON配置创建MapStore成功");
        
        // 测试基本操作
        store.set("name".to_string(), "rustx".to_string(), SetOptions::new()).await?;
        store.set("version".to_string(), "0.1.0".to_string(), SetOptions::new()).await?;
        
        let name = store.get("name".to_string()).await?;
        let version = store.get("version".to_string()).await?;
        println!("📦 项目名称: {}", name);
        println!("🔖 项目版本: {}", version);

        // 测试批量操作
        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let values = vec!["value1".to_string(), "value2".to_string(), "value3".to_string()];
        
        let batch_results = store.batch_set(keys.clone(), values, SetOptions::new()).await?;
        println!("📝 批量设置结果: {:?}", batch_results);
        
        let (batch_values, batch_errors) = store.batch_get(keys).await?;
        println!("📖 批量获取值: {:?}", batch_values);
        println!("⚠️  批量获取错误: {:?}", batch_errors);
    }

    // YAML 配置示例 - 使用实际的类型名
    let yaml_config = format!(r#"
type: {}
options:
  initial_capacity: 500
  enable_stats: false
"#, actual_type_name);

    let yaml_type_options = TypeOptions::from_yaml(&yaml_config)?;
    let yaml_store_obj = create_from_type_options(&yaml_type_options)?;

    if let Some(yaml_store) = yaml_store_obj.downcast_ref::<MapStore<String, String>>() {
        println!("✅ YAML配置创建MapStore成功");
        
        yaml_store.set("config_type".to_string(), "yaml".to_string(), SetOptions::new()).await?;
        let config_type = yaml_store.get("config_type".to_string()).await?;
        println!("⚙️  配置类型: {}", config_type);
        
        // 测试条件设置
        let result = yaml_store.set("config_type".to_string(), "json".to_string(), SetOptions::new().with_if_not_exist()).await;
        match result {
            Ok(_) => println!("❌ 条件设置应该失败"),
            Err(e) => println!("✅ 条件设置正确失败: {}", e),
        }
    }

    // 测试不同类型组合的 MapStore
    let i32_type_name = generate_auto_type_name::<MapStore<String, i32>>();
    let int_config = format!(r#"
    {{
        "type": "{}",
        "options": {{
            "initial_capacity": 200,
            "enable_stats": true
        }}
    }}"#, i32_type_name);

    let int_type_options = TypeOptions::from_json(&int_config)?;
    let int_store_obj = create_from_type_options(&int_type_options)?;

    if let Some(int_store) = int_store_obj.downcast_ref::<MapStore<String, i32>>() {
        println!("✅ 创建 MapStore<String, i32> 成功");
        
        int_store.set("count".to_string(), 42, SetOptions::new()).await?;
        int_store.set("max_value".to_string(), 100, SetOptions::new()).await?;
        
        let count = int_store.get("count".to_string()).await?;
        let max_value = int_store.get("max_value".to_string()).await?;
        println!("🔢 计数: {}, 最大值: {}", count, max_value);
    }

    println!("\n🎉 MapStore 配置示例完成!");
    
    Ok(())
}