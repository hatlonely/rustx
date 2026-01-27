//! AOP 全局管理器使用示例
//!
//! 演示如何使用 init 初始化全局 AOP 管理器，并使用 aop! 宏为方法添加切面功能

use anyhow::Result;
use rustx::aop::{init, AopManagerConfig};
use rustx::aop; // 导入 aop! 宏
use std::sync::Arc;
use std::time::Duration;

// ========== 模拟服务层 ==========

/// 用户服务
struct UserService {
    // 从全局管理器获取的 aop 实例
    aop: Arc<rustx::aop::Aop>,
}

impl UserService {
    /// 创建新的用户服务
    fn new(aop: Arc<rustx::aop::Aop>) -> Self {
        Self { aop }
    }

    /// 获取用户（参数是引用，不需要 clone）
    ///
    /// aop! 宏会自动添加：
    /// - 日志记录（开始、结束、耗时、结果）
    /// - 重试机制（如果配置了 retry）
    /// - 分布式追踪（如果配置了 tracing）
    async fn get_user(&self, user_id: &str) -> Result<String> {
        // 使用 aop! 宏包装模拟的数据库查询
        aop!(
            Some(&self.aop),
            mock_db_query(user_id).await
        )
    }

    /// 创建用户（参数会被消费，需要 clone 以支持重试）
    ///
    /// 注意：当参数会被移动时，需要使用 clone 指定哪些参数需要在重试时 clone
    async fn create_user(&self, name: String, email: String) -> Result<String> {
        // 使用 aop! 宏，带 clone 参数
        // name 和 email 会在每次重试时被 clone
        aop!(
            Some(&self.aop),
            clone(name, email),
            mock_db_insert(name, email).await
        )
    }
}

// ========== 模拟数据库操作 ==========

/// 模拟数据库查询（可能失败）
async fn mock_db_query(user_id: &str) -> Result<String> {
    // 模拟网络延迟
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 模拟偶发失败（20% 概率）
    if rand::random::<f32>() < 0.2 {
        return Err(anyhow::anyhow!("Database connection failed"));
    }

    Ok(format!("User(id={}, name=Alice, age=30)", user_id))
}

/// 模拟数据库插入（可能失败）
async fn mock_db_insert(name: String, email: String) -> Result<String> {
    // 模拟网络延迟
    tokio::time::sleep(Duration::from_millis(80)).await;

    // 模拟偶发失败（30% 概率）
    if rand::random::<f32>() < 0.3 {
        return Err(anyhow::anyhow!("Database timeout"));
    }

    Ok(format!("User(name={}, email={}) created", name, email))
}

// ========== 主函数 ==========

#[tokio::main]
async fn main() -> Result<()> {
    // ===== 步骤 1: 初始化全局 AOP 管理器 =====

    // 创建 AOP 管理器配置
    let aop_config: AopManagerConfig = json5::from_str(
        r#"
        {
            // 默认 aop（空配置，不启用任何功能）
            default: {},

            // 命名 aop：service_aop
            aops: {
                // 配置一个带 logging、retry、tracing 的 aop
                service_aop: {
                    // Logging 配置
                    logging: {
                        logger: {
                            type: "console",
                            level: "info"
                        },
                        // 成功日志采样率 100%
                        info_sample_rate: 1.0,
                        // 失败日志采样率 100%
                        warn_sample_rate: 1.0
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
                        name: "user_service",
                        with_args: true
                    }
                }
            },

            // 全局 Tracer 配置
            tracer: {
                enabled: true,
                service_name: "aop-example",
                sample_rate: 1.0,
                exporter: {
                    type: "stdout"
                },
                subscriber: {
                    log_level: "info",
                    with_fmt_layer: true
                }
            }
        }
    "#,
    )?;

    // 初始化全局 AOP 管理器（会自动初始化全局 tracer）
    init(aop_config)?;
    println!("✓ 全局 AOP 管理器初始化完成");
    println!("✓ 全局 Tracer 已启用（stdout exporter）\n");

    // ===== 步骤 2: 从全局管理器获取 aop 实例 =====

    let service_aop = rustx::aop::get("service_aop")
        .expect("service_aop should exist");
    println!("✓ 从全局管理器获取 service_aop\n");

    // ===== 步骤 3: 创建服务并使用 aop! 宏 =====

    let service = UserService::new(service_aop);

    // 示例 1: 获取用户（不带 clone 的 aop! 宏）
    println!("===== 示例 1: 获取用户 =====");
    println!("调用 get_user(\"user123\")");
    println!("提示：可能触发重试（模拟数据库偶发失败）\n");

    match service.get_user("user123").await {
        Ok(user) => println!("✅ 成功: {}\n", user),
        Err(e) => println!("❌ 失败: {}\n", e),
    }

    // 示例 2: 创建用户（带 clone 的 aop! 宏）
    println!("===== 示例 2: 创建用户 =====");
    println!("调用 create_user(\"Bob\", \"bob@example.com\")");
    println!("提示：name 和 email 参数会在重试时被 clone\n");

    match service
        .create_user("Bob".to_string(), "bob@example.com".to_string())
        .await
    {
        Ok(result) => println!("✅ 成功: {}\n", result),
        Err(e) => println!("❌ 失败: {}\n", e),
    }

    // 多次调用以展示重试机制
    println!("===== 多次调用以观察重试和日志 =====\n");

    for i in 1..=3 {
        println!("--- 第 {} 次调用 get_user ---", i);
        match service.get_user(&format!("user{}", i)).await {
            Ok(user) => println!("✅ 成功: {}\n", user),
            Err(e) => println!("❌ 失败: {}\n", e),
        }
    }

    println!("✓ 所有示例执行完成");
    println!("\n观察要点：");
    println!("- [AOP] 日志：每次调用都会记录开始、结束、耗时、结果");
    println!("- 重试机制：失败时会自动重试（最多 3 次）");
    println!("- 退避策略：exponential，延迟指数增长（50ms, 100ms, 200ms...）");
    println!("- 抖动：jitter=true，延迟基础上添加随机抖动");
    println!("- 分布式追踪：每个操作都创建了 span（查看下方的 JSON 输出）");
    println!("- fmt layer：可读的日志输出（显示嵌套结构和耗时）");

    Ok(())
}
