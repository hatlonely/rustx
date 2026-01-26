use anyhow::Result;
use rustx::log::{Logger, LoggerConfig, LoggerManagerConfig};
use serde::Deserialize;
use smart_default::SmartDefault;
use std::sync::Arc;

/// 服务配置
#[derive(Debug, Clone, Deserialize, SmartDefault)]
#[serde(default)]
pub struct MyServiceConfig {
    /// 服务名称
    #[default = "MyService"]
    pub name: String,

    /// Logger 配置
    pub logger: LoggerConfig,
}

/// 示例业务服务
pub struct MyService {
    name: String,
    logger: Arc<Logger>,
}

impl MyService {
    /// 从配置创建服务（唯一的构造方法）
    pub fn new(config: MyServiceConfig) -> Result<Self> {
        let logger = Logger::resolve(config.logger)?;
        Ok(Self {
            name: config.name,
            logger,
        })
    }

    /// 使用 logger 记录日志
    pub async fn do_work(&self, message: &str) -> Result<()> {
        self.logger
            .infom(
                &format!("{} working", self.name),
                vec![
                    ("service", self.name.as_str().into()),
                    ("message", message.into()),
                ],
            )
            .await?;

        Ok(())
    }
}

// 实现从 Config 到 Service 的转换
impl From<MyServiceConfig> for MyService {
    fn from(config: MyServiceConfig) -> Self {
        MyService::new(config).expect("Failed to create MyService")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // ===== LoggerManager 配置 =====
    let config_json5 = r#"
    {
      default: {
        level: "info",
        formatter: {
          type: "TextFormatter",
          options: {
            colored: false
          }
        },
        appender: {
          type: "ConsoleAppender",
          options: {
            target: "stdout",
            auto_flush: true
          }
        }
      },

      loggers: {
        "production": {
          level: "info",
          formatter: {
            type: "TextFormatter",
            options: {
              colored: false
            }
          },
          appender: {
            type: "ConsoleAppender",
            options: {
              target: "stdout",
              auto_flush: true
            }
          }
        },

        "debug": {
          level: "trace",
          formatter: {
            type: "TextFormatter",
            options: {
              colored: true
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
    }
    "#;

    let manager_config: LoggerManagerConfig = json5::from_str(config_json5)?;

    // ===== 初始化全局 Logger Manager =====
    ::rustx::log::init(manager_config)?;

    // ===== 场景 1: 引用全局 'production' logger =====
    println!("===== 场景 1: 引用全局 production logger =====");
    let service1_config: MyServiceConfig = json5::from_str(
        r#"
        {
          name: "UserService",
          logger: {
            "$instance": "production"
          }
        }
    "#,
    )?;
    let service1 = MyService::new(service1_config)?;
    service1.do_work("Creating user").await?;

    // ===== 场景 2: 创建全新的独立 logger =====
    println!("===== 场景 2: 创建全新的独立 debug logger =====");
    let service2_config: MyServiceConfig = json5::from_str(
        r#"
        {
          name: "PaymentService",
          logger: {
            level: "debug",
            formatter: {
              type: "TextFormatter",
              options: {
                colored: true
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
    "#,
    )?;
    let service2 = MyService::new(service2_config)?;
    service2.do_work("Processing payment").await?;

    Ok(())
}
