//! FileSource 使用示例
//!
//! 演示如何使用 FileSource 加载和监听配置文件

use rustx::cfg::serde_duration::{serde_as, HumanDur};
use rustx::cfg::{ConfigChange, ConfigSource, FileSource, FileSourceConfig};
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
    username: String,
    database: String,
    max_connections: u32,
    #[serde_as(as = "HumanDur")]
    timeout: Duration,
}

fn main() -> anyhow::Result<()> {
    println!("=== FileSource 使用示例 ===\n");

    // 1. 创建文件配置源
    println!("1. 创建 FileSource，指向 examples/cfg/configs 目录");
    let source = FileSource::new(FileSourceConfig {
        base_path: "examples/cfg/configs".to_string(),
    });

    // 2. 使用 load 加载配置并反序列化为结构体
    println!("2. 加载数据库配置");
    let config = source.load("database")?;
    let db_config: DatabaseConfig = config.into_type()?;
    println!("   配置: {:?}\n", db_config);

    // 3. 使用 watch 监听配置变化
    println!("3. 启动配置监听");
    source.watch("database", Box::new(move |change| match change {
        ConfigChange::Updated(new_config) => {
            println!("   ✅ 检测到配置更新！");
            if let Ok(new_db_config) = new_config.as_type::<DatabaseConfig>() {
                println!("   新配置: {:?}", new_db_config);
            }
        }
        ConfigChange::Deleted => {
            println!("   ⚠️  配置文件已删除");
        }
        ConfigChange::Error(msg) => {
            eprintln!("   ❌ 错误: {}", msg);
        }
    }))?;

    println!("   监听已启动");
    println!("   提示：你可以修改 examples/cfg/configs/database.json 文件来测试热更新");
    println!("   程序将运行 30 秒后自动退出\n");

    // 4. 保持程序运行以测试配置热更新
    thread::sleep(Duration::from_secs(30));

    println!("程序退出，配置监听自动停止");
    Ok(())
}
