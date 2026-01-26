use anyhow::Result;
use rustx::aop::{Aop, AopConfig};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// 模拟一个不稳定的 API 客户端
struct ApiClient {
    attempt_count: Arc<AtomicUsize>,
}

impl ApiClient {
    fn new() -> Self {
        Self {
            attempt_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    // 模拟一个前几次会失败的 API 调用
    async fn call_api(&self, endpoint: &str) -> Result<String> {
        let count = self.attempt_count.fetch_add(1, Ordering::SeqCst);
        println!("  API 调用尝试 #{}", count + 1);

        if count < 3 {
            Err(anyhow::anyhow!("API call failed (attempt {})", count + 1))
        } else {
            Ok(format!("Success response from {}", endpoint))
        }
    }
}

// 服务配置
#[derive(Debug, Clone, serde::Deserialize, smart_default::SmartDefault)]
#[serde(default)]
pub struct ApiServiceConfig {
    pub aop: Option<AopConfig>,
}

// API 服务
pub struct ApiService {
    client: ApiClient,
    aop: Option<Arc<Aop>>,
}

impl ApiService {
    pub fn new(config: ApiServiceConfig) -> Result<Self> {
        let aop = config.aop.map(|config| Aop::resolve(config)).transpose()?;
        Ok(Self {
            client: ApiClient::new(),
            aop,
        })
    }

    // 使用 AOP 的方法
    pub async fn fetch_data(&self, endpoint: &str) -> Result<String> {
        rustx::aop!(&self.aop, self.client.call_api(endpoint).await)
    }
}

impl From<ApiServiceConfig> for ApiService {
    fn from(config: ApiServiceConfig) -> Self {
        ApiService::new(config).expect("Failed to create ApiService")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // ===== 场景 1: Constant 策略重试 =====
    println!("===== 场景 1: Constant 策略（固定延迟 200ms）=====");
    let config1: ApiServiceConfig = json5::from_str(
        r#"
        {
          aop: {
            retry: {
              max_times: 5,
              strategy: "constant",
              delay: "200ms",
            }
          }
        }
    "#,
    )?;
    let service1 = ApiService::new(config1)?;
    match service1.fetch_data("/api/users").await {
        Ok(result) => println!("✅ 成功: {}\n", result),
        Err(e) => println!("❌ 失败: {:?}\n", e),
    }

    // ===== 场景 2: Exponential 策略重试 =====
    println!("===== 场景 2: Exponential 策略（指数退避）=====");
    let config2: ApiServiceConfig = json5::from_str(
        r#"
        {
          aop: {
            retry: {
              max_times: 5,
              strategy: "exponential",
              min_delay: "100ms",
              max_delay: "2s",
              factor: 2.0,
            }
          }
        }
    "#,
    )?;
    let service2 = ApiService::new(config2)?;
    match service2.fetch_data("/api/products").await {
        Ok(result) => println!("✅ 成功: {}\n", result),
        Err(e) => println!("❌ 失败: {:?}\n", e),
    }

    // ===== 场景 3: Fibonacci 策略重试 =====
    println!("===== 场景 3: Fibonacci 策略（斐波那契退避）=====");
    let config3: ApiServiceConfig = json5::from_str(
        r#"
        {
          aop: {
            retry: {
              max_times: 5,
              strategy: "fibonacci",
              min_delay: "100ms",
              max_delay: "1s",
            }
          }
        }
    "#,
    )?;
    let service3 = ApiService::new(config3)?;
    match service3.fetch_data("/api/orders").await {
        Ok(result) => println!("✅ 成功: {}\n", result),
        Err(e) => println!("❌ 失败: {:?}\n", e),
    }

    // ===== 场景 4: 使用 Jitter（随机抖动）避免惊群效应 =====
    println!("===== 场景 4: Constant + Jitter（随机抖动）=====");
    let config4: ApiServiceConfig = json5::from_str(
        r#"
        {
          aop: {
            retry: {
              max_times: 3,
              strategy: "constant",
              delay: "200ms",
              jitter: true
            }
          }
        }
    "#,
    )?;
    let service4 = ApiService::new(config4)?;
    match service4.fetch_data("/api/items").await {
        Ok(result) => println!("✅ 成功: {}\n", result),
        Err(e) => println!("❌ 失败: {:?}\n", e),
    }

    // ===== 场景 5: 超过最大重试次数 =====
    println!("===== 场景 5: 超过最大重试次数（max_times=2）=====");
    let config5: ApiServiceConfig = json5::from_str(
        r#"
        {
          aop: {
            retry: {
              max_times: 2,
              strategy: "constant",
              delay: "100ms",
            }
          }
        }
    "#,
    )?;
    let service5 = ApiService::new(config5)?;
    match service5.fetch_data("/api/fail").await {
        Ok(result) => println!("✅ 成功: {}\n", result),
        Err(e) => println!("❌ 失败（超过最大重试次数）: {:?}\n", e),
    }

    // ===== 场景 6: 不启用重试 =====
    println!("===== 场景 6: 不启用重试 =====");
    let config6: ApiServiceConfig = json5::from_str("{}")?;
    let service6 = ApiService::new(config6)?;
    match service6.fetch_data("/api/no-retry").await {
        Ok(result) => println!("✅ 成功: {}\n", result),
        Err(e) => println!("❌ 失败（无重试）: {:?}\n", e),
    }

    println!("💡 提示：");
    println!("  - Constant: 固定延迟，适合稳定的重试场景");
    println!("  - Exponential: 延迟指数增长，适合高负载服务");
    println!("  - Fibonacci: 延迟按斐波那契数列增长，比指数更平滑");
    println!("  - Jitter: 在延迟基础上添加随机抖动，避免多个客户端同时重试造成惊群效应");

    Ok(())
}
