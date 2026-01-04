#[cfg(test)]
mod integration_tests {
    use rustx::cfg::{Configurable, TypeOptions, register, create_from_type_options};
    use rustx::cfg::duration::{serde_as, HumanDur};
    use serde::{Deserialize, Serialize};
    use std::any::Any;
    use std::time::Duration;
    use anyhow::Result;

    // 模拟一个完整的服务配置
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
        auth: bool,
        cors: bool,
        rate_limit: Option<RateLimitConfig>,
    }

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
    struct RateLimitConfig {
        requests_per_minute: u32,
        burst_size: u32,
    }

    #[derive(Debug, PartialEq)]
    struct WebServer {
        config: WebServerConfig,
    }

    impl WebServer {
        fn new(config: WebServerConfig) -> Self {
            Self { config }
        }
    }

    impl Configurable for WebServer {
        type Config = WebServerConfig;
        
        fn from_config(config: Self::Config) -> Result<Box<dyn Any + Send + Sync>> {
            Ok(Box::new(WebServer::new(config)))
        }
        
        fn type_name() -> &'static str {
            "web_server"
        }
    }

    #[test]
    fn test_complete_workflow_json() -> Result<()> {
        register::<WebServer>()?;
        
        let json_config = r#"
        {
            "type": "web_server",
            "options": {
                "host": "0.0.0.0",
                "port": 8080,
                "timeout": "30s",
                "keepalive": "5m",
                "max_connections": 1000,
                "ssl_enabled": true,
                "routes": [
                    {
                        "path": "/api/users",
                        "method": "GET",
                        "handler": "get_users"
                    },
                    {
                        "path": "/api/users",
                        "method": "POST", 
                        "handler": "create_user"
                    }
                ],
                "middleware": {
                    "auth": true,
                    "cors": true,
                    "rate_limit": {
                        "requests_per_minute": 100,
                        "burst_size": 20
                    }
                }
            }
        }"#;
        
        let type_options = TypeOptions::from_json(json_config)?;
        let server_obj = create_from_type_options(&type_options)?;
        let server = server_obj.downcast_ref::<WebServer>().unwrap();
        
        assert_eq!(server.config.host, "0.0.0.0");
        assert_eq!(server.config.port, 8080);
        assert_eq!(server.config.timeout, Duration::from_secs(30));
        assert_eq!(server.config.keepalive, Duration::from_secs(300));
        assert_eq!(server.config.max_connections, 1000);
        assert!(server.config.ssl_enabled);
        assert_eq!(server.config.routes.len(), 2);
        assert_eq!(server.config.routes[0].path, "/api/users");
        assert_eq!(server.config.routes[0].method, "GET");
        assert!(server.config.middleware.auth);
        assert!(server.config.middleware.cors);
        assert!(server.config.middleware.rate_limit.is_some());
        
        let rate_limit = server.config.middleware.rate_limit.as_ref().unwrap();
        assert_eq!(rate_limit.requests_per_minute, 100);
        assert_eq!(rate_limit.burst_size, 20);
        
        Ok(())
    }

    #[test]
    fn test_complete_workflow_yaml() -> Result<()> {
        register::<WebServer>()?;
        
        let yaml_config = r#"
type: web_server
options:
  host: "127.0.0.1"
  port: 3000
  timeout: "1m"
  keepalive: "2m"
  max_connections: 500
  ssl_enabled: false
  routes:
    - path: "/health"
      method: "GET"
      handler: "health_check"
    - path: "/metrics"
      method: "GET"
      handler: "get_metrics"
  middleware:
    auth: false
    cors: true
    rate_limit: null
"#;
        
        let type_options = TypeOptions::from_yaml(yaml_config)?;
        let server_obj = create_from_type_options(&type_options)?;
        let server = server_obj.downcast_ref::<WebServer>().unwrap();
        
        assert_eq!(server.config.host, "127.0.0.1");
        assert_eq!(server.config.port, 3000);
        assert_eq!(server.config.timeout, Duration::from_secs(60));
        assert_eq!(server.config.keepalive, Duration::from_secs(120));
        assert!(!server.config.ssl_enabled);
        assert!(!server.config.middleware.auth);
        assert!(server.config.middleware.cors);
        assert!(server.config.middleware.rate_limit.is_none());
        
        Ok(())
    }

    #[test]
    fn test_format_conversion_workflow() -> Result<()> {
        register::<WebServer>()?;
        
        // 创建基础配置
        let base_config = WebServerConfig {
            host: "localhost".to_string(),
            port: 8000,
            timeout: Duration::from_secs(45),
            keepalive: Duration::from_secs(180),
            max_connections: 800,
            ssl_enabled: true,
            routes: vec![
                RouteConfig {
                    path: "/".to_string(),
                    method: "GET".to_string(),
                    handler: "index".to_string(),
                }
            ],
            middleware: MiddlewareConfig {
                auth: true,
                cors: false,
                rate_limit: Some(RateLimitConfig {
                    requests_per_minute: 60,
                    burst_size: 10,
                }),
            },
        };

        let type_options = TypeOptions {
            type_name: "web_server".to_string(),
            options: serde_json::to_value(base_config.clone())?,
        };

        // JSON -> YAML -> TOML 格式转换
        let json_str = type_options.to_json()?;
        let yaml_str = type_options.to_yaml()?;
        let toml_str = type_options.to_toml()?;
        
        // 从每种格式创建服务实例
        let from_json = TypeOptions::from_json(&json_str)?;
        let from_yaml = TypeOptions::from_yaml(&yaml_str)?;
        let from_toml = TypeOptions::from_toml(&toml_str)?;
        
        let server_json = create_from_type_options(&from_json)?.downcast::<WebServer>().unwrap();
        let server_yaml = create_from_type_options(&from_yaml)?.downcast::<WebServer>().unwrap();
        let server_toml = create_from_type_options(&from_toml)?.downcast::<WebServer>().unwrap();
        
        // 验证所有格式解析的结果一致
        assert_eq!(server_json.config, base_config);
        assert_eq!(server_yaml.config, base_config);
        assert_eq!(server_toml.config, base_config);
        
        Ok(())
    }

    #[test]
    fn test_multiple_service_types() -> Result<()> {
        #[serde_as]
        #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
        struct DatabaseConfig {
            url: String,
            pool_size: u32,
            #[serde_as(as = "HumanDur")]
            connect_timeout: Duration,
        }

        #[derive(Debug, PartialEq)]
        struct Database {
            config: DatabaseConfig,
        }

        impl Configurable for Database {
            type Config = DatabaseConfig;
            
            fn from_config(config: Self::Config) -> Result<Box<dyn Any + Send + Sync>> {
                Ok(Box::new(Database { config }))
            }
            
            fn type_name() -> &'static str {
                "database"
            }
        }

        #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
        struct CacheConfig {
            redis_url: String,
            ttl_seconds: u64,
        }

        #[derive(Debug, PartialEq)]
        struct Cache {
            config: CacheConfig,
        }

        impl Configurable for Cache {
            type Config = CacheConfig;
            
            fn from_config(config: Self::Config) -> Result<Box<dyn Any + Send + Sync>> {
                Ok(Box::new(Cache { config }))
            }
            
            fn type_name() -> &'static str {
                "cache"
            }
        }

        // 注册多个服务类型
        register::<WebServer>()?;
        register::<Database>()?;
        register::<Cache>()?;
        
        // 测试多个配置文件
        let configs_yaml = r#"
