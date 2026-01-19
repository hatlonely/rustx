# Logger

灵活的日志模块，支持多种日志级别、格式和输出方式。

## 快速开始

### 1. 初始化全局 LoggerManager

```rust
use rustx::log::*;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // 从配置初始化全局 logger manager
    let config: LoggerManagerConfig = json5::from_str(r#"
        {
            default: {
                level: "info",
                formatter: {
                    type: "TextFormatter",
                    options: {}
                },
                appender: {
                    type: "ConsoleAppender",
                    options: {}
                }
            },
            loggers: {
                "db": {
                    level: "debug",
                    formatter: {
                        type: "JsonFormatter",
                        options: {}
                    },
                    appender: {
                        type: "FileAppender",
                        options: {
                            file_path: "/tmp/db.log"
                        }
                    }
                }
            }
        }
    "#)?;

    init_logger_manager(config)?;
    Ok(())
}
```

### 2. 使用 Logger

```rust
// 获取命名 logger
let db_logger = get_logger("db").expect("logger not found");
db_logger.info("database connected".to_string()).await?;

// 获取默认 logger
let logger = get_default_logger();
logger.warn("high memory usage".to_string()).await?;

// 使用 info/trace 等宏（自动捕获文件和行号）
info!(logger, "user logged in");
error!(logger, "connection failed", "host" => "localhost", "port" => 5432);
```

### 3. 使用全局默认 logger

直接使用 `ginfo`/`gtrace` 等宏，无需传递 logger 参数：

```rust
// 简单日志
ginfo!("application started");
gwarn!("high memory usage");
gerror!("database connection failed");

// 带 metadata 的日志
ginfo!("user logged in", "user_id" => 12345, "username" => "alice");
gdebug!("processing request", "endpoint" => "/api/users", "method" => "GET");
```

## 日志级别

支持 5 个日志级别：`Trace` < `Debug` < `Info` < `Warn` < `Error`

## 配置项

### LoggerConfig

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| level | String | "info" | 日志级别 |
| formatter | TypeOptions | TextFormatter | 格式化器 |
| appender | TypeOptions | ConsoleAppender | 输出目标 |

### Formatter

- `TextFormatter` - 文本格式（支持彩色输出）
- `JsonFormatter` - JSON 格式

### Appender

- `ConsoleAppender` - 输出到终端（stdout/stderr）
- `FileAppender` - 输出到文件
