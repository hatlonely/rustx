// cfg 模块序列化示例

use anyhow::Result;
use rustx::cfg::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
    database: String,
    username: String,
    password: String,
}

fn main() -> Result<()> {
    println!("=== cfg 序列化示例 ===");

    let config = DatabaseConfig {
        host: "localhost".to_string(),
        port: 5432,
        database: "myapp".to_string(),
        username: "user".to_string(),
        password: "password".to_string(),
    };

    let type_options = TypeOptions {
        type_name: "database".to_string(),
        options: serde_json::to_value(config)?,
    };

    // JSON 格式
    println!("\nJSON 格式:");
    println!("{}", type_options.to_json()?);

    // YAML 格式  
    println!("\nYAML 格式:");
    println!("{}", type_options.to_yaml()?);

    // TOML 格式
    println!("\nTOML 格式:");
    println!("{}", type_options.to_toml()?);

    Ok(())
}