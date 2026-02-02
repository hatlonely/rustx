# AOP (Aspect-Oriented Programming)

提供切面编程支持，为方法调用添加日志记录和自动重试功能。

## 快速开始

### 基础使用

```rust
use anyhow::Result;
use rustx::aop::{Aop, AopConfig};
use serde::Deserialize;
use std::sync::Arc;

// 用户服务配置
#[derive(Debug, Clone, Deserialize)]
struct UserServiceConfig {
    pub aop: Option<AopConfig>,
}

// 用户服务
pub struct UserService {
    client: DatabaseClient,
    aop: Option<Arc<Aop>>,
}

impl UserService {
    pub fn new(config: UserServiceConfig) -> Result<Self> {
        // 使用 resolve 方法，支持 Reference 和 Create 两种模式
        let aop = config.aop.map(|config| Aop::resolve(config)).transpose()?;
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
        },
        tracing: {
            name: "user.service",    // 分布式追踪的 span 名称
            with_args: false         // 是否记录参数
        },
        metrics: {
            prefix: "user_service",  // metric 指标前缀
            labels: {                // 固定标签
                service: "user-service",
                env: "production"
            }
        }
    }
}
"#)?;

let service = UserService::new(config)?;
service.get_user("123").await?;
service.get_user_sync("456")?;
```

### 使用 AopManager 管理多个 Aop 实例

当需要在多个服务之间共享 Aop 配置时，可以使用 `AopManager`：

```rust
use rustx::aop::{AopManager, AopManagerConfig};

// 使用 json5::from_str 解析配置
let manager_config: AopManagerConfig = json5::from_str(r#"
{
    default: {
        // 默认配置为空
    },
    aops: {
        // 创建名为 "api" 的 aop（带重试）
        "api": {
            retry: {
                max_times: 5,
                strategy: "exponential",
                min_delay: "100ms",
                max_delay: "2s"
            }
        },

        // 创建名为 "database" 的 aop（带日志和重试）
        "database": {
            logging: {
                logger: {
                    level: "info",
                    formatter: { type: "TextFormatter" },
                    appender: { type: "ConsoleAppender" }
                },
                info_sample_rate: 0.1,
                warn_sample_rate: 1.0
            },
            retry: {
                max_times: 3,
                strategy: "constant",
                delay: "200ms"
            }
        },

        // 引用 "api" aop（共享同一个实例）
        "api_ref": {
            $instance: "api"
        }
    }
}
"#)?;

let manager = AopManager::new(manager_config)?;

// 使用 aop
let api_aop = manager.get("api").unwrap();
let database_aop = manager.get("database").unwrap();
let api_ref_aop = manager.get("api_ref").unwrap();

// api_ref_aop 和 api_aop 指向同一个实例
```

### 使用全局 AopManager

全局 AopManager 提供了便捷的访问函数，适合在应用的任何地方使用：

```rust
use rustx::aop::{init, get, get_or_default, get_default, add, AopManagerConfig};

// 使用 json5::from_str 解析配置并初始化全局 AopManager
let config: AopManagerConfig = json5::from_str(r#"
{
    default: {},
    aops: {
        "api": {
            retry: {
                max_times: 3,
                strategy: "constant",
                delay: "100ms"
            }
        }
    }
}
"#)?;

init(config)?;

// 在应用的任何地方使用
if let Some(api_aop) = get("api") {
    // 使用 api_aop
}

// 获取指定 aop，如果不存在则返回默认
let aop = get_or_default("some_key");

// 获取默认 aop
let default_aop = get_default();

// 动态添加 aop
use rustx::aop::{Aop, AopCreateConfig};
let new_aop_config: AopCreateConfig = json5::from_str(r#"
    {
        retry: {
            max_times: 5,
            strategy: "exponential",
            min_delay: "50ms"
        }
    }
"#)?;
let new_aop = Aop::new(new_aop_config)?;
add("new_service".to_string(), new_aop);
```

## 配置说明

### AopConfig

AOP 主配置，支持两种模式：

#### 1. Create 模式 - 创建新的 Aop 实例

