//! ConfigSource 注册使用示例
//!
//! 展示如何使用 register_sources 注册所有配置源，并通过配置创建不同的配置源实例

use rustx::cfg::{register_sources, ConfigSource, TypeOptions, create_trait_from_type_options};
use serde::Deserialize;

/// 数据库配置
#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
}

fn main() -> anyhow::Result<()> {
    // 1. 注册所有配置源
    register_sources()?;
    println!("✓ 已注册所有配置源");

    // 2. 通过配置创建 FileSource
    println!("\n=== 创建 FileSource ===");
    let file_opts = TypeOptions::from_json(r#"{
        "type": "FileSource",
        "options": {
            "base_path": "examples/configs/cfg"
        }
    }"#)?;

    let file_source: Box<dyn ConfigSource> = create_trait_from_type_options(&file_opts)?;
    println!("✓ 创建 FileSource 成功");

    // 使用 FileSource 加载配置
    match file_source.load("database") {
        Ok(config_value) => {
            match config_value.into_type::<DatabaseConfig>() {
                Ok(db_config) => {
                    println!("  - 加载配置成功: host={}, port={}", db_config.host, db_config.port);
                }
                Err(e) => {
                    println!("  - 解析配置失败: {}", e);
                }
            }
        }
        Err(e) => {
            println!("  - 加载配置失败: {}", e);
        }
    }

    // 3. 通过配置创建 ApolloSource（需要实际的 Apollo 服务器）
    println!("\n=== 创建 ApolloSource ===");
    let apollo_opts = TypeOptions::from_json(r#"{
        "type": "ApolloSource",
        "options": {
            "server_url": "http://localhost:8080",
            "app_id": "sample-app",
            "namespace": "application",
            "cluster": "default"
        }
    }"#)?;

    let apollo_source: Box<dyn ConfigSource> = create_trait_from_type_options(&apollo_opts)?;
    println!("✓ 创建 ApolloSource 成功");

    // 尝试加载配置（如果没有实际的服务器会失败）
    match apollo_source.load("database") {
        Ok(_) => {
            println!("  - 加载 Apollo 配置成功");
        }
        Err(e) => {
            println!("  - 加载 Apollo 配置失败（预期行为，需要实际的 Apollo 服务器）");
            println!("    错误: {}", e);
        }
    }

    // 4. 展示如何根据环境选择不同的配置源
    println!("\n=== 根据环境选择配置源 ===");
    let env = std::env::var("ENV").unwrap_or_else(|_| "dev".to_string());

    let source_opts = if env == "production" {
        TypeOptions::from_json(r#"{
            "type": "ApolloSource",
            "options": {
                "server_url": "http://apollo.prod:8080",
                "app_id": "my-app"
            }
        }"#)?
    } else {
        TypeOptions::from_json(r#"{
            "type": "FileSource",
            "options": {
                "base_path": "examples/configs/cfg"
            }
        }"#)?
    };

    let _dynamic_source: Box<dyn ConfigSource> = create_trait_from_type_options(&source_opts)?;
    println!("✓ 根据环境 '{}' 创建配置源成功", env);

    Ok(())
}
