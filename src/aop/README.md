# AOP (Aspect-Oriented Programming)

提供切面编程支持，为方法调用添加日志记录和自动重试功能。

## 快速开始

```rust
use anyhow::Result;
use rustx::aop::{Aop, AopConfig};
use serde::Deserialize;

// 用户服务配置
#[derive(Debug, Clone, Deserialize)]
struct UserServiceConfig {
    pub aop: Option<AopConfig>,
}

// 用户服务
pub struct UserService {
    client: DatabaseClient,
    aop: Option<Aop>,
}

impl UserService {
    pub fn new(config: UserServiceConfig) -> Result<Self> {
        let aop = config.aop.map(Aop::new).transpose()?;
        Ok(Self {
            client: DatabaseClient::new(),
            aop,
        })
    }

    // 异步方法 - 使用 aop! 宏
    pub async fn get_user(&self, id: &str) -> Result<String> {
        rustx::aop!(&self.aop, self.client.query(id).await)
    }

    // 同步方法 - 使用 aop_sync! 宏
    pub fn get_user_sync(&self, id: &str) -> Result<String> {
        rustx::aop_sync!(&self.aop, self.client.query_sync(id))
    }
}

// 使用示例
let config: UserServiceConfig = json5::from_str(r#"
{
    aop: {
        logging: {
            logger: {
                level: "info",
                formatter: { type: "TextFormatter" },
                appender: { type: "ConsoleAppender" }
            },
            info_sample_rate: 1.0,   // 成功日志采样率
            warn_sample_rate: 1.0    // 失败日志采样率
        },
        retry: {
            max_times: 3,            // 最大重试次数
            strategy: "exponential", // 退避策略: constant/exponential/fibonacci
            min_delay: "100ms",      // 最小延迟
            max_delay: "2s",         // 最大延迟
            jitter: true             // 是否添加随机抖动
        }
    }
}
"#)?;

let service = UserService::new(config)?;
service.get_user("123").await?;
service.get_user_sync("456")?;
```

## 配置说明

### AopConfig

AOP 主配置，包含 logging 和 retry 两个可选部分：

```json5
{
    aop: {
        // Logging 配置（可选）
        logging: {
            // Logger 配置（完整配置参见 log 模块文档）
            logger: {
                level: "info",
                formatter: {
                    type: "TextFormatter",
                    options: {
                        colored: false,
                        display_metadata: true
                    }
                },
                appender: {
                    type: "ConsoleAppender",
                    options: { target: "stdout" }
                }
            },
            // 成功日志的采样率（0.0 - 1.0），默认 1.0
            // 生产环境可设置为 0.01 - 0.1 以降低日志量
            info_sample_rate: 1.0,

            // 失败日志的采样率（0.0 - 1.0），默认 1.0
            // 生产环境可设置为 0.1 - 0.5 以降低日志量
            warn_sample_rate: 1.0
        },

        // Retry 配置（可选）
        retry: {
            // 最大重试次数（1 - 100），默认 3
            max_times: 3,

            // 退避策略: "constant" | "exponential" | "fibonacci"
            // - constant: 固定延迟，适合稳定的重试场景
            // - exponential: 指数退避，适合高负载服务
            // - fibonacci: 斐波那契退避，比指数更平滑
            strategy: "exponential",

            // 固定延迟，用于 constant 策略（支持 "100ms", "1s", "1m" 等格式）
            delay: "200ms",

            // 最小延迟，用于 exponential/fibonacci 策略
            min_delay: "100ms",

            // 最大延迟，用于 exponential/fibonacci 策略
            max_delay: "2s",

            // 退避因子（用于 exponential 策略），默认 2.0
            factor: 2.0,

            // 是否添加随机抖动（避免多个客户端同时重试造成惊群效应）
            jitter: true
        }
    }
}
```

### LoggingConfig

```json5
{
    logging: {
        // Logger 配置（完整配置参见 log 模块文档）
        logger: {
            level: "info",
            formatter: { type: "TextFormatter" },
            appender: { type: "ConsoleAppender" }
        },

        // 成功日志的采样率（0.0 - 1.0），默认 1.0
        // 生产环境可设置为 0.01 - 0.1 以降低日志量
        info_sample_rate: 1.0,

        // 失败日志的采样率（0.0 - 1.0），默认 1.0
        // 生产环境可设置为 0.1 - 0.5 以降低日志量
        warn_sample_rate: 1.0
    }
}
```

### RetryConfig

```json5
{
    retry: {
        // 最大重试次数（1 - 100），默认 3
        max_times: 3,

        // 退避策略: "constant" | "exponential" | "fibonacci"
        // - constant: 固定延迟，适合稳定的重试场景
        // - exponential: 指数退避，适合高负载服务
        // - fibonacci: 斐波那契退避，比指数更平滑
        strategy: "constant",

        // 固定延迟，用于 constant 策略（支持 "100ms", "1s", "1m" 等格式）
        delay: "100ms",

        // 最小延迟，用于 exponential/fibonacci 策略
        min_delay: "100ms",

        // 最大延迟，用于 exponential/fibonacci 策略
        max_delay: "2s",

        // 退避因子（用于 exponential 策略），默认 2.0
        factor: 2.0,

        // 是否添加随机抖动（默认 false）
        // 设置为 true 可避免多个客户端同时重试造成惊群效应
        jitter: false
    }
}
```

## 使用建议

### 生产环境推荐配置

```json5
{
    aop: {
        logging: {
            info_sample_rate: 0.01,  // 1% 成功日志
            warn_sample_rate: 0.1    // 10% 失败日志
        },
        retry: {
            max_times: 5,
            strategy: "fibonacci",
            jitter: true  // 避免惊群效应
        }
    }
}
```

### 策略选择

- **Constant**: 低延迟场景，固定延迟更可控
- **Exponential**: 高负载服务，延迟快速增长
- **Fibonacci**: 生产环境推荐，平滑增长且配合 jitter 避免惊群效应
