//! ApolloSource 使用示例
//!
//! 演示如何使用 ApolloSource 从 Apollo 配置中心加载和监听配置

use rustx::TypeOptions;
use rustx::cfg::{ApolloSource, ApolloSourceConfig, ConfigChange, ConfigSource};
use std::thread;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    println!("=== ApolloSource 使用示例 ===\n");

    // 1. 创建 Apollo 配置源
    println!("1. 创建 ApolloSource，连接到 Apollo 配置中心");
    let source = ApolloSource::new(ApolloSourceConfig {
        server_url: "http://localhost:8080".to_string(),
        app_id: "test-app".to_string(),
        namespace: "application".to_string(),
        cluster: "default".to_string(),
    })?;
    println!("   已连接到: http://localhost:8080\n");

    // 2. 使用 load 加载配置并反序列化为结构体
    println!("2. 加载数据库配置");
    let db_config: TypeOptions = source.load("database")?.into_type()?;
    println!("   配置: {:?}\n", db_config);

    // 3. 使用 watch 监听配置变化
    // 注意：watch 仅监听变化，不会立即触发回调
    // 因此需要先 load 获取初始配置，再 watch 监听后续变化
    println!("3. 启动配置监听");
    source.watch("database", Box::new(move |change| match change {
        ConfigChange::Updated(new_config) => {
            println!("   ✅ 检测到配置更新！");
            if let Ok(new_db_config) = new_config.as_type::<TypeOptions>() {
                println!("   新配置: {:?}", new_db_config);
            }
        }
        ConfigChange::Deleted => {
            println!("   ⚠️  配置已删除");
        }
        ConfigChange::Error(msg) => {
            eprintln!("   ❌ 错误: {}", msg);
        }
    }))?;

    println!("   监听已启动（使用 Apollo 长轮询机制）");
    println!("   提示：只有配置发生变化时才会触发回调");
    println!("   你可以在 Apollo 控制台修改 database 配置来测试热更新");
    println!("   Apollo 控制台地址: http://localhost:8070");
    println!("   程序将运行 60 秒后自动退出\n");

    // 4. 保持程序运行以测试配置热更新
    thread::sleep(Duration::from_secs(60));

    println!("程序退出，配置监听自动停止");
    Ok(())
}