```json5
{
    aop: {
        // Logging 配置（可选）
        logging: {
            logger: { /* ... */ },
            info_sample_rate: 1.0,
            warn_sample_rate: 1.0
        },

        // Retry 配置（可选）
        retry: {
            max_times: 3,
            strategy: "exponential",
            min_delay: "100ms",
            max_delay: "2s",
            jitter: true
        }
    }
}
```

#### 2. Reference 模式 - 引用已存在的 Aop 实例

```json5
{
    aop: {
        $instance: "api"  // 引用名为 "api" 的 aop 实例
    }
}
```

**使用场景：**
- 多个服务共享同一个 Aop 配置
- 避免重复创建相同配置的 Aop 实例
- 统一管理和更新 Aop 配置

**完整配置示例：**

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
        },

        // Tracing 配置（可选）
        tracing: {
            // Span 名称，默认 "aop"
            // 建议设置为服务名或模块名，便于在追踪系统中识别
            name: "database.client",

            // 是否在 span 中记录函数参数（默认 false）
            // 开启后可在 Jaeger/Zipkin 等追踪系统中查看调用参数
            // 注意：敏感数据请保持关闭，或在函数级别使用 skip() 排除
            with_args: false
        },

        // Metrics 配置（可选）
        metrics: {
            // Metric 名称前缀，默认 "aop"
            // 生成的指标包括：
            // - {prefix}_total: 总调用次数
            // - {prefix}_retry_count: 重试次数
            // - {prefix}_duration_ms: 调用耗时分布
            // - {prefix}_in_progress: 当前正在执行的请求数
            prefix: "database_client",

            // 常量标签，会应用到所有 metric
            labels: {
                service: "user-service",
                env: "production",
                version: "1.0.0",
                cluster: "us-west-1"
            }
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

### TracingConfig

```json5
{
    tracing: {
        // Span 名称（用于分布式追踪），默认 "aop"
        // 可设置为服务名或模块名，如 "database.client", "oss.service"
        name: "aop",

        // 是否在 span 中记录函数参数（默认 false）
        // 设置为 true 可在追踪系统中查看函数调用参数
        // 注意：敏感数据请设置为 false 或在函数级别使用 skip() 排除
        with_args: false
    }
}
```

### MetricsConfig

```json5
{
    metrics: {
        // Metric 名称前缀（默认 "aop"）
        // 实际生成的 metric 指标名为：
        // - {prefix}_total: 总调用次数（按 operation + status 分组）
        // - {prefix}_retry_count: 重试次数（按 operation 分组）
        // - {prefix}_duration_ms: 调用耗时分布（按 operation 分组）
        // - {prefix}_in_progress: 当前正在执行的请求数（按 operation 分组）
        prefix: "aop",

        // 常量 Labels（会应用到所有 metric）
        // 支持的固定标签：service, env, version, cluster
        // 这些标签会自动添加到所有 metric 中，方便在 Prometheus 中查询和聚合
        labels: {
            service: "user-service",
            env: "production",
            version: "1.0.0",
            cluster: "us-west-1"
        }
    }
}
```

### GlobalTracingConfig（全局 Tracing 配置）

GlobalTracingConfig 用于初始化全局的 OpenTelemetry tracer provider，支持分布式追踪。

```json5
{
    enabled: true,                    // 是否启用 tracing（默认 false）
    service_name: "my-service",       // 服务名称（默认 "rustx-service"）
    sample_rate: 1.0,                 // 采样率 0.0-1.0（默认 1.0）

    // Exporter 配置
    exporter: {
        type: "otlp",                 // 导出器类型: "otlp" | "stdout" | "none"
        endpoint: "http://localhost:4317",  // OTLP endpoint（默认 "http://localhost:4317"）
        timeout: "10s",                // 请求超时（默认 10s）
        headers: {                     // 可选的请求头（用于认证等）
            "Authorization": "Bearer token123"
        }
    },

    // BatchSpanProcessor 配置（可选）
    batch_processor: {
        scheduled_delay: "5s",         // 导出间隔（默认 5s）
        max_queue_size: 2048,          // 最大队列大小（默认 2048）
        max_export_batch_size: 512,    // 最大导出批次大小（默认 512）
        max_concurrent_exports: 1      // 最大并发导出数量（默认 1）
    },

    // tracing_subscriber 配置
    subscriber: {
        log_level: "info",             // 日志级别（默认 "info"）
        with_fmt_layer: false          // 是否输出可读日志到控制台（默认 false）
    }
}
```

**使用示例：**

```rust
use rustx::aop::tracing::{init_tracer, GlobalTracingConfig};

// 使用 json5::from_str 解析配置
let config: GlobalTracingConfig = json5::from_str(r#"
{
    enabled: true,
    service_name: "user-service",
    sample_rate: 0.1,
    exporter: {
        type: "otlp",
        endpoint: "http://otel-collector:4317",
        timeout: "30s"
    },
    subscriber: {
        log_level: "info",
        with_fmt_layer: true
    }
}
"#)?;

// 初始化全局 tracer（只需调用一次）
init_tracer(&config)?;

// 现在可以在代码中使用 #[tracing::instrument] 宏进行分布式追踪
```

### GlobalMetricsConfig（全局 Metrics Server 配置）

GlobalMetricsConfig 用于启动 Prometheus HTTP Server，提供 metrics 拉取端点。

```json5
{
    port: 9090,              // HTTP 服务端口（默认 9090）
    path: "/metrics"         // Metric 端点路径（默认 "/metrics"）
}
```

**使用示例：**

```rust
use rustx::aop::metrics::{init_metric, GlobalMetricsConfig};

// 使用 json5::from_str 解析配置
let config: GlobalMetricsConfig = json5::from_str(r#"
{
    port: 9090,
    path: "/metrics"
}
"#)?;

// 启动 Metric HTTP Server（只需调用一次，在后台运行）
init_metric(config).await?;

// 现在可以通过 http://localhost:9090/metrics 访问 Prometheus 格式的 metrics
// 所有 Aop 实例的指标都会自动注册到这个全局 Registry
```

## 使用建议

### AopConfig 选择

**使用 `Aop::resolve()` 方法：**
- ✅ 支持 Reference 模式（引用已存在的实例）
- ✅ 支持 Create 模式（创建新实例）
- ✅ 自动处理配置类型，无需手动判断
- ✅ 返回 `Arc<Aop>`，支持多线程共享

```rust
// 推荐：使用 resolve
let aop = config.aop.map(|c| Aop::resolve(c)).transpose()?;

// 不推荐：直接使用 new（不支持引用）
let aop = config.aop.map(|c| Aop::new(c)).transpose()?;
```

### AopManager 使用场景

**适用场景：**
1. **多服务共享配置**：多个微服务共享同一个 Aop 实例
2. **统一管理**：集中管理所有 Aop 配置，方便维护和更新
3. **引用复用**：通过 Reference 模式避免重复创建相同配置
4. **动态管理**：支持运行时动态添加、移除 Aop 实例

**示例：微服务架构**

```rust
use rustx::aop::{init, get, AopManagerConfig};

// 使用 json5::from_str 解析配置并初始化全局 AopManager
let config: AopManagerConfig = json5::from_str(r#"
{
    default: {},
    aops: {
        // 创建基础 API 配置
        "api": {
            retry: {
                max_times: 3,
                strategy: "exponential",
                min_delay: "100ms"
            }
        },

        // 创建数据库配置（带日志）
        "database": {
            logging: {
                logger: {
                    level: "info",
                    formatter: { type: "TextFormatter" }
                },
                info_sample_rate: 0.1,
                warn_sample_rate: 1.0
            },
            retry: {
                max_times: 5,
                strategy: "fibonacci",
                jitter: true
            }
        },

        // 多个服务引用同一个 API 配置
        "user_service": {
            $instance: "api"
        },
        "order_service": {
            $instance: "api"
        }
    }
}
"#)?;

init(config)?;

// 各服务使用对应的 Aop
let user_aop = get("user_service");
let order_aop = get("order_service");
let db_aop = get("database");
```

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
        },
        tracing: {
            name: "production-service",
            with_args: false  // 生产环境不建议记录参数，避免敏感数据泄露
        },
        metric: {
            prefix: "prod_service",
            labels: {
                service: "user-service",
                env: "production",
                version: "1.0.0"
            }
        }
    }
}
```

### 策略选择

- **Constant**: 低延迟场景，固定延迟更可控
- **Exponential**: 高负载服务，延迟快速增长
- **Fibonacci**: 生产环境推荐，平滑增长且配合 jitter 避免惊群效应
