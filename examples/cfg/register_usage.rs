use anyhow::Result;
use rustx::cfg::serde_duration::{serde_as, HumanDur};
use rustx::cfg::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ServiceConfig {
    name: String,
    host: String,
    port: u16,
    #[serde_as(as = "HumanDur")]
    timeout: Duration,
    max_connections: Option<u32>,
}

#[derive(Debug)]
struct Service {
    #[allow(unused)]
    config: ServiceConfig,
}

impl Service {
    fn new(config: ServiceConfig) -> Self {
        println!(
            "创建服务: {} @ {}:{}, 超时: {:?}",
            config.name, config.host, config.port, config.timeout
        );
        Self { config }
    }
}

impl From<ServiceConfig> for Service {
    fn from(config: ServiceConfig) -> Self {
        Service::new(config)
    }
}

fn main() -> Result<()> {
    register::<Service, ServiceConfig>("service")?;

    // JSON 配置示例
    let json_config = r#"
    {
        "type": "service",
        "options": {
            "name": "web-api",
            "host": "localhost",
            "port": 8080,
            "timeout": "30s",
            "max_connections": 100
        }
    }"#;

    let type_options = TypeOptions::from_json(json_config)?;
    let service_obj = create_from_type_options(&type_options)?;

    if let Some(_service) = service_obj.downcast_ref::<Service>() {
        println!("✅ JSON配置创建服务成功");
    }

    // YAML 配置示例
    let yaml_config = r#"
type: service
options:
  name: "db-service"
  host: "127.0.0.1"
  port: 3306
  timeout: "1m"
  max_connections: 50
"#;

    let yaml_type_options = TypeOptions::from_yaml(yaml_config)?;
    let yaml_service_obj = create_from_type_options(&yaml_type_options)?;

    if let Some(_service) = yaml_service_obj.downcast_ref::<Service>() {
        println!("✅ YAML配置创建服务成功");
    }

    Ok(())
}
