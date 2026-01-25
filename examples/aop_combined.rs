use anyhow::Result;
use rustx::aop::{Aop, AopConfig};
use rustx::log::{init_logger_manager, LoggerManagerConfig};
use std::sync::atomic::Ordering;

// 模拟一个外部支付服务客户端
struct PaymentServiceClient {
    attempt_count: std::sync::atomic::AtomicUsize,
}

impl PaymentServiceClient {
    fn new() -> Self {
        Self {
            attempt_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    // 模拟一个可能失败的支付操作
    async fn process_payment(&self, amount: f64, currency: &str) -> Result<String> {
        let count = self.attempt_count.fetch_add(1, Ordering::SeqCst);
        println!("    → 支付服务调用 #{}", count + 1);

        // 前两次失败，第三次成功
        if count < 2 {
            Err(anyhow::anyhow!(
                "Payment service temporary unavailable (attempt {})",
                count + 1
            ))
        } else {
            Ok(format!(
                "Payment processed: {:.2} {}",
                amount, currency
            ))
        }
    }

    // 模拟退款操作
    async fn refund_payment(&self, transaction_id: &str) -> Result<String> {
        let count = self.attempt_count.fetch_add(1, Ordering::SeqCst);
        println!("    → 退款服务调用 #{}", count + 1);

        if count < 1 {
            Err(anyhow::anyhow!("Refund service timeout"))
        } else {
            Ok(format!("Refund completed for transaction: {}", transaction_id))
        }
    }
}

// 支付服务配置
#[derive(Debug, Clone, serde::Deserialize, smart_default::SmartDefault)]
#[serde(default)]
pub struct PaymentServiceConfig {
    pub aop: Option<AopConfig>,
}

// 支付服务
pub struct PaymentService {
    client: PaymentServiceClient,
    aop: Option<Aop>,
}

impl PaymentService {
    pub fn new(config: PaymentServiceConfig) -> Result<Self> {
        let aop = config.aop.map(|config| Aop::new(config)).transpose()?;
        Ok(Self {
            client: PaymentServiceClient::new(),
            aop,
        })
    }

    // 处理支付（带 Logging + Retry）
    pub async fn process_payment(&self, amount: f64, currency: &str) -> Result<String> {
        rustx::aop!(&self.aop, self.client.process_payment(amount, currency).await)
    }

    // 处理退款（带 Logging + Retry）
    pub async fn refund_payment(&self, transaction_id: &str) -> Result<String> {
        rustx::aop!(&self.aop, self.client.refund_payment(transaction_id).await)
    }
}

impl From<PaymentServiceConfig> for PaymentService {
    fn from(config: PaymentServiceConfig) -> Self {
        PaymentService::new(config).expect("Failed to create PaymentService")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // ===== 初始化 Logger Manager =====
    let logger_config = r#"
    {
      default: {
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
          options: {
            target: "stdout",
            auto_flush: true
          }
        }
      }
    }
    "#;
    let manager_config: LoggerManagerConfig = json5::from_str(logger_config)?;
    init_logger_manager(manager_config)?;

    // ===== 场景 1: Logging + Exponential Retry =====
    println!("===== 场景 1: 完整 Logging + Exponential Retry =====");
    let config1: PaymentServiceConfig = json5::from_str(r#"
        {
          aop: {
            logging: {
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
                  options: {
                    target: "stdout"
                  }
                }
              },
              info_sample_rate: 1.0,
              warn_sample_rate: 1.0
            },
            retry: {
              max_times: 5,
              strategy: "exponential",
              min_delay: "100ms",
              max_delay: "2s",
              factor: 2.0,
            }
          }
        }
    "#)?;
    let service1 = PaymentService::new(config1)?;
    match service1.process_payment(99.99, "USD").await {
        Ok(result) => println!("✅ {}\n", result),
        Err(e) => println!("❌ 失败: {:?}\n", e),
    }

    // ===== 场景 2: 只记录失败的日志（降低日志量）=====
    println!("===== 场景 2: 只记录失败日志（info_sample_rate=0）=====");
    let config2: PaymentServiceConfig = json5::from_str(r#"
        {
          aop: {
            logging: {
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
                  options: {
                    target: "stdout"
                  }
                }
              },
              info_sample_rate: 0.0,
              warn_sample_rate: 1.0
            },
            retry: {
              max_times: 3,
              strategy: "constant",
              delay: "150ms",
            }
          }
        }
    "#)?;
    let service2 = PaymentService::new(config2)?;
    match service2.refund_payment("TXN-12345").await {
        Ok(result) => println!("✅ {}\n", result),
        Err(e) => println!("❌ 失败: {:?}\n", e),
    }

    // ===== 场景 3: 高采样率 + Fibonacci Retry（生产环境推荐）=====
    println!("===== 场景 3: 生产环境配置（低采样率 + Fibonacci + Jitter）=====");
    let config3: PaymentServiceConfig = json5::from_str(r#"
        {
          aop: {
            logging: {
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
                  options: {
                    target: "stdout"
                  }
                }
              },
              info_sample_rate: 0.01,
              warn_sample_rate: 0.1
            },
            retry: {
              max_times: 5,
              strategy: "fibonacci",
              min_delay: "100ms",
              max_delay: "5s",
              jitter: true
            }
          }
        }
    "#)?;
    let service3 = PaymentService::new(config3)?;
    match service3.process_payment(199.99, "EUR").await {
        Ok(result) => println!("✅ {}\n", result),
        Err(e) => println!("❌ 失败: {:?}\n", e),
    }

    // ===== 场景 4: 只启用 Retry（不记录日志）=====
    println!("===== 场景 4: 只启用 Retry（无 Logging）=====");
    let config4: PaymentServiceConfig = json5::from_str(r#"
        {
          aop: {
            retry: {
              max_times: 3,
              strategy: "constant",
              delay: "200ms",
            }
          }
        }
    "#)?;
    let service4 = PaymentService::new(config4)?;
    match service4.refund_payment("TXN-67890").await {
        Ok(result) => println!("✅ {} (无日志记录)\n", result),
        Err(e) => println!("❌ 失败: {:?}\n", e),
    }

    println!("💡 组合使用建议：");
    println!("  - 开发环境: info_sample_rate=1.0, warn_sample_rate=1.0（完整日志）");
    println!("  - 生产环境: info_sample_rate=0.01, warn_sample_rate=0.1（降低日志量）");
    println!("  - 高并发场景: Fibonacci + Jitter（避免惊群效应）");
    println!("  - 低延迟场景: Constant（固定延迟更可控）");

    Ok(())
}
