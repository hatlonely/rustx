//! 初始化全局 LoggerManager 使用示例
//!
//! 演示如何通过配置初始化全局 LoggerManager，然后使用全局 logger

use anyhow::Result;
use rustx::log::*;
// 宏定义在 crate root，需要单独导入
use rustx::{debug, error, ginfo, gwarn, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== 初始化全局 LoggerManager 示例 ===\n");

    // 1. 创建 LoggerManager 配置
    let config: LoggerManagerConfig = json5::from_str(
        r#"
        {
            default: {
                level: "info",
                formatter: {
                    type: "TextFormatter",
                    options: {
                        colored: false
                    }
                },
                appender: {
                    type: "ConsoleAppender",
                    options: {
                        target: "stdout",
                        auto_flush: true
                    }
                }
            },
            loggers: {
                "database": {
                    level: "debug",
                    formatter: {
                        type: "JsonFormatter",
                        options: {}
                    },
                    appender: {
                        type: "FileAppender",
                        options: {
                            file_path: "/tmp/rustx_database.log"
                        }
                    }
                },
                "api": {
                    level: "info",
                    formatter: {
                        type: "TextFormatter",
                        options: {
                            colored: true
                        }
                    },
                    appender: {
                        type: "ConsoleAppender",
                        options: {
                            target: "stdout",
                            auto_flush: true
                        }
                    }
                }
            }
        }
        "#,
    )?;

    // 2. 初始化全局 LoggerManager
    println!("2. 初始化全局 LoggerManager");
    ::rustx::log::init(config)?;

    // 3. 使用全局默认 logger
    println!("\n3. 使用全局默认 logger:");
    ginfo!("application started successfully");
    ginfo!(
        "server listening",
        "host" => "0.0.0.0",
        "port" => 8080
    );

    // 4. 获取并使用命名 logger
    println!("\n4. 获取并使用命名 logger:");
    let db_logger = get("database").expect("database logger not found");
    info!(db_logger, "database connected");
    info!(
        db_logger,
        "executed query",
        "sql" => "SELECT * FROM users",
        "rows" => 100
    );

    let api_logger = get("api").expect("api logger not found");
    info!(api_logger, "API request received", "path" => "/api/users");
    warn!(
        api_logger,
        "slow API request",
        "path" => "/api/posts",
        "duration_ms" => 1500
    );

    // 5. 使用宏与命名 logger
    println!("\n5. 使用宏与命名 logger:");
    debug!(
        db_logger,
        "query details",
        "query" => "SELECT * FROM orders WHERE user_id = ?",
        "params" => "[12345]",
        "execution_time_ms" => 25
    );

    error!(
        db_logger,
        "database error",
        "error_code" => "DUPLICATE_KEY",
        "table" => "users",
        "constraint" => "unique_email"
    );

    // 6. 继续使用全局默认 logger
    println!("\n6. 继续使用全局默认 logger:");
    ginfo!("request processed successfully");
    gwarn!(
        "high response time detected",
        "endpoint" => "/api/search",
        "duration_ms" => 2300
    );

    // 7. 动态添加新的 logger
    println!("\n7. 动态添加新的 logger:");
    let audit_config: LoggerCreateConfig = json5::from_str(
        r#"
        {
            level: "info",
            formatter: {
                type: "JsonFormatter",
                options: {}
            },
            appender: {
                type: "FileAppender",
                options: {
                    file_path: "/tmp/rustx_audit.log"
                }
            }
        }
        "#,
    )?;

    let audit_logger = Logger::new(audit_config)?;
    add("audit".to_string(), audit_logger);

    let audit = get("audit").expect("audit logger not found");
    info!(
        audit,
        "user action",
        "action" => "update_profile",
        "user_id" => 12345,
        "ip" => "192.168.1.100"
    );

    println!("\n=== 日志已写入以下文件 ===");
    println!("- /tmp/rustx_database.log (JSON 格式)");
    println!("- /tmp/rustx_audit.log (JSON 格式)");

    Ok(())
}
