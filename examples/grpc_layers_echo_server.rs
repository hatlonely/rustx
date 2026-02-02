//! gRPC Echo 服务端示例（包含 trace layer 和 prometheus metrics）
//!
//! 演示如何使用 tonic 实现一个带 trace layer 和 prometheus metrics 的 gRPC Echo 服务
//! metrics 通过 rustx::aop::metric 模块统一管理，支持 aop metrics 和 gRPC metrics 合并输出
//!
//! 本示例还演示了如何在 gRPC 服务方法中使用 AOP 宏包装异步方法，
//! 实现 logging、retry、metric 等切面功能
//!
//! 遵循类设计规范：
//! - 使用 MyEchoServiceConfig 配置结构体
//! - 通过 new 方法构造服务实例
//! - 从 AopManager 获取专用的 aop 实例

use anyhow::Result;
use echo::echo_service_server::{EchoService, EchoServiceServer};
use echo::{EchoRequest, EchoResponse};
use rustx::aop::{Aop, AopConfig, AopManagerConfig};
use serde::Deserialize;
use smart_default::SmartDefault;
use std::sync::Arc;
use tonic::{transport::Server, Request, Response, Status};

// 生成的 proto 代码位于 echo 模块
pub mod echo {
    include!(concat!(env!("OUT_DIR"), "/echo.rs"));
}

// MyEchoService 配置
#[derive(Debug, Clone, Deserialize, SmartDefault)]
#[serde(default)]
pub struct MyEchoServiceConfig {
    /// aop 配置，支持 Create 和 Reference 两种模式
    pub aop: Option<AopConfig>,
}

// 实现 Echo Service
struct MyEchoService {
    aop: Option<Arc<Aop>>,
}

impl MyEchoService {
    /// 使用配置创建新的 EchoService 实例
    pub fn new(config: MyEchoServiceConfig) -> Result<Self> {
        // 使用 resolve 方法，支持 Reference 和 Create 两种模式
        let aop = config.aop.map(|config| Aop::resolve(config)).transpose()?;
        Ok(Self { aop })
    }

    /// 使用 aop 宏包装的异步方法处理消息
    async fn process_message(
        &self,
        message: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // 定义内部处理逻辑
        async fn internal_process(
            msg: &str,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            // 模拟一些处理逻辑
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            Ok(format!("echo: {}", msg))
        }

        // 使用 aop! 宏包装异步调用
        _ = rustx::aop!(self.aop.as_ref(), internal_process(message).await);
        _ = rustx::aop!(self.aop.as_ref(), internal_process(message).await);

        rustx::aop!(self.aop.as_ref(), internal_process(message).await)
    }
}

#[tonic::async_trait]
impl EchoService for MyEchoService {
    async fn echo(&self, request: Request<EchoRequest>) -> Result<Response<EchoResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("收到请求: {:?}", req.message);

        // 使用 aop 宏包装的异步方法处理消息
        let processed_message = self
            .process_message(&req.message)
            .await
            .map_err(|e| Status::internal(format!("处理消息失败: {}", e)))?;

        let response = EchoResponse {
            message: processed_message,
        };

        Ok(Response::new(response))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 使用 aop::init 统一初始化 tracer 和 metric，并配置 echo_service 专用的 aop
    let aop_config: AopManagerConfig = json5::from_str(
        r#"{
        default: {},
        aops: {
            // 为 echo_service 配置专用的 aop 切面
            echo_service: {
                // Logging 配置
                logging: {
                    logger: {
                        type: "console",
                        level: "info"
                    },
                    info_sample_rate: 0,
                    warn_sample_rate: 0
                },
                // Retry 配置
                retry: {
                    max_times: 3,
                    strategy: "exponential",
                    min_delay: "50ms",
                    max_delay: "500ms",
                    factor: 2.0,
                    jitter: true
                },
                // Tracing 配置
                tracing: {
                    name: "echo_service",
                    with_args: true
                },
                // Metrics 配置
                metrics: {
                    prefix: "grpc_echo",
                    labels: {
                        service: "grpc-echo",
                        instance: "echo-1"
                    }
                }
            }
        },
        global_tracing: {
            enabled: true,
            service_name: "grpc-echo-server",
            sample_rate: 1,
            exporter: {
                type: "otlp",
                endpoint: "http://localhost:4317",
                timeout: "10s"
            },
            subscriber: {
                log_level: "info",
                with_fmt_layer: true
            }
        },
        global_metrics: {
            port: 9091,
            path: "/metrics"
        }
    }"#,
    )?;
    rustx::aop::init(aop_config)?;

    // 使用配置创建 EchoService 实例
    let service_config: MyEchoServiceConfig = json5::from_str(
        r#"{
        // 使用 Reference 模式引用全局注册的 echo_service aop
        aop: {
            $instance: "echo_service"
        }
    }"#,
    )?;
    let echo_service = MyEchoService::new(service_config)?;

    let addr = "[::1]:50051".parse()?;

    tracing::info!("gRPC Echo 服务端启动，监听: {}", addr);

    Server::builder()
        .layer(rustx::aop::grpc_metrics_layer())
        .layer(rustx::aop::grpc_tracing_layer())
        .add_service(EchoServiceServer::new(echo_service))
        .serve(addr)
        .await?;

    Ok(())
}
