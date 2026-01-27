//! HTTP Echo 客户端示例
//!
//! 演示如何使用 reqwest 调用 HTTP 服务

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = "http://[::1]:3000/echo/hello-http";
    println!("发送 GET 请求: {}\n", url);

    let response = reqwest::get(url).await?;
    let status = response.status();
    let body = response.text().await?;

    println!("状态码: {}", status);
    println!("响应内容: {}", body);

    Ok(())
}
