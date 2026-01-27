//! gRPC Echo 服务端示例
//!
//! 演示如何使用 tonic 实现一个简单的 gRPC Echo 服务

use echo::echo_service_server::{EchoService, EchoServiceServer};
use echo::{EchoRequest, EchoResponse};
use tonic::{transport::Server, Request, Response, Status};

// 生成的 proto 代码位于 echo 模块
pub mod echo {
    include!(concat!(env!("OUT_DIR"), "/echo.rs"));
}

// 实现 Echo Service
#[derive(Debug, Default)]
struct MyEchoService {}

#[tonic::async_trait]
impl EchoService for MyEchoService {
    async fn echo(&self, request: Request<EchoRequest>) -> Result<Response<EchoResponse>, Status> {
        let req = request.into_inner();
        println!("收到请求: {:?}", req.message);

        let response = EchoResponse {
            message: format!("echo: {}", req.message),
        };

        Ok(Response::new(response))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let echo_service = MyEchoService::default();

    println!("gRPC Echo 服务端启动，监听: {}", addr);

    Server::builder()
        .add_service(EchoServiceServer::new(echo_service))
        .serve(addr)
        .await?;

    Ok(())
}
