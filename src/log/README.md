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

### 4. 使用结构体作为 Metadata

支持将自定义结构体作为 metadata 传入日志：

```rust
use rustx::log::*;
use rustx::ginfo;
use serde::Serialize;

#[derive(Serialize)]
struct UserInfo {
    user_id: i64,
    username: String,
    email: String,
    role: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let user = UserInfo {
        user_id: 12345,
        username: "alice".to_string(),
        email: "alice@example.com".to_string(),
        role: "admin".to_string(),
    };

    // 结构体会被自动序列化为 JSON
    ginfo!(
        "user logged in",
        "success" => true,
        "user" => MetadataValue::from_struct(user)
    );

    Ok(())
}
```

## 日志级别

支持 5 个日志级别：`Trace` < `Debug` < `Info` < `Warn` < `Error`

## Logger 配置选项

### TextFormatter - 文本格式化器

将日志记录格式化为可读的文本格式，支持彩色输出。

```json5
{
    // Formatter 类型，固定为 "TextFormatter"
    "type": "TextFormatter",
    "options": {
        // 是否启用彩色输出，可选，默认 false
        // true: 使用 ANSI 颜色代码美化输出（适合终端）
        // false: 纯文本输出（适合日志文件）
        "colored": false
    }
}
```

**输出示例：**
```
[2025-01-19T12:34:56.789Z] [ThreadId(1)] INFO [main.rs:42] user logged in | user_id=12345 username=alice success=true
```

### JsonFormatter - JSON 格式化器

将日志记录格式化为 JSON 格式，适合日志收集系统。

```json5
{
    // Formatter 类型，固定为 "JsonFormatter"
    "type": "JsonFormatter",
    "options": {
        // JsonFormatter 暂无配置选项
    }
}
```

**输出示例：**
```json
{
  "timestamp": 1737278896789,
  "level": "INFO",
  "message": "user logged in",
  "file": "main.rs",
  "line": 42,
  "thread_id": "ThreadId(1)",
  "metadata": {
    "user_id": 12345,
    "username": "alice",
    "success": true
  }
}
```

### ConsoleAppender - 终端输出器

将日志输出到标准输出或标准错误。

```json5
{
    // Appender 类型，固定为 "ConsoleAppender"
    "type": "ConsoleAppender",
    "options": {
        // 输出目标，可选，默认 "stdout"
        // "stdout": 输出到标准输出
        // "stderr": 输出到标准错误
        "target": "stdout",

        // 是否自动刷新缓冲区，可选，默认 true
        // true: 每次写入后立即刷新（确保日志实时输出）
        // false: 由系统决定刷新时机（性能更好但可能延迟）
        "auto_flush": true
    }
}
```

### FileAppender - 文件输出器

将日志输出到文件，支持自动创建目录。

```json5
{
    // Appender 类型，固定为 "FileAppender"
    "type": "FileAppender",
    "options": {
        // 日志文件路径（必需）
        // 支持相对路径和绝对路径
        // 如果父目录不存在会自动创建
        "file_path": "/var/log/app.log"
    }
}
```

**特性：**
- 自动创建父目录
- 追加模式写入（不会覆盖已有日志）
- 每条日志后自动换行
