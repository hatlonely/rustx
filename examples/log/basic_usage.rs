use rustx::log::*;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 从 JSON 构建 LoggerConfig - 使用文本格式 + 终端输出
    let config: LoggerConfig = json5::from_str(
        r#"
        {
            "level": "info",
            "formatter": {
                "type": "TextFormatter",
                "options": {
                    "colored": false
                }
            },
            "appender": {
                "type": "ConsoleAppender",
                "options": {
                    "use_colors": true
                }
            }
        }
        "#,
    )?;

    // 2. 创建 Logger
    let logger = Logger::new(config)?;

    // 3. 使用 Logger
    logger
        .info("Application started".to_string())
        .await?;

    logger
        .debug("Debug information".to_string())
        .await?;

    logger
        .warn("Warning message".to_string())
        .await?;

    logger
        .error("Error occurred".to_string())
        .await?;

    // 4. 动态切换日志级别
    println!("\n=== Switching to DEBUG level ===\n");
    logger.set_level(LogLevel::Debug).await;

    logger
        .debug("Now debug messages are visible".to_string())
        .await?;

    Ok(())
}
