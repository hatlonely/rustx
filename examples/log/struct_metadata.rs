//! 使用结构体作为 Metadata 的示例
//!
//! 演示如何使用自定义结构体作为日志的 metadata

use rustx::log::*;
use anyhow::Result;
use rustx::ginfo;
use serde::Serialize;

// 定义用户信息结构体
#[derive(Serialize)]
struct UserInfo {
    user_id: i64,
    username: String,
    email: String,
    role: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== 使用结构体作为 Metadata 示例 ===\n");

    // 创建用户信息
    let user = UserInfo {
        user_id: 12345,
        username: "alice".to_string(),
        email: "alice@example.com".to_string(),
        role: "admin".to_string(),
    };

    // 使用 ginfo! 记录日志，传入结构体作为 metadata
    ginfo!(
        "user logged in",
        "success" => true,
        "user" => MetadataValue::from_struct(user)
    );

    println!("\n提示：查看上方的输出，结构体已被序列化到 metadata 中");

    Ok(())
}
