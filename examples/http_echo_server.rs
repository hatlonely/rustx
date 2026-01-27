//! HTTP Echo 服务端示例
//!
//! 演示如何使用 axum 实现一个简单的 HTTP Echo 服务

use axum::{
    extract::Path,
    routing::get,
    Router,
};

// 简单的 echo 处理器
async fn echo(Path(message): Path<String>) -> String {
    println!("收到请求: {}", message);
    format!("echo: {}", message)
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/echo/{message}", get(echo));

    let addr = "[::1]:3000";
    println!("HTTP Echo 服务端启动，监听: http://{}", addr);
    println!("访问示例: http://{}/echo/hello-world", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
