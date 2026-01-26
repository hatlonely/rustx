use anyhow::Result;
use rustx::aop::{Aop, AopManagerConfig};
use rustx::log::LoggerManagerConfig;

// 模拟外部服务客户端
struct ExternalServiceClient {
    attempt_count: std::sync::atomic::AtomicUsize,
}

impl ExternalServiceClient {
    fn new() -> Self {
        Self {
            attempt_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    async fn call(&self, service: &str) -> Result<String> {
        let count = self
            .attempt_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if count < 2 {
            Err(anyhow::anyhow!("{} service unavailable", service))
        } else {
            Ok(format!("Response from {}", service))
        }
    }
}

// 服务配置（使用全局 aop）
#[derive(Debug, Clone, serde::Deserialize, smart_default::SmartDefault)]
#[serde(default)]
pub struct ServiceConfig {
    pub aop_key: Option<String>,
}

// 服务（使用全局 aop）
pub struct Service {
    client: ExternalServiceClient,
    aop_key: Option<String>,
}

impl Service {
    pub fn new(config: ServiceConfig) -> Result<Self> {
        Ok(Self {
            client: ExternalServiceClient::new(),
            aop_key: config.aop_key,
        })
    }

    // 从全局管理器获取 aop 并使用
    pub async fn call_external(&self, service_name: &str) -> Result<String> {
        let aop = self
            .aop_key
            .as_ref()
            .map(|key| rustx::aop::get_or_default(key));

        rustx::aop!(&aop, self.client.call(service_name).await)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化 Logger Manager
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
    ::rustx::log::init(manager_config)?;

    // 场景 1: 初始化全局 AopManager（多个命名 aop + 引用）
    let config: AopManagerConfig = json5::from_str(
        r#"
        {
          // 默认 aop（用于 fallback）
          default: {
            retry: {
              max_times: 2,
              strategy: "constant",
              delay: "100ms"
            }
          },

          // 命名 aop 映射
          aops: {
            // 创建一个新的 aop（带日志和重试）
            "database": {
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
                max_times: 3,
                strategy: "exponential",
                min_delay: "100ms",
                max_delay: "2s",
                factor: 2.0
              }
            },

            // 引用上面的 "database" aop（共享同一个实例）
            "cache": {
              "$instance": "database"
            },

            // 创建另一个独立的 aop（只重试，不记录日志）
            "api": {
              retry: {
                max_times: 5,
                strategy: "fibonacci",
                min_delay: "100ms",
                max_delay: "1s"
              }
            }
          }
        }
    "#,
    )?;
    ::rustx::aop::init(config)?;

    // 场景 2: 使用全局函数获取 aop
    let db_aop = rustx::aop::get("database");
    assert!(db_aop.is_some());

    let nonexistent = rustx::aop::get("nonexistent");
    assert!(nonexistent.is_none());

    // 场景 3: get_or_default - 不存在则返回默认 aop
    let fallback_aop = rustx::aop::get_or_default("nonexistent");
    assert!(fallback_aop.retry_config.is_some());

    // 场景 4: 服务使用全局 aop
    let service = Service::new(ServiceConfig {
        aop_key: Some("database".to_string()),
    })?;
    let _ = service.call_external("database").await?;

    // 场景 5: 动态添加 aop
    let new_aop_config: rustx::aop::AopCreateConfig = json5::from_str(
        r#"
        {
          retry: {
            max_times: 3,
            strategy: "constant",
            delay: "200ms"
          }
        }
    "#,
    )?;
    let new_aop = Aop::new(new_aop_config)?;
    rustx::aop::add("dynamic".to_string(), new_aop);
    assert!(rustx::aop::contains("dynamic"));

    // 场景 6: 移除 aop
    let removed = rustx::aop::remove("dynamic");
    assert!(removed.is_some());
    assert!(!rustx::aop::contains("dynamic"));

    // 场景 7: 获取所有 aop 的 keys
    let keys = rustx::aop::keys();
    assert!(keys.contains(&"database".to_string()));
    assert!(keys.contains(&"cache".to_string()));
    assert!(keys.contains(&"api".to_string()));

    // 场景 8: 验证引用实例共享
    let db_aop1 = rustx::aop::get("database").unwrap();
    let cache_aop = rustx::aop::get("cache").unwrap();
    // cache 引用了 database，应该是同一个实例
    assert!(std::sync::Arc::ptr_eq(&db_aop1, &cache_aop));

    // 场景 9: 设置新的默认 aop
    let default_aop_config: rustx::aop::AopCreateConfig = json5::from_str(
        r#"
        {
          retry: {
            max_times: 1,
            strategy: "constant",
            delay: "50ms"
          }
        }
    "#,
    )?;
    let new_default = std::sync::Arc::new(Aop::new(default_aop_config)?);
    rustx::aop::set_default(new_default);

    Ok(())
}
