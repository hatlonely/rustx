//! 全局配置源使用示例
//!
//! 演示如何使用全局配置源在应用任何地方访问配置

use rustx::cfg::{init, load, register_sources, ConfigChange, TypeOptions, watch};
use serde::Deserialize;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
    username: String,
    database: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
    workers: usize,
}

fn main() -> anyhow::Result<()> {
    println!("=== 全局配置源使用示例 ===\n");

    // 1. 注册所有配置源类型
    println!("1. 注册所有配置源类型");
    register_sources()?;
    println!("   ✅ 配置源类型已注册\n");

    // 2. 初始化全局配置源
    println!("2. 初始化全局配置源（FileSource）");
    let options = TypeOptions::from_json(
        r#"{
        type: "FileSource",
        options: {
            base_path: "examples/configs/cfg"
        }
    }"#,
    )?;

    init(options)?;
    println!("   ✅ 全局配置源已初始化\n");

    // 3. 使用全局 load 函数加载配置
    println!("3. 使用全局 load 函数加载数据库配置");
    let db_config_value = load("database.json5", None)?;
    let db_config: DatabaseConfig = db_config_value.into_type()?;
    println!("   数据库配置: {:?}", db_config);
    println!("   - 主机: {}", db_config.host);
    println!("   - 端口: {}", db_config.port);
    println!("   - 用户名: {}", db_config.username);
    println!("   - 数据库: {}\n", db_config.database);

    // 4. 加载另一个配置
    println!("4. 加载服务器配置");
    let server_config_value = load("server.json5", None)?;
    let server_config: ServerConfig = server_config_value.into_type()?;
    println!("   服务器配置: {:?}", server_config);
    println!("   - 地址: {}:{}", server_config.host, server_config.port);
    println!("   - 工作线程: {}\n", server_config.workers);

    // 5. 使用全局 watch 函数监听配置变化
    println!("5. 启动配置监听");
    watch(
        "database.json5",
        None,
        Box::new(move |change| match change {
            ConfigChange::Updated(new_config) => {
                println!("   ✅ 检测到配置更新！");
                if let Ok(new_db_config) = new_config.as_type::<DatabaseConfig>() {
                    println!("   新数据库配置: {:?}", new_db_config);
                }
            }
            ConfigChange::Deleted => {
                println!("   ⚠️  配置文件已删除");
            }
            ConfigChange::Error(msg) => {
                eprintln!("   ❌ 错误: {}", msg);
            }
        }),
    )?;

    println!("   监听已启动");
    println!("   提示：你可以修改 examples/configs/cfg/database.json5 文件来测试热更新");
    println!("   程序将运行 3 秒后自动退出\n");

    // 6. 保持程序运行以测试配置热更新
    thread::sleep(Duration::from_secs(3));

    println!("程序退出，配置监听自动停止");
    Ok(())
}
