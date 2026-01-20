//! Configurable 使用示例
//!
//! 展示如何使用 Configurable trait 创建支持热更新的配置对象

use rustx::cfg::{ConfigReloader, Configurable, FileSource, FileSourceConfig};
use serde::Deserialize;

/// 数据库配置
#[derive(Debug, Deserialize, Clone)]
struct DatabaseConfig {
    host: String,
    port: u16,
}

/// 数据库服务
#[derive(Debug)]
struct DatabaseService {
    config: DatabaseConfig,
}

impl DatabaseService {
    /// 获取连接信息
    fn connection_info(&self) -> String {
        format!("{}:{}", self.config.host, self.config.port)
    }
}

// 从配置创建服务
impl From<DatabaseConfig> for DatabaseService {
    fn from(config: DatabaseConfig) -> Self {
        println!("初始化数据库服务: {}:{}", config.host, config.port);
        Self { config }
    }
}

// 实现配置热更新
impl ConfigReloader<DatabaseConfig> for DatabaseService {
    fn reload_config(&mut self, config: DatabaseConfig) -> anyhow::Result<()> {
        println!("更新数据库配置: {}:{} -> {}:{}",
            self.config.host, self.config.port, config.host, config.port);
        self.config = config;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    // 1. 创建文件配置源
    let source = FileSource::new(FileSourceConfig {
        base_path: "examples/configs/cfg".to_string(),
    });

    // 2. 一次性创建对象（无热更新）
    println!("=== 一次性创建 ===");
    let service: DatabaseService = source.create::<DatabaseService, DatabaseConfig>("database")?;
    println!("连接信息: {}\n", service.connection_info());

    // 3. 创建对象并自动监听配置变化（支持热更新）
    println!("=== 创建并监听配置变化 ===");
    let service = source.create_with_watch::<DatabaseService, DatabaseConfig>("database")?;

    // 读取当前配置
    {
        let guard = service.read().unwrap();
        println!("当前连接信息: {}", guard.connection_info());
    }

    println!("\n提示: 修改 examples/configs/cfg/database.json5 文件查看热更新效果");
    println!("按 Ctrl+C 退出");

    // 保持程序运行以观察配置变化
    std::thread::park();

    Ok(())
}
