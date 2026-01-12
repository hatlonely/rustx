//! FileSource 使用示例
//!
//! 演示如何使用 FileSource 加载和监听配置文件

use rustx::cfg::{ConfigChange, ConfigSource, FileSource};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    println!("=== FileSource 使用示例 ===\n");

    // 1. 创建文件配置源
    println!("1. 创建 FileSource，指向 examples/cfg/configs 目录");
    let source = FileSource::new("examples/cfg/configs");

    // 2. 加载配置
    println!("2. 加载数据库配置");
    let db_config = source.load("database")?;
    println!("   配置类型: {}", db_config.type_name);
    println!("   配置内容: {}\n", serde_json::to_string_pretty(&db_config.options)?);

    // 3. 使用 Arc<RwLock> 存储配置，支持并发读写
    let config = Arc::new(RwLock::new(db_config));
    println!("3. 将配置存储到 Arc<RwLock> 中，支持并发访问\n");

    // 4. 启动配置监听
    println!("4. 启动配置监听");
    let config_clone = config.clone();
    source.watch("database", move |change| {
        match change {
            ConfigChange::Updated(new_config) => {
                println!("   ✅ 检测到配置更新！");
                println!("   新配置: {}",
                    serde_json::to_string_pretty(&new_config.options).unwrap());

                // 更新配置
                *config_clone.write().unwrap() = new_config;
            }
            ConfigChange::Deleted => {
                println!("   ⚠️  配置文件已删除");
            }
            ConfigChange::Error(msg) => {
                eprintln!("   ❌ 错误: {}", msg);
            }
        }
    })?;
    println!("   监听已启动\n");

    // 5. 模拟应用运行，定期读取配置
    println!("5. 应用运行中，定期读取配置...");
    println!("   提示：你可以修改 examples/cfg/configs/database.json 文件来测试热更新");
    println!("   程序将运行 30 秒后自动退出\n");

    for i in 1..=6 {
        thread::sleep(Duration::from_secs(5));

        let current_config = config.read().unwrap();
        println!("   [{}] 当前配置类型: {}", i, current_config.type_name);

        // 读取具体配置值
        if let Some(host) = current_config.options.get("host") {
            println!("       数据库地址: {}", host);
        }
        if let Some(port) = current_config.options.get("port") {
            println!("       数据库端口: {}", port);
        }
    }

    println!("\n程序即将退出，所有配置监听将自动停止");

    // source drop 时，所有监听自动停止
    Ok(())
}
