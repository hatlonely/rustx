//! 全局默认 Logger 使用示例
//!
//! 演示如何直接使用全局默认 logger，无需任何初始化

use rustx::log::*;
// 宏定义在 crate root，需要单独导入
use rustx::{gtrace, gdebug, ginfo, gwarn, gerror};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== 全局默认 Logger 使用示例 ===\n");

    // 全局默认 logger 无需初始化即可使用
    // 默认配置：TextFormatter + ConsoleAppender (stdout)

    // 1. 使用 ginfo!/gdebug! 等宏直接记录日志
    println!("1. 使用全局宏记录简单日志:");
    gtrace!("trace message - 通常不会被显示");
    gdebug!("debug message - 调试信息");
    ginfo!("info message - 一般信息");
    gwarn!("warn message - 警告信息");
    gerror!("error message - 错误信息");

    // 2. 使用带 metadata 的宏
    println!("\n2. 使用全局宏记录带 metadata 的日志:");
    ginfo!(
        "user logged in",
        "user_id" => 12345,
        "username" => "alice"
    );

    gdebug!(
        "processing request",
        "endpoint" => "/api/users",
        "method" => "GET",
        "duration_ms" => 150
    );

    gwarn!(
        "high memory usage",
        "usage_mb" => 512,
        "threshold_mb" => 400
    );

    gerror!(
        "database connection failed",
        "host" => "localhost",
        "port" => 5432,
        "error_code" => "CONN001"
    );

    // 3. 也可以直接使用函数
    println!("\n3. 使用全局函数记录日志:");
    info("application started").await?;
    warn("configuration file not found, using defaults").await?;

    Ok(())
}
