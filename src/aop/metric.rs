//! Metric 支持
//!
//! 提供全局的 Prometheus Registry 和 HTTP 服务
//! 同时支持 prometheus-client (aop 模块) 和 tonic_prometheus_layer (gRPC metrics) 的输出

use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use garde::Validate;
use once_cell::sync::Lazy;
use prometheus_client::encoding::text::encode;
use prometheus_client::registry::Registry;
use serde::Deserialize;
use smart_default::SmartDefault;
use std::sync::{Arc, RwLock};
use tokio::{net::TcpListener, sync::OnceCell};

/// 全局 Prometheus Registry (prometheus-client)
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

    /// 是否启用 gRPC metrics (tonic_prometheus_layer)
    #[default = true]
    #[garde(skip)]
    pub with_grpc: bool,
}

/// 保证 init_metric 只被调用一次
static INIT_ONCE: OnceCell<Result<()>> = OnceCell::const_new();

/// 启动 Metric HTTP Server
///
/// 在后台启动一个 Tokio 任务，提供 Prometheus 格式的 metrics 拉取端点
/// 同时支持 prometheus-client (aop) 和 tonic_prometheus_layer (gRPC) 的 metrics
/// 多次调用此函数只会初始化一次，后续调用会等待第一次初始化完成并返回其结果
pub async fn init_metric(config: MetricServerConfig) -> Result<()> {
    INIT_ONCE
        .get_or_init(|| async { init_metric_inner(config).await })
        .await
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{}", e))
        .copied()
}

/// Metrics 处理器
///
/// 合并 prometheus-client 和 tonic_prometheus_layer 两个 registry 的输出
async fn metrics_handler(State(with_grpc): State<bool>) -> Result<impl IntoResponse, StatusCode> {
    let mut buffer = String::new();

    // 1. 编码 prometheus-client registry (aop metrics)
    {
        let registry_guard = GLOBAL_REGISTRY
            .read()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        encode(&mut buffer, &*registry_guard).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    // 2. 如果启用了 gRPC metrics，追加 tonic_prometheus_layer 的输出
    if with_grpc {
        if let Ok(grpc_metrics) = tonic_prometheus_layer::metrics::encode_to_string() {
            if !buffer.is_empty() && !buffer.ends_with('\n') {
                buffer.push('\n');
            }
            buffer.push_str(&grpc_metrics);
        }
    }

    Ok((
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        buffer,
    ))
}

/// 内部初始化函数，实际启动 Metric HTTP Server
async fn init_metric_inner(config: MetricServerConfig) -> Result<()> {
    let path = config.path.clone();
    let port = config.port;

    tokio::spawn(async move {
        let app = Router::new()
            .route(&path, get(metrics_handler))
            .with_state(config.with_grpc);

        let addr = format!("0.0.0.0:{}", port);
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to bind metric server on {}: {}", addr, e);
                return;
            }
        };

        eprintln!("Metric server listening on http://{}{}", addr, path);

        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("Metric server error: {}", e);
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

    #[test]
    fn test_metric_server_config_default() {
        let config = MetricServerConfig::default();
        assert_eq!(config.port, 9090);
        assert_eq!(config.path, "/metrics");
        assert!(config.with_grpc);
    }

    #[test]
    fn test_metric_server_config_deserialize() {
        let config: MetricServerConfig = json5::from_str(
            r#"{
                port: 9091,
                path: "/prometheus",
                with_grpc: false
            }"#,
        )
        .unwrap();

        assert_eq!(config.port, 9091);
        assert_eq!(config.path, "/prometheus");
        assert!(!config.with_grpc);
    }

    #[tokio::test]
    async fn test_init_metric() {
        let config = MetricServerConfig {
            port: 19999,
            path: "/metrics".to_string(),
            with_grpc: false,
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
