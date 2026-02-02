//! Metric 支持
//!
//! 提供全局的 Prometheus Registry 和 HTTP 服务
//! 同时支持 prometheus-client (aop 模块)、tonic_prometheus_layer (gRPC metrics) 和 axum-prometheus (HTTP metrics) 的输出

use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use garde::Validate;
use once_cell::sync::{Lazy, OnceCell};
use prometheus_client::encoding::text::encode;
use prometheus_client::registry::Registry;
use serde::Deserialize;
use smart_default::SmartDefault;
use std::sync::{Arc, RwLock};
use tokio::{net::TcpListener, sync::OnceCell as TokioOnceCell};

/// 全局 Prometheus Registry (prometheus-client)
///
/// 所有 Aop 实例的指标都注册到这个 Registry
static GLOBAL_REGISTRY: Lazy<Arc<RwLock<Registry>>> =
    Lazy::new(|| Arc::new(RwLock::new(Registry::default())));

/// 全局 HTTP Prometheus Handle (axum-prometheus)
///
/// axum-prometheus 的 metrics handle 用于渲染 HTTP metrics
static GLOBAL_HTTP_HANDLE: Lazy<
    Arc<RwLock<Option<axum_prometheus::metrics_exporter_prometheus::PrometheusHandle>>>,
> = Lazy::new(|| Arc::new(RwLock::new(None)));

/// HTTP metrics 初始化标志
static HTTP_METRICS_INIT: OnceCell<()> = OnceCell::new();

/// 获取全局 Registry
pub fn global_registry() -> Arc<RwLock<Registry>> {
    Arc::clone(&GLOBAL_REGISTRY)
}

/// 初始化 HTTP metrics 并返回 PrometheusMetricLayer
///
/// 自动创建 axum-prometheus 的 metrics layer 和 handle，并注册到全局 metrics handler。
/// 多次调用此函数只会注册一次 handle，但每次都会返回新的 layer（共享同一个 recorder）。
///
/// **注意**：此函数需要在 Tokio runtime 上下文中调用，且只能在第一次调用时成功初始化
/// 全局 metrics recorder。如果已经初始化过，后续调用会返回新的 layer 但不创建新的 recorder。
///
/// # 使用示例
///
/// ```rust
/// use rustx::aop::metric::http_metric_layer;
///
/// let metric_layer = http_metric_layer();
/// let app = Router::new()
///     .route("/", get(handler))
///     .layer(metric_layer);
/// ```
///
/// # Panics
///
/// 如果不在 Tokio runtime 上下文中调用，会 panic。
pub fn http_metric_layer() -> axum_prometheus::PrometheusMetricLayer<'static> {
    // 使用 OnceCell 确保 handle 只创建一次
    HTTP_METRICS_INIT.get_or_init(|| {
        let (_, handle) = axum_prometheus::PrometheusMetricLayer::pair();
        *GLOBAL_HTTP_HANDLE.write().unwrap() = Some(handle);
    });

    // 返回新的 layer（会复用已设置的全局 recorder）
    axum_prometheus::PrometheusMetricLayer::new()
}

/// gRPC metrics 初始化标志
static GRPC_METRICS_INIT: OnceCell<()> = OnceCell::new();

/// 初始化 gRPC metrics 并返回 MetricsLayer
///
/// 自动初始化 tonic_prometheus_layer 的 metrics 和返回 MetricsLayer。
/// 多次调用此函数只会初始化一次，后续调用会返回新的 layer。
///
/// **注意**：此函数需要在 Tokio runtime 上下文中调用。
///
/// # 使用示例
///
/// ```rust,ignore
/// use rustx::aop::metric::grpc_metric_layer;
///
/// let metrics_layer = grpc_metric_layer();
/// Server::builder()
///     .layer(metrics_layer)
///     .add_service(service)
///     .serve(addr)
///     .await?;
/// ```
///
/// # Panics
///
/// 如果不在 Tokio runtime 上下文中调用，会 panic。
pub fn grpc_metric_layer() -> tonic_prometheus_layer::MetricsLayer {
    // 使用 OnceCell 确保只初始化一次
    GRPC_METRICS_INIT.get_or_init(|| {
        tonic_prometheus_layer::metrics::try_init_settings(
            tonic_prometheus_layer::metrics::GlobalSettings {
                histogram_buckets: vec![
                    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
                ],
                ..Default::default()
            },
        )
        .expect("failed to initialize gRPC metrics");
    });

    // 返回新的 layer
    tonic_prometheus_layer::MetricsLayer::new()
}

