//! gRPC Echo 服务端示例（包含 trace layer 和 prometheus metrics）
//!
//! 演示如何使用 tonic 实现一个带 trace layer 和 prometheus metrics 的 gRPC Echo 服务
//! metrics 通过 rustx::aop::metric 模块统一管理，支持 aop metrics 和 gRPC metrics 合并输出

use echo::echo_service_server::{EchoService, EchoServiceServer};
use echo::{EchoRequest, EchoResponse};
use rustx::aop::metric::{init_metric, MetricServerConfig};
use rustx::aop::tracer::{init_tracer, TracerConfig};
use tonic::{transport::Server, Request, Response, Status};
use tonic_prometheus_layer::metrics::GlobalSettings;
use tonic_tracing_opentelemetry::middleware::server;

// 生成的 proto 代码位于 echo 模块
pub mod echo {
    include!(concat!(env!("OUT_DIR"), "/echo.rs"));
}

// 实现 Echo Service
#[derive(Debug, Default)]
struct MyEchoService {}

#[tonic::async_trait]
impl EchoService for MyEchoService {
    #[tracing::instrument(skip(self))]
    async fn echo(&self, request: Request<EchoRequest>) -> Result<Response<EchoResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("收到请求: {:?}", req.message);

        let response = EchoResponse {
            message: format!("echo: {}", req.message),
        };

        Ok(Response::new(response))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 tracer
    let tracer_config: TracerConfig = json5::from_str(
        r#"{
        enabled: true,
        service_name: "grpc-echo-server",
        sample_rate: 1.0,
        exporter: {
            type: "none"
        },
        subscriber: {
            log_level: "info",
            with_fmt_layer: true
        }
    }"#,
    )?;
    init_tracer(&tracer_config)?;

    // 初始化 gRPC metrics (tonic_prometheus_layer)
    tonic_prometheus_layer::metrics::try_init_settings(GlobalSettings {
        histogram_buckets: vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
        ..Default::default()
    })?;
    let metrics_layer = tonic_prometheus_layer::MetricsLayer::new();

    // 启动统一的 Metric HTTP 服务器（默认启用 gRPC metrics）
    let metric_config: MetricServerConfig = json5::from_str(
        r#"{
        port: 9091,
        path: "/metrics"
    }"#,
    )?;
    init_metric(metric_config).await?;

    let addr = "[::1]:50051".parse()?;
    let echo_service = MyEchoService::default();

    tracing::info!("gRPC Echo 服务端启动，监听: {}", addr);

    Server::builder()
        .layer(metrics_layer)
        .layer(server::OtelGrpcLayer::default())
        .add_service(EchoServiceServer::new(echo_service))
        .serve(addr)
        .await?;

    Ok(())
}
