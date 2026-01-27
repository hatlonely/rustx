//! AOP Tracer 配置示例
//!
//! 演示如何配置 OpenTelemetry Tracer

use rustx::aop::{AopManager, AopManagerConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 示例 1: 使用 OTLP exporter（发送到 OpenTelemetry Collector）
    let otlp_config_json = r#"{
        default: {
            create: {
                retry: {
                    max_times: 3,
                    strategy: "constant",
                    delay: "100ms"
                }
            }
        },
        aops: {},
        tracer: {
            enabled: true,
            service_name: "my-rust-service",
            sample_rate: 1.0,
            exporter: {
                type: "otlp",
                endpoint: "http://localhost:4317",
                timeout: "10s"
            }
        }
    }"#;

    let config: AopManagerConfig = json5::from_str(otlp_config_json)?;
    let _manager = AopManager::new(config)?;
    println!("✓ OTLP tracer initialized");

    // 示例 2: 使用 Stdout exporter（用于调试，输出到控制台）
    println!("\n=== Stdout Exporter 示例 ===");
    let stdout_config_json = r#"{
        default: {
            create: {}
        },
        aops: {},
        tracer: {
            enabled: true,
            service_name: "my-debug-service",
            sample_rate: 1.0,
            exporter: {
                type: "stdout"
            }
        }
    }"#;

    match json5::from_str::<AopManagerConfig>(stdout_config_json) {
        Ok(config) => {
            match AopManager::new(config) {
                Ok(_manager) => {
                    println!("✓ Stdout tracer initialized");

                    // 创建一个示例 span 来演示输出
                    use tracing::{info, instrument};
                    use tracing_subscriber::prelude::*;

                    // 设置一个简单的 layer 来显示 tracing 日志
                    tracing_subscriber::registry()
                        .with(tracing_subscriber::fmt::layer().with_target(false))
                        .init();

                    // 执行一个带 tracing 的函数
                    #[instrument]
                    fn example_function(x: i32, y: i32) -> i32 {
                        info!("calculating sum");
                        x + y
                    }

                    let result = example_function(10, 20);
                    println!("函数返回值: {}", result);

                    println!("提示：查看上面的输出，你会看到 OpenTelemetry span 信息");
                }
                Err(e) => println!("✗ Failed to initialize stdout tracer: {}", e),
            }
        }
        Err(e) => println!("✗ Failed to parse config: {}", e),
    }

    // 示例 3: 不启用 tracer
    let no_tracer_config_json = r#"{
        default: {
            create: {}
        },
        aops: {},
        tracer: {
            enabled: false,
            service_name: "my-service",
            sample_rate: 1.0,
            exporter: {
                type: "none"
            }
        }
    }"#;

    let config: AopManagerConfig = json5::from_str(no_tracer_config_json)?;
    let _manager = AopManager::new(config)?;
    println!("✓ Tracer disabled");

    // 示例 4: 使用采样率（只记录 10% 的 traces）
    let sampled_config_json = r#"{
        default: {
            create: {}
        },
        aops: {},
        tracer: {
            enabled: true,
            service_name: "high-traffic-service",
            sample_rate: 0.1,
            exporter: {
                type: "otlp",
                endpoint: "http://otel-collector:4317"
            }
        }
    }"#;

    let config: AopManagerConfig = json5::from_str(sampled_config_json)?;
    let _manager = AopManager::new(config)?;
    println!("✓ Sampled tracer initialized (10% sampling)");

    // 示例 5: 带认证的 OTLP exporter
    let auth_config_json = r#"{
        default: {
            create: {}
        },
        aops: {},
        tracer: {
            enabled: true,
            service_name: "my-production-service",
            sample_rate: 1.0,
            exporter: {
                type: "otlp",
                endpoint: "https://otel-collector.example.com:4317",
                headers: {
                    "Authorization": "Bearer your-token-here"
                },
                timeout: "30s"
            }
        }
    }"#;

    let config: AopManagerConfig = json5::from_str(auth_config_json)?;
    let _manager = AopManager::new(config)?;
    println!("✓ Authenticated OTLP tracer initialized");

    Ok(())
}
