//! gRPC Echo 客户端示例
//!
//! 演示如何调用 gRPC Echo 服务

use echo::echo_service_client::EchoServiceClient;
use echo::EchoRequest;

// 生成的 proto 代码位于 echo 模块
pub mod echo {
    include!(concat!(env!("OUT_DIR"), "/echo.rs"));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 连接到服务端
    let mut client = EchoServiceClient::connect("http://[::1]:50051").await?;
    println!("已连接到 gRPC Echo 服务: http://[::1]:50051\n");

    // 发送 Echo 请求
    let request = tonic::Request::new(EchoRequest {
        message: "Hello, gRPC!".to_string(),
    });

    println!("发送请求: {:?}", request.get_ref().message);

    let response = client.echo(request).await?;
    let echo_response = response.into_inner();

    println!("收到响应: {:?}", echo_response.message);

    Ok(())
}
