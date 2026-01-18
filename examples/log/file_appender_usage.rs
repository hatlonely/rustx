use anyhow::Result;
use rustx::log::*;

#[tokio::main]
async fn main() -> Result<()> {
    // 注册所有组件
    register_log_components()?;

    // 从 JSON 构建 LoggerConfig - 使用 JSON 格式 + 文件输出
    let config: LoggerConfig = json5::from_str(
        r#"
        {
            "level": "debug",
            "formatter": {
                "type": "JsonFormatter",
                "options": {}
            },
            "appender": {
                "type": "FileAppender",
                "options": {
                    "file_path": "/tmp/rustx_example.log"
                }
            }
        }
        "#,
    )?;

    let logger = Logger::new(config)?;

    // 记录不同级别的日志
    logger.trace("Trace message".to_string()).await?;

    logger.debug("Debug message".to_string()).await?;

    logger.info("Info message".to_string()).await?;

    logger.warn("Warning message".to_string()).await?;

    logger.error("Error message".to_string()).await?;

    println!("Logs have been written to /tmp/rustx_example.log");

    // 读取并显示文件内容
    let contents = tokio::fs::read_to_string("/tmp/rustx_example.log").await?;
    println!("\n=== File contents ===");
    println!("{}", contents);

    Ok(())
}
