//! 零耦合配置系统功能测试

use anyhow::Result;
use rustx::cfg::*;
use rustx::cfg::duration::{serde_as, HumanDur};
use rustx::cfg::registry::generate_auto_type_name;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 测试服务配置
#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
struct WebServerConfig {
    host: String,
    port: u16,
    #[serde_as(as = "HumanDur")]
    timeout: Duration,
    #[serde_as(as = "HumanDur")]
    keepalive: Duration,
    max_connections: usize,
    ssl_enabled: bool,
    routes: Vec<RouteConfig>,
    middleware: MiddlewareConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
struct RouteConfig {
    path: String,
    method: String,
    handler: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
struct MiddlewareConfig {
    cors_enabled: bool,
    auth_required: bool,
    rate_limit: Option<u32>,
}

/// Web服务器（业务逻辑类型，完全不知道配置系统）
#[derive(Debug, PartialEq)]
struct WebServer {
    config: WebServerConfig,
    status: String,
}

impl WebServer {
    fn new(config: WebServerConfig) -> Self {
        Self {
            config,
            status: "running".to_string(),
        }
    }

    fn get_endpoint(&self) -> String {
        format!("{}://{}:{}",
            if self.config.ssl_enabled { "https" } else { "http" },
            self.config.host,
            self.config.port
        )
    }
}

/// 零耦合配置实现
impl WithConfig<WebServerConfig> for WebServer {
    fn with_config(config: WebServerConfig) -> Self {
        WebServer::new(config)
    }
}

/// 数据库配置
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
struct DatabaseConfig {
    driver: String,
    host: String,
    port: u16,
    database: String,
    username: String,
    password: String,
    #[serde(default = "default_max_connections")]
    max_connections: u32,
    #[serde(default)]
    ssl_mode: Option<String>,
}

fn default_max_connections() -> u32 {
    10
}

/// 数据库服务
#[derive(Debug, PartialEq)]
struct Database {
    config: DatabaseConfig,
    connected: bool,
}

impl WithConfig<DatabaseConfig> for Database {
    fn with_config(config: DatabaseConfig) -> Self {
        Self {
            config,
            connected: true,
        }
    }
}

/// 缓存配置
#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
struct CacheConfig {
    cache_type: String,
    #[serde_as(as = "HumanDur")]
    ttl: Duration,
    max_size: usize,
}

/// 缓存服务
#[derive(Debug, PartialEq)]
struct Cache {
    config: CacheConfig,
}

impl WithConfig<CacheConfig> for Cache {
    fn with_config(config: CacheConfig) -> Self {
        Self { config }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_coupling_json_workflow() -> Result<()> {
        // 注册类型 - 零耦合，不需要业务类型知道配置系统
        register::<WebServer, WebServerConfig>()?;

        // 获取实际的类型名
        let type_name = generate_auto_type_name::<WebServer>();

        let json_config = format!(r#"
        {{
            "type": "{}",
            "options": {{
                "host": "0.0.0.0",
                "port": 8080,
                "timeout": "30s",
                "keepalive": "5m",
                "max_connections": 1000,
                "ssl_enabled": true,
                "routes": [
                    {{
                        "path": "/api/v1",
                        "method": "GET",
                        "handler": "api_handler"
                    }},
                    {{
                        "path": "/health",
                        "method": "GET",
                        "handler": "health_handler"
                    }}
                ],
                "middleware": {{
                    "cors_enabled": true,
                    "auth_required": false,
                    "rate_limit": 100
                }}
            }}
        }}"#, type_name);

        let type_options = TypeOptions::from_json(&json_config)?;
        let server_obj = create_from_type_options(&type_options)?;

        let server = server_obj.downcast_ref::<WebServer>()
            .ok_or_else(|| anyhow::anyhow!("Failed to downcast to WebServer"))?;

        assert_eq!(server.config.host, "0.0.0.0");
        assert_eq!(server.config.port, 8080);
        assert_eq!(server.config.timeout, Duration::from_secs(30));
        assert_eq!(server.config.keepalive, Duration::from_secs(300));
        assert_eq!(server.config.max_connections, 1000);
        assert!(server.config.ssl_enabled);
        assert_eq!(server.config.routes.len(), 2);
        assert_eq!(server.config.routes[0].path, "/api/v1");
        assert!(server.config.middleware.cors_enabled);
        assert_eq!(server.get_endpoint(), "https://0.0.0.0:8080");

        Ok(())
    }

    #[test]
    fn test_zero_coupling_yaml_workflow() -> Result<()> {
        register::<WebServer, WebServerConfig>()?;

        let type_name = generate_auto_type_name::<WebServer>();
        let yaml_config = format!(r#"
type: "{}"
options:
  host: "localhost"
  port: 3000
  timeout: "45s"
  keepalive: "10m"
  max_connections: 500
  ssl_enabled: false
  routes:
    - path: "/api"
      method: "POST"
      handler: "post_handler"
  middleware:
    cors_enabled: false
    auth_required: true
    rate_limit: null
"#, type_name);

        let type_options = TypeOptions::from_yaml(&yaml_config)?;
        let server_obj = create_from_type_options(&type_options)?;

        let server = server_obj.downcast_ref::<WebServer>()
            .ok_or_else(|| anyhow::anyhow!("Failed to downcast to WebServer"))?;

        assert_eq!(server.config.host, "localhost");
        assert_eq!(server.config.port, 3000);
        assert_eq!(server.config.timeout, Duration::from_secs(45));
        assert!(!server.config.ssl_enabled);
        assert!(server.config.middleware.auth_required);
        assert!(server.config.middleware.rate_limit.is_none());
        assert_eq!(server.get_endpoint(), "http://localhost:3000");

        Ok(())
    }

    #[test]
    fn test_multiple_service_types() -> Result<()> {
        // 注册多个不同的服务类型
        register::<WebServer, WebServerConfig>()?;
        register::<Database, DatabaseConfig>()?;
        register::<Cache, CacheConfig>()?;

        let web_type_name = generate_auto_type_name::<WebServer>();
        let db_type_name = generate_auto_type_name::<Database>();
        let cache_type_name = generate_auto_type_name::<Cache>();

        // Web服务器配置
        let web_config = format!(r#"
        {{
            "type": "{}",
            "options": {{
                "host": "127.0.0.1",
                "port": 8080,
                "timeout": "30s",
                "keepalive": "5m",
                "max_connections": 100,
                "ssl_enabled": false,
                "routes": [],
                "middleware": {{
                    "cors_enabled": true,
                    "auth_required": false,
                    "rate_limit": null
                }}
            }}
        }}"#, web_type_name);

        // 数据库配置
        let db_config = format!(r#"
        {{
            "type": "{}",
            "options": {{
                "driver": "postgresql",
                "host": "localhost",
                "port": 5432,
                "database": "myapp",
                "username": "user",
                "password": "pass"
            }}
        }}"#, db_type_name);

        // 缓存配置
        let cache_config = format!(r#"
        {{
            "type": "{}",
            "options": {{
                "cache_type": "redis",
                "ttl": "1h",
                "max_size": 1000
            }}
        }}"#, cache_type_name);

        // 创建各个服务
        let web_options = TypeOptions::from_json(&web_config)?;
        let web_obj = create_from_type_options(&web_options)?;
        let web_server = web_obj.downcast_ref::<WebServer>().unwrap();

        let db_options = TypeOptions::from_json(&db_config)?;
        let db_obj = create_from_type_options(&db_options)?;
        let database = db_obj.downcast_ref::<Database>().unwrap();

        let cache_options = TypeOptions::from_json(&cache_config)?;
        let cache_obj = create_from_type_options(&cache_options)?;
        let cache = cache_obj.downcast_ref::<Cache>().unwrap();

        // 验证各个服务
        assert_eq!(web_server.config.host, "127.0.0.1");
        assert_eq!(web_server.status, "running");

        assert_eq!(database.config.driver, "postgresql");
        assert_eq!(database.config.max_connections, 10); // 默认值
        assert!(database.connected);

        assert_eq!(cache.config.cache_type, "redis");
        assert_eq!(cache.config.ttl, Duration::from_secs(3600));

        Ok(())
    }

    #[test]
    fn test_manual_type_name_registration() -> Result<()> {
        // 使用自定义类型名注册
        register_with_name::<WebServer, WebServerConfig>("custom_web_server")?;

        let config = r#"
        {
            "type": "custom_web_server",
            "options": {
                "host": "custom.example.com",
                "port": 9000,
                "timeout": "60s",
                "keepalive": "15m",
                "max_connections": 2000,
                "ssl_enabled": true,
                "routes": [],
                "middleware": {
                    "cors_enabled": false,
                    "auth_required": true,
                    "rate_limit": 50
                }
            }
        }"#;

        let type_options = TypeOptions::from_json(config)?;
        let server_obj = create_from_type_options(&type_options)?;
        let server = server_obj.downcast_ref::<WebServer>().unwrap();

        assert_eq!(server.config.host, "custom.example.com");
        assert_eq!(server.config.port, 9000);
        assert!(server.config.ssl_enabled);

        Ok(())
    }

    #[test]
    fn test_format_conversion_workflow() -> Result<()> {
        register::<Cache, CacheConfig>()?;

        let cache_type_name = generate_auto_type_name::<Cache>();

        // JSON -> TypeOptions -> YAML
        let json_config = format!(r#"
        {{
            "type": "{}",
            "options": {{
                "cache_type": "memory",
                "ttl": "30m",
                "max_size": 500
            }}
        }}"#, cache_type_name);

        let type_options = TypeOptions::from_json(&json_config)?;
        let yaml_output = type_options.to_yaml()?;

        // YAML -> TypeOptions -> 对象创建
        let yaml_type_options = TypeOptions::from_yaml(&yaml_output)?;
        let cache_obj = create_from_type_options(&yaml_type_options)?;
        let cache = cache_obj.downcast_ref::<Cache>().unwrap();

        assert_eq!(cache.config.cache_type, "memory");
        assert_eq!(cache.config.ttl, Duration::from_secs(1800));
        assert_eq!(cache.config.max_size, 500);

        // 转换为TOML格式
        let toml_output = type_options.to_toml()?;
        let toml_type_options = TypeOptions::from_toml(&toml_output)?;
        let cache_obj2 = create_from_type_options(&toml_type_options)?;
        let cache2 = cache_obj2.downcast_ref::<Cache>().unwrap();

        assert_eq!(cache.config, cache2.config);

        Ok(())
    }

    #[test]
    fn test_error_handling() -> Result<()> {
        // 测试未注册类型的错误
        let invalid_config = r#"
        {
            "type": "NonExistentService",
            "options": {}
        }"#;

        let type_options = TypeOptions::from_json(invalid_config)?;
        let result = create_from_type_options(&type_options);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("not registered"));
        assert!(error_msg.contains("NonExistentService"));

        // 测试配置格式错误
        register::<Database, DatabaseConfig>()?;

        let db_type_name = generate_auto_type_name::<Database>();
        let malformed_config = format!(r#"
        {{
            "type": "{}",
            "options": {{
                "driver": "postgresql",
                "port": "invalid_port_number"
            }}
        }}"#, db_type_name);

        let type_options = TypeOptions::from_json(&malformed_config)?;
        let result = create_from_type_options(&type_options);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_optional_fields_and_defaults() -> Result<()> {
        register::<Database, DatabaseConfig>()?;

        let db_type_name = generate_auto_type_name::<Database>();

        // 测试可选字段和默认值
        let minimal_config = format!(r#"
        {{
            "type": "{}",
            "options": {{
                "driver": "sqlite",
                "host": "localhost",
                "port": 3306,
                "database": "test",
                "username": "root",
                "password": "secret"
            }}
        }}"#, db_type_name);

        let type_options = TypeOptions::from_json(&minimal_config)?;
        let db_obj = create_from_type_options(&type_options)?;
        let database = db_obj.downcast_ref::<Database>().unwrap();

        // 验证默认值
        assert_eq!(database.config.max_connections, 10);
        assert!(database.config.ssl_mode.is_none());

        // 测试带可选字段的配置
        let full_config = format!(r#"
        {{
            "type": "{}",
            "options": {{
                "driver": "mysql",
                "host": "db.example.com",
                "port": 3306,
                "database": "production",
                "username": "admin",
                "password": "complex_password",
                "max_connections": 50,
                "ssl_mode": "require"
            }}
        }}"#, db_type_name);

        let type_options = TypeOptions::from_json(&full_config)?;
        let db_obj = create_from_type_options(&type_options)?;
        let database = db_obj.downcast_ref::<Database>().unwrap();

        assert_eq!(database.config.max_connections, 50);
        assert_eq!(database.config.ssl_mode, Some("require".to_string()));

        Ok(())
    }

    #[test]
    fn test_automatic_type_name_generation() -> Result<()> {
        // 验证自动生成的类型名
        let web_server_type = generate_auto_type_name::<WebServer>();
        let database_type = generate_auto_type_name::<Database>();
        let cache_type = generate_auto_type_name::<Cache>();

        // 类型名应该包含完整路径
        assert!(web_server_type.contains("WebServer"));
        assert!(database_type.contains("Database"));
        assert!(cache_type.contains("Cache"));

        // 验证类型名是稳定的
        let web_server_type2 = generate_auto_type_name::<WebServer>();
        assert_eq!(web_server_type, web_server_type2);

        Ok(())
    }
}
