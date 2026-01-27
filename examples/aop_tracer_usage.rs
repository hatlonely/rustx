//! AOP Tracer 使用示例
//!
//! 演示如何使用 init_tracer 直接设置全局 tracer，
//! 并在同步/异步方法中使用 tracing::instrument 进行追踪

use anyhow::Result;
use rustx::aop::tracer::{init_tracer, TracerConfig};
use std::time::Duration;

// ========== 同步方法示例 ==========

/// 简单的同步函数，使用 instrument 宏自动创建 span
#[tracing::instrument]
fn sync_add(x: i32, y: i32) -> i32 {
    tracing::info!("calculating sum");
    x + y
}

/// 嵌套调用的同步函数
#[tracing::instrument]
fn sync_multiply(x: i32, y: i32) -> i32 {
    tracing::info!("multiplying");
    // 嵌套调用另一个带 instrument 的函数
    let result = x * y;
    sync_add(result, 10) // 先乘后加
}

/// 手动创建 span 的同步函数
fn sync_manual_divide(x: i32, y: i32) -> Result<i32> {
    let span = tracing::info_span!("manual_divide", dividend = x, divisor = y);
    let _enter = span.enter();

    tracing::info!("dividing");
    if y == 0 {
        return Err(anyhow::anyhow!("division by zero"));
    }
    Ok(x / y)
}

// ========== 异步方法示例 ==========

/// 简单的异步函数，使用 instrument 宏
#[tracing::instrument]
async fn async_fetch_user(user_id: &str) -> String {
    tracing::info!("fetching user from database");
    tokio::time::sleep(Duration::from_millis(50)).await;
    format!("User({})", user_id)
}

/// 异步函数嵌套调用
#[tracing::instrument]
async fn async_process_order(user_id: &str, order_id: &str) -> String {
    tracing::info!("processing order");

    // 调用另一个带 instrument 的异步函数
    let user = async_fetch_user(user_id).await;

    tokio::time::sleep(Duration::from_millis(30)).await;
    format!("{} processed Order({})", user, order_id)
}

/// 未加 instrument 的异步函数
async fn async_external_api_call(endpoint: &str) -> String {
    tracing::info!("calling external API");
    tokio::time::sleep(Duration::from_millis(50)).await;
    format!("Response from {}", endpoint)
}

/// 使用 Instrument trait 包装未加宏的异步函数
async fn async_call_external_with_span(endpoint: &str) -> String {
    use tracing::Instrument;

    let span = tracing::info_span!("external_api_wrapper", endpoint);
    async_external_api_call(endpoint).instrument(span).await
}

// ========== 并发任务示例 ==========

#[tracing::instrument]
async fn async_worker(worker_id: i32, duration_ms: u64) -> i32 {
    tracing::info!("worker started");
    tokio::time::sleep(Duration::from_millis(duration_ms)).await;
    tracing::info!("worker completed");
    worker_id
}

async fn run_concurrent_workers() -> (i32, i32, i32) {
    use tracing::Instrument;

    // 为每个任务创建独立的 span
    let task1 = async_worker(1, 50).instrument(tracing::info_span!("worker_1"));
    let task2 = async_worker(2, 30).instrument(tracing::info_span!("worker_2"));
    let task3 = async_worker(3, 40).instrument(tracing::info_span!("worker_3"));

    tokio::join!(task1, task2, task3)
}

// ========== 混合示例 ==========

#[tracing::instrument]
fn sync_preprocessing(value: i32) -> i32 {
    tracing::info!("preprocessing");
    value * 2
}

#[tracing::instrument]
async fn async_postprocessing(value: i32) -> i32 {
    tracing::info!("postprocessing");
    tokio::time::sleep(Duration::from_millis(20)).await;
    value + 100
}

#[tracing::instrument]
async fn mixed_workflow(input: i32) -> i32 {
    tracing::info!("starting mixed workflow");

    // 同步处理
    let step1 = sync_preprocessing(input);

    // 异步处理
    let step2 = async_postprocessing(step1).await;

    step2
}

#[tokio::main]
async fn main() -> Result<()> {
    // ===== 初始化 Tracer =====
    let tracer_config: TracerConfig = json5::from_str(
        r#"
        {
            enabled: true,
            service_name: "tracer-example",
            sample_rate: 1.0,
            exporter: {
                type: "stdout"
            }
        }
    "#,
    )?;

    init_tracer(&tracer_config)?;
    println!("✓ Tracer initialized with stdout exporter\n");
    println!("提示：init_tracer 已自动初始化 tracing_subscriber");
    println!("- OTEL spans 导出到 stdout (JSON 格式)");
    println!("- 可读日志输出到控制台\n");

    // ===== 示例 1: 同步方法 =====
    println!("===== 示例 1: 同步方法 =====");
    let result1 = sync_add(10, 20);
    println!("sync_add(10, 20) = {}\n", result1);

    let result2 = sync_multiply(5, 3);
    println!("sync_multiply(5, 3) = {}\n", result2);

    match sync_manual_divide(100, 4) {
        Ok(result) => println!("sync_manual_divide(100, 4) = {}\n", result),
        Err(e) => println!("Error: {}\n", e),
    }

    // ===== 示例 2: 异步方法 =====
    println!("===== 示例 2: 异步方法 =====");
    let user = async_fetch_user("user123").await;
    println!("Result: {}\n", user);

    let order_result = async_process_order("user456", "order789").await;
    println!("Result: {}\n", order_result);

    // ===== 示例 3: 包装未加 instrument 的异步函数 =====
    println!("===== 示例 3: 包装未加 instrument 的异步函数 =====");
    let api_result = async_call_external_with_span("https://api.example.com").await;
    println!("Result: {}\n", api_result);

    // ===== 示例 4: 并发任务 =====
    println!("===== 示例 4: 并发任务 =====");
    let (r1, r2, r3) = run_concurrent_workers().await;
    println!("Concurrent results: {}, {}, {}\n", r1, r2, r3);

    // ===== 示例 5: 混合同步/异步工作流 =====
    println!("===== 示例 5: 混合工作流 =====");
    let final_result = mixed_workflow(50).await;
    println!("Final result: {} (50 * 2 + 100 = 200)\n", final_result);

    // ===== 示例 6: 错误处理 =====
    println!("===== 示例 6: 错误处理 =====");
    match sync_manual_divide(10, 0) {
        Ok(_) => println!("Division succeeded"),
        Err(e) => println!("Expected error: {}", e),
    }

    println!("\n✓ 所有示例执行完成");
    println!("提示：向上滚动查看完整的 trace 输出");
    println!("- 可读日志: fmt layer 输出（带嵌套结构和耗时）");
    println!("- OTEL spans: OpenTelemetry JSON 格式（在下方）\n");

    // 注意：tracer provider 会在 drop 时自动 shutdown
    // 如需手动控制 shutdown，可以使用 TracerProviderGuard

    Ok(())
}