services:
  - type: web_server
    options:
      host: "0.0.0.0"
      port: 8080
      timeout: "30s"
      keepalive: "5m"
      max_connections: 1000
      ssl_enabled: true
      routes: []
      middleware:
        auth: true
        cors: true
        rate_limit: null
        
  - type: database
    options:
      url: "postgresql://localhost:5432/mydb"
      pool_size: 20
      connect_timeout: "10s"
      
  - type: cache
    options:
      redis_url: "redis://localhost:6379"
      ttl_seconds: 3600
"#;

        let services_config: serde_yaml::Value = serde_yaml::from_str(configs_yaml)?;
        let services = services_config["services"].as_sequence().unwrap();
        
        for service in services {
            let type_options: TypeOptions = serde_yaml::from_value(service.clone())?;
            let service_obj = create_from_type_options(&type_options)?;
            
            match type_options.type_name.as_str() {
                "web_server" => {
                    let server = service_obj.downcast_ref::<WebServer>().unwrap();
                    assert_eq!(server.config.port, 8080);
                },
                "database" => {
                    let db = service_obj.downcast_ref::<Database>().unwrap();
                    assert_eq!(db.config.pool_size, 20);
                    assert_eq!(db.config.connect_timeout, Duration::from_secs(10));
                },
                "cache" => {
                    let cache = service_obj.downcast_ref::<Cache>().unwrap();
                    assert_eq!(cache.config.ttl_seconds, 3600);
                },
                _ => panic!("Unexpected service type"),
            }
        }
        
        Ok(())
    }

    #[test]
    fn test_error_handling_integration() -> Result<()> {
        register::<WebServer>()?;
        
        // 测试类型不匹配错误
        let wrong_type_config = TypeOptions {
            type_name: "nonexistent_service".to_string(),
            options: serde_json::json!({}),
        };
        
        let result = create_from_type_options(&wrong_type_config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not registered"));
        
        // 测试配置格式错误
        let invalid_config = TypeOptions {
            type_name: "web_server".to_string(),
            options: serde_json::json!({
                "host": "localhost",
                "port": "invalid_port",  // 应该是数字
                "timeout": "30s"
            }),
        };
        
        let result = create_from_type_options(&invalid_config);
        assert!(result.is_err());
        
        // 测试必需字段缺失
        let missing_fields = TypeOptions {
            type_name: "web_server".to_string(),
            options: serde_json::json!({
                "host": "localhost"
                // 缺失其他必需字段
            }),
        };
        
        let result = create_from_type_options(&missing_fields);
        assert!(result.is_err());
        
        Ok(())
    }
}