use anyhow::Result;
use rustx::cfg::*;
use serde::{Deserialize, Serialize};

// 定义 Service trait
trait Service: Send + Sync {
    fn serve(&self);
    fn get_name(&self) -> &str;
    fn get_version(&self) -> &str;
}

// ServiceV1 配置
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ServiceV1Config {
    name: String,
    host: String,
    port: u16,
}

// ServiceV1 实现
#[derive(Debug)]
struct ServiceV1 {
    config: ServiceV1Config,
}

impl ServiceV1 {
    fn new(config: ServiceV1Config) -> Self {
        println!(
            "创建 ServiceV1: {} @ {}:{}",
            config.name, config.host, config.port
        );
        Self { config }
    }
}

impl From<ServiceV1Config> for ServiceV1 {
    fn from(config: ServiceV1Config) -> Self {
        ServiceV1::new(config)
    }
}

impl Service for ServiceV1 {
    fn serve(&self) {
        println!(
            "[V1] 服务 '{}' 运行在 {}:{}",
            self.config.name, self.config.host, self.config.port
        );
    }

    fn get_name(&self) -> &str {
        &self.config.name
    }

    fn get_version(&self) -> &str {
        "v1"
    }
}

// ServiceV2 配置（增加了新功能）
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ServiceV2Config {
    name: String,
    host: String,
    port: u16,
    max_connections: u32,
    enable_tls: bool,
}

// ServiceV2 实现
#[derive(Debug)]
struct ServiceV2 {
    config: ServiceV2Config,
}

impl ServiceV2 {
    fn new(config: ServiceV2Config) -> Self {
        println!(
            "创建 ServiceV2: {} @ {}:{} (max_connections: {}, tls: {})",
            config.name, config.host, config.port, config.max_connections, config.enable_tls
        );
        Self { config }
    }
}

impl From<ServiceV2Config> for ServiceV2 {
    fn from(config: ServiceV2Config) -> Self {
        ServiceV2::new(config)
    }
}

impl Service for ServiceV2 {
    fn serve(&self) {
        println!(
            "[V2] 服务 '{}' 运行在 {}:{} (最大连接数: {}, TLS: {})",
            self.config.name,
            self.config.host,
            self.config.port,
            self.config.max_connections,
            if self.config.enable_tls { "启用" } else { "禁用" }
        );
    }

    fn get_name(&self) -> &str {
        &self.config.name
    }

    fn get_version(&self) -> &str {
        "v2"
    }
}

// 为 Box<ServiceV1> 实现到 Box<dyn Service> 的转换
impl From<Box<ServiceV1>> for Box<dyn Service> {
    fn from(service: Box<ServiceV1>) -> Self {
        service as Box<dyn Service>
    }
}

// 为 Box<ServiceV2> 实现到 Box<dyn Service> 的转换
impl From<Box<ServiceV2>> for Box<dyn Service> {
    fn from(service: Box<ServiceV2>) -> Self {
        service as Box<dyn Service>
    }
}

fn main() -> Result<()> {
    println!("=== Trait-based 配置注册示例 ===\n");

    // 注册两个不同的 Service 实现
    register_trait::<ServiceV1, dyn Service, ServiceV1Config>("service-v1")?;
    register_trait::<ServiceV2, dyn Service, ServiceV2Config>("service-v2")?;

    println!("✅ 已注册 ServiceV1 和 ServiceV2\n");

    // 测试创建 ServiceV1
    let v1_config = r#"
    {
        "type": "service-v1",
        "options": {
            "name": "web-api",
            "host": "localhost",
            "port": 8080
        }
    }"#;

    let v1_type_options = TypeOptions::from_json(v1_config)?;
    let service_v1: Box<dyn Service> = create_trait_from_type_options(&v1_type_options)?;

    println!("服务名称: {}", service_v1.get_name());
    println!("服务版本: {}", service_v1.get_version());
    service_v1.serve();
    println!();

    // 测试创建 ServiceV2
    let v2_config = r#"
    {
        "type": "service-v2",
        "options": {
            "name": "db-service",
            "host": "127.0.0.1",
            "port": 3306,
            "max_connections": 100,
            "enable_tls": true
        }
    }"#;

    let v2_type_options = TypeOptions::from_json(v2_config)?;
    let service_v2: Box<dyn Service> = create_trait_from_type_options(&v2_type_options)?;

    println!("服务名称: {}", service_v2.get_name());
    println!("服务版本: {}", service_v2.get_version());
    service_v2.serve();
    println!();

    // 演示：根据配置动态选择实现
    println!("=== 动态选择服务实现 ===\n");

    let configs = vec![
        r#"{"type": "service-v1", "options": {"name": "api-1", "host": "0.0.0.0", "port": 8081}}"#,
        r#"{"type": "service-v2", "options": {"name": "api-2", "host": "0.0.0.0", "port": 8082, "max_connections": 50, "enable_tls": false}}"#,
        r#"{"type": "service-v1", "options": {"name": "api-3", "host": "0.0.0.0", "port": 8083}}"#,
    ];

    let mut services: Vec<Box<dyn Service>> = Vec::new();

    for config_str in configs {
        let type_options = TypeOptions::from_json(config_str)?;
        let service: Box<dyn Service> = create_trait_from_type_options(&type_options)?;
        services.push(service);
    }

    println!("创建了 {} 个服务实例：", services.len());
    for service in &services {
        println!("  - {} ({})", service.get_name(), service.get_version());
    }
    println!();

    // 统一调用
    println!("=== 统一调用所有服务 ===\n");
    for service in services {
        service.serve();
    }

    Ok(())
}