/// Global Metrics Server 配置
#[derive(Debug, Clone, Deserialize, SmartDefault, Validate, PartialEq)]
#[serde(default)]
pub struct GlobalMetricsConfig {
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

    /// 是否启用 HTTP metrics (axum-prometheus)
    #[default = true]
    #[garde(skip)]
    pub with_http: bool,
}

/// 保证 init_metric 只被调用一次
static INIT_ONCE: TokioOnceCell<Result<()>> = TokioOnceCell::const_new();

/// 启动 Global Metrics HTTP Server
///
/// 在后台启动一个 Tokio 任务，提供 Prometheus 格式的 metrics 拉取端点
/// 同时支持 prometheus-client (aop) 和 tonic_prometheus_layer (gRPC) 的 metrics
/// 多次调用此函数只会初始化一次，后续调用会等待第一次初始化完成并返回其结果
pub async fn init_metric(config: GlobalMetricsConfig) -> Result<()> {
    INIT_ONCE
        .get_or_init(|| async { init_metric_inner(config).await })
        .await
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{}", e))
        .copied()
}

/// Metrics 处理器状态
#[derive(Clone)]
struct MetricsState {
    with_grpc: bool,
    with_http: bool,
}

/// Metrics 处理器
///
/// 合并 prometheus-client、tonic_prometheus_layer 和 axum-prometheus 三个 registry 的输出
async fn metrics_handler(State(state): State<MetricsState>) -> Result<impl IntoResponse, StatusCode> {
    let mut buffer = String::new();

    // 1. 编码 prometheus-client registry (aop metrics)
    {
        let registry_guard = GLOBAL_REGISTRY
            .read()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        encode(&mut buffer, &*registry_guard).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    // 2. 如果启用了 gRPC metrics，追加 tonic_prometheus_layer 的输出
    if state.with_grpc {
        if let Ok(grpc_metrics) = tonic_prometheus_layer::metrics::encode_to_string() {
            if !buffer.is_empty() && !buffer.ends_with('\n') {
                buffer.push('\n');
            }
            buffer.push_str(&grpc_metrics);
        }
    }

    // 3. 如果启用了 HTTP metrics，追加 axum-prometheus 的输出
    if state.with_http {
        if let Ok(handle_guard) = GLOBAL_HTTP_HANDLE.read() {
            if let Some(handle) = handle_guard.as_ref() {
                let http_metrics = handle.render();
                if !http_metrics.is_empty() {
                    if !buffer.is_empty() && !buffer.ends_with('\n') {
                        buffer.push('\n');
                    }
                    buffer.push_str(&http_metrics);
                }
            }
        }
    }

    Ok((
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        buffer,
    ))
}

/// 内部初始化函数，实际启动 Global Metrics HTTP Server
async fn init_metric_inner(config: GlobalMetricsConfig) -> Result<()> {
    let path = config.path.clone();
    let port = config.port;

    tokio::spawn(async move {
        let state = MetricsState {
            with_grpc: config.with_grpc,
            with_http: config.with_http,
        };
        let app = Router::new()
            .route(&path, get(metrics_handler))
            .with_state(state);

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
    fn test_global_metrics_server_config_default() {
        let config = GlobalMetricsConfig::default();
        assert_eq!(config.port, 9090);
        assert_eq!(config.path, "/metrics");
        assert!(config.with_grpc);
        assert!(config.with_http);
    }

    #[test]
    fn test_global_metrics_server_config_deserialize() {
        let config: GlobalMetricsConfig = json5::from_str(
            r#"{
                port: 9091,
                path: "/prometheus",
                with_grpc: false,
                with_http: false
            }"#,
        )
        .unwrap();

        assert_eq!(config.port, 9091);
        assert_eq!(config.path, "/prometheus");
        assert!(!config.with_grpc);
        assert!(!config.with_http);
    }

    // 注意：test_http_metric_layer 已移除，因为 http_metric_layer()
    // 需要设置全局 metrics recorder，这在单元测试环境中会冲突。
    // 该功能已在 examples/http_layers_echo_server.rs 中通过集成测试验证。

    #[tokio::test]
    async fn test_init_global_metrics() {
        let config = GlobalMetricsConfig {
            port: 19999,
            path: "/metrics".to_string(),
            with_grpc: false,
            with_http: false,
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
