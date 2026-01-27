//! gRPC Echo 分布式追踪示例
//!
//! 演示如何使用 tonic-tracing-opentelemetry 自动注入和传播 trace 信息

use echo::echo_service_server::{EchoService, EchoServiceServer};
use echo::echo_service_client::EchoServiceClient;
use echo::{EchoRequest, EchoResponse};
use rustx::aop::tracer::{init_tracer, TracerConfig};
use tonic::{Request, Response, Status, transport::Server, transport::Channel};
use tonic_tracing_opentelemetry::middleware::{
    server::OtelGrpcLayer,
    client::OtelGrpcLayer as ClientOtelGrpcLayer
};
use tower::ServiceBuilder;
use tracing::{info, info_span};

// 生成的 proto 代码位于 echo 模块
pub mod echo {
    include!(concat!(env!("OUT_DIR"), "/echo.rs"));
}

// 实现 Echo Service
#[derive(Debug, Default)]
struct MyEchoService {}

#[tonic::async_trait]
impl EchoService for MyEchoService {
    // 使用 instrument 宏自动创建 span
    #[tracing::instrument(skip(self, request))]
    async fn echo(&self, request: Request<EchoRequest>) -> Result<Response<EchoResponse>, Status> {
        let req = request.into_inner();

        // 业务逻辑中的日志会自动关联到当前的 trace
        info!("处理 echo 请求: {}", req.message);

        // 模拟业务处理，创建子操作
        let processed = process_message(&req.message).await;

        let response = EchoResponse {
            message: format!("echo: {}", processed),
        };

        Ok(Response::new(response))
    }
}

// 模拟业务处理函数，展示嵌套 span
#[tracing::instrument]
async fn process_message(message: &str) -> String {
    info!("开始处理消息");
    message.to_string()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 初始化 tracer
    let tracer_config: TracerConfig = json5::from_str(r#"{
        enabled: true,
        service_name: "grpc-echo-server",
        sample_rate: 1.0,
        exporter: {
            type: "stdout"
        },
        subscriber: {
            log_level: "info",
            with_fmt_layer: true
        }
    }"#)?;

    init_tracer(&tracer_config)?;

    // 启动服务端任务
    let server_task = tokio::spawn(run_server());

    // 等待服务端启动
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // 运行客户端
    run_client().await?;

    // 等待服务端任务完成
    server_task.abort();

    Ok(())
}

// 运行服务端
async fn run_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = "[::1]:50051".parse()?;
    let echo_service = MyEchoService::default();

    info!("gRPC Echo 服务端启动，监听: {}", addr);

    Server::builder()
        // 添加 OpenTelemetry 中间件，自动从 metadata 提取 traceparent
        .layer(OtelGrpcLayer::default())
        .add_service(EchoServiceServer::new(echo_service))
        .serve(addr)
        .await?;

    Ok(())
}

// 运行客户端
async fn run_client() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 创建 channel
    let channel = Channel::from_static("http://[::1]:50051")
        .connect()
        .await?;

    // 使用 ServiceBuilder 包装 channel，自动注入 traceparent
    let channel = ServiceBuilder::new()
        .layer(ClientOtelGrpcLayer::default())
        .service(channel);

    let mut client = EchoServiceClient::new(channel);

    // 创建 span 包裹整个客户端调用
    let request_span = info_span!("client_request", service = "echo");
    let _guard = request_span.enter();

    info!("发送 gRPC 请求，trace context 将自动注入");

    let request = Request::new(EchoRequest {
        message: "Hello, Trace!".to_string(),
    });

    // 发送请求，trace context 会自动注入到 metadata
    let response = client.echo(request).await?;
    let echo_response = response.into_inner();

    info!("收到响应: {}", echo_response.message);

    Ok(())
}
