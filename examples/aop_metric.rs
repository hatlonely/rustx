use rustx::aop::{AopManager, AopManagerConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Response {
    data: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // AopManager 配置，启用 metric server
    let manager_config_json = r#"{
        // 默认 Aop 配置
        default: {
            metric: {
                enabled: true,
                prefix: "myapp",
                labels: {
                    env: "production",
                    version: "1.0.0"
                }
            },
            retry: {
                max_times: 2,
                strategy: "constant",
                delay: "100ms"
            }
        },

        // 命名 Aop 配置
        aops: {
            "api": {
                metric: {
                    enabled: true,
                    prefix: "myapp_api",
                    labels: {
                        service: "api-gateway"
                    }
                },
                retry: {
                    max_times: 3,
                    strategy: "exponential",
                    min_delay: "50ms",
                    max_delay: "1s"
                }
            },
            "database": {
                metric: {
                    enabled: true,
                    prefix: "myapp_db",
                    labels: {
                        service: "postgres",
                        cluster: "main"
                    }
                },
                retry: {
                    max_times: 5,
                    strategy: "fibonacci",
                    min_delay: "100ms",
                    max_delay: "5s"
                }
            }
        },

        // 全局 Metric Server 配置
        metric: {
            enabled: true,
            port: 9090,
            path: "/metrics"
        }
    }"#;

    let manager_config: AopManagerConfig = json5::from_str(manager_config_json)?;

    // 创建 AopManager（会自动启动 metric server）
    let _manager = AopManager::new(manager_config)?;

    println!("AopManager created with metric server listening on http://localhost:9090/metrics");
    println!();
    println!("You can curl the metrics endpoint:");
    println!("  curl http://localhost:9090/metrics");
    println!();
    println!("Or use Prometheus to scrape:");
    println!("  scrape_configs:");
    println!("    - job_name: 'myapp'");
    println!("      static_configs:");
    println!("        - targets: ['localhost:9090']");
    println!();
    println!("Available metrics:");
    println!("  - myapp_total{{operation=\"...\",status=\"success|error\"}}");
    println!("  - myapp_retry_count{{operation=\"...\"}}");
    println!("  - myapp_duration_ms{{operation=\"...\"}}");
    println!("  - myapp_in_progress{{operation=\"...\"}}");
    println!("  - myapp_api_*");
    println!("  - myapp_db_*");

    // 保持程序运行
    tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;

    Ok(())
}
