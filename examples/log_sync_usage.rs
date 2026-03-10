// 宏会自动通过 #[macro_export] 导出到 crate root
use rustx::log::*;
use std::thread;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    // 初始化日志系统
    let config: LoggerCreateConfig = json5::from_str(
        r#"
        {
            level: "debug",
            formatter: {
                type: "TextFormatter",
                options: {
                    colored: true
                }
            },
            appender: {
                type: "ConsoleAppender",
                options: {
                    target: "stdout",
                    auto_flush: true
                }
            }
        }
    "#,
    )?;

    let logger = Logger::new(config)?;

    println!("=== 同步日志接口演示 ===\n");

    // 1. 使用同步方法直接调用
    println!("--- 1. 使用 Logger 的同步方法 ---");
    logger.info_sync("应用启动")?;
    logger.debug_sync("调试信息")?;
    logger.warn_sync("警告信息")?;
    logger.error_sync("错误信息")?;
    logger.trace_sync("跟踪信息")?;

    // 2. 设置全局 logger 并使用全局同步函数
    println!("\n--- 2. 使用全局同步函数 ---");
    rustx::log::add("main".to_string(), logger);

    rustx::log::info_sync("全局 info 日志")?;
    rustx::log::debug_sync("全局 debug 日志")?;
    rustx::log::warn_sync("全局 warn 日志")?;
    rustx::log::error_sync("全局 error 日志")?;
    rustx::log::trace_sync("全局 trace 日志")?;

    // 3. 使用带 metadata 的全局同步函数
    println!("\n--- 3. 使用带 metadata 的全局同步函数 ---");
    rustx::log::infom_sync(
        "全局用户登录",
        vec![("user_id", 99999i64.into()), ("ip", "192.168.1.1".into())],
    )?;
    rustx::log::warnm_sync(
        "全局警告",
        vec![("metric", "cpu_usage".into()), ("value", 85.5.into())],
    )?;
    rustx::log::errorm_sync(
        "全局错误",
        vec![("service", "database".into()), ("error", "timeout".into())],
    )?;

    // 4. 在多线程环境中使用同步日志
    println!("\n--- 4. 在多线程环境中使用同步日志 ---");
    let handles: Vec<_> = (0..5)
        .map(|_i| {
            thread::spawn(move || {
                let _ = rustx::log::info_sync("线程开始");
                thread::sleep(Duration::from_millis(10));
                let _ = rustx::log::info_sync("线程结束");
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    println!("\n=== 演示完成 ===");
    Ok(())
}
