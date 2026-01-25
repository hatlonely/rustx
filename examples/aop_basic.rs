use anyhow::Result;
use rustx::aop::{Aop, AopConfig};
use rustx::log::{init_logger_manager, LoggerManagerConfig};

// 模拟一个简单的数据库客户端
struct DatabaseClient {
    fail_count: std::sync::atomic::AtomicUsize,
}

impl DatabaseClient {
    fn new() -> Self {
        Self {
            fail_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    // 模拟一个可能失败的查询
    async fn query(&self, sql: &str) -> Result<String> {
        let count = self.fail_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if count < 2 {
            Err(anyhow::anyhow!("Database connection failed"))
        } else {
            Ok(format!("Result for: {}", sql))
        }
    }

    // 同步方法示例
    fn sync_query(&self, key: &str) -> Result<String> {
        Ok(format!("Sync result for: {}", key))
    }
}

// 用户服务配置
#[derive(Debug, Clone, serde::Deserialize, smart_default::SmartDefault)]
#[serde(default)]
pub struct UserServiceConfig {
    pub aop: Option<AopConfig>,
}

// 用户服务
pub struct UserService {
    client: DatabaseClient,
    aop: Option<Aop>,
}

impl UserService {
    pub fn new(config: UserServiceConfig) -> Result<Self> {
        let aop = config
            .aop
            .map(|config| Aop::new(config))
            .transpose()?;

        Ok(Self {
            client: DatabaseClient::new(),
            aop,
        })
    }

    // 使用 aop! 宏包装异步方法
    pub async fn get_user(&self, user_id: &str) -> Result<String> {
        rustx::aop!(&self.aop, self.client.query(&format!("SELECT * FROM users WHERE id = {}", user_id)).await)
    }

    // 使用 aop_sync! 宏包装同步方法
    pub fn get_user_sync(&self, user_id: &str) -> Result<String> {
        rustx::aop_sync!(&self.aop, self.client.sync_query(user_id))
    }
}

impl From<UserServiceConfig> for UserService {
    fn from(config: UserServiceConfig) -> Self {
        UserService::new(config).expect("Failed to create UserService")
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

    // ===== 场景 1: 只启用 Logging（异步方法）=====
    println!("===== 场景 1: Logging (异步方法) =====");
    let config1: UserServiceConfig = json5::from_str(r#"
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
            }
          }
        }
    "#)?;
    let service1 = UserService::new(config1)?;
    // 由于未启用 retry，前几次调用会失败（已记录日志）
    match service1.get_user("123").await {
        Ok(result) => println!("Result: {}\n", result),
        Err(e) => println!("Expected error (no retry enabled): {}\n", e),
    }

    // ===== 场景 2: 采样率配置（只记录 50% 的日志）=====
    println!("===== 场景 2: 采样率配置 =====");
    let config2: UserServiceConfig = json5::from_str(r#"
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
              info_sample_rate: 0.5,
              warn_sample_rate: 1.0
            }
          }
        }
    "#)?;
    let service2 = UserService::new(config2)?;
    for i in 0..3 {
        match service2.get_user(&format!("user_{}", i)).await {
            Ok(_) => {},
            Err(_) => {},
        }
    }
    println!("注意：由于 info_sample_rate=0.5，部分日志不会被记录\n");

    // ===== 场景 3: 同步方法的 Logging =====
    println!("===== 场景 3: Logging (同步方法) =====");
    let config3: UserServiceConfig = json5::from_str(r#"
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
              }
            }
          }
        }
    "#)?;
    let service3 = UserService::new(config3)?;
    let result = service3.get_user_sync("456")?;
    println!("Result: {}\n", result);

    // ===== 场景 4: 不启用 AOP =====
    println!("===== 场景 4: 不启用 AOP =====");
    let config4: UserServiceConfig = json5::from_str("{}")?;
    let service4 = UserService::new(config4)?;
    match service4.get_user("789").await {
        Ok(result) => println!("Result: {} (无日志记录)\n", result),
        Err(e) => println!("Error: {} (无日志记录)\n", e),
    }

    Ok(())
}
