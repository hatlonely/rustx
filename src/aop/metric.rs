//! Metric 支持
//!
//! 提供全局的 Prometheus Registry 和 HTTP 服务

use anyhow::Result;
use garde::Validate;
use once_cell::sync::Lazy;
use prometheus_client::registry::Registry;
use serde::Deserialize;
use smart_default::SmartDefault;
use std::sync::{Arc, RwLock};
use tokio::spawn;

/// 全局 Prometheus Registry
///
/// 所有 Aop 实例的指标都注册到这个 Registry
static GLOBAL_REGISTRY: Lazy<Arc<RwLock<Registry>>> =
    Lazy::new(|| Arc::new(RwLock::new(Registry::default())));

/// 获取全局 Registry
pub fn global_registry() -> Arc<RwLock<Registry>> {
    Arc::clone(&GLOBAL_REGISTRY)
}

/// Metric Server 配置
#[derive(Debug, Clone, Deserialize, SmartDefault, Validate, PartialEq)]
#[serde(default)]
pub struct MetricServerConfig {
    /// HTTP 服务端口
    #[default = 9090]
    #[garde(range(min = 1024, max = 65535))]
    pub port: u16,

    /// Metric 端点路径
    #[default = "/metrics"]
    #[garde(length(min = 1))]
    pub path: String,
}

/// 启动 Metric HTTP Server
///
/// 在后台启动一个 Tokio 任务，提供 Prometheus 格式的 metrics 拉取端点
pub async fn init_metric(config: MetricServerConfig) -> Result<()> {
    let registry = Arc::clone(&GLOBAL_REGISTRY);

    spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let addr = format!("0.0.0.0:{}", config.port);
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to bind metric server on {}: {}", addr, e);
                return;
            }
        };

        eprintln!("Metric server listening on http://{}{}", addr, config.path);

        loop {
            match listener.accept().await {
                Ok((mut socket, _)) => {
                    let registry = Arc::clone(&registry);
                    let path = config.path.clone();

                    spawn(async move {
                        let mut buf = [0; 1024];

                        // 读取请求
                        match socket.read(&mut buf).await {
                            Ok(n) if n > 0 => {
                                let request = String::from_utf8_lossy(&buf[..n]);

                                // 解析 GET 请求
                                if request.starts_with("GET ") {
                                    // 提取路径
                                    let request_path =
                                        request.split(' ').nth(1).unwrap_or("/metrics");

                                    // 检查路径是否匹配
                                    if request_path == path {
                                        // 返回 metrics
                                        let mut buffer = String::new();
                                        let encode_result = {
                                            let registry_guard = registry.read().unwrap();
                                            prometheus_client::encoding::text::encode(
                                                &mut buffer,
                                                &*registry_guard,
                                            )
                                        };

                                        match encode_result {
                                            Ok(_) => {
                                                let response = format!(
                                                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                                                    buffer.len(),
                                                    buffer
                                                );
                                                let _ = socket.write_all(response.as_bytes()).await;
                                            }
                                            Err(e) => {
                                                eprintln!("Failed to encode metrics: {}", e);
                                                let response =
                                                    "HTTP/1.1 500 Internal Server Error\r\n\r\n"
                                                        .to_string();
                                                let _ = socket.write_all(response.as_bytes()).await;
                                            }
                                        }
                                    } else {
                                        // 404 Not Found
                                        let response = "HTTP/1.1 404 Not Found\r\n\r\n".to_string();
                                        let _ = socket.write_all(response.as_bytes()).await;
                                    }
                                } else {
                                    // 400 Bad Request
                                    let response = "HTTP/1.1 400 Bad Request\r\n\r\n".to_string();
                                    let _ = socket.write_all(response.as_bytes()).await;
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to read from socket: {}", e);
                            }
                            _ => {}
                        }

                        let _ = socket.shutdown().await;
                    });
                }
                Err(e) => {
                    eprintln!("Failed to accept connection: {}", e);
                }
            }
        }
    });

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_registry() {
        let registry1 = global_registry();
        let registry2 = global_registry();

        // 验证是同一个实例
        assert!(Arc::ptr_eq(&registry1, &registry2));
    }

    #[tokio::test]
    async fn test_init_metric() {
        let config = MetricServerConfig {
            port: 19999,
            path: "/metrics".to_string(),
        };

        // 启动 server
        let result = init_metric(config.clone()).await;
        assert!(result.is_ok());

        // 等待 server 启动
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 使用 reqwest 请求 metrics
        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://localhost:{}{}", config.port, config.path))
            .send()
            .await;

        assert!(response.is_ok());
        let response = response.unwrap();
        assert_eq!(response.status(), 200);

        let _body = response.text().await.unwrap();
        // 由于没有注册任何 metric，这里只验证请求能成功

        // 测试 404
        let response = client
            .get(format!("http://localhost:{}/not_found", config.port))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 404);
    }
}
