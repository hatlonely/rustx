//! Trait Object 热更新示例
//!
//! 展示如何使用 create_trait_with_watch 创建支持热更新的 trait object

use rustx::cfg::{ConfigReloader, Configurable, FileSource, FileSourceConfig, register_trait};
use serde::Deserialize;
use std::sync::{Arc, RwLock};

/// 缓存 trait
trait Cache: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
    fn name(&self) -> &str;
}

/// Redis 缓存配置
#[derive(Debug, Deserialize, Clone)]
struct RedisCacheConfig {
    host: String,
    port: u16,
}

/// Redis 缓存实现
struct RedisCache {
    config: RedisCacheConfig,
}

impl From<RedisCacheConfig> for RedisCache {
    fn from(config: RedisCacheConfig) -> Self {
        println!("初始化 Redis 缓存: {}", config.host);
        Self { config }
    }
}

impl ConfigReloader<RedisCacheConfig> for RedisCache {
    fn reload_config(&mut self, config: RedisCacheConfig) -> anyhow::Result<()> {
        println!("更新 Redis 配置: {}:{} -> {}:{}",
            self.config.host, self.config.port, config.host, config.port);
        self.config = config;
        Ok(())
    }
}

impl Cache for RedisCache {
    fn get(&self, key: &str) -> Option<String> {
        println!("{} Redis GET: {}", self.name(), key);
        Some(format!("value_from_{}", self.config.host))
    }

    fn set(&self, key: &str, value: &str) {
        println!("{} Redis SET: {} = {}", self.name(), key, value);
    }

    fn name(&self) -> &str {
        "Redis"
    }
}

impl From<Box<RedisCache>> for Box<dyn Cache> {
    fn from(cache: Box<RedisCache>) -> Self {
        cache as Box<dyn Cache>
    }
}

/// 内存缓存配置
#[derive(Debug, Deserialize, Clone)]
struct MemoryCacheConfig {
    max_size: usize,
}

/// 内存缓存实现
struct MemoryCache {
    config: MemoryCacheConfig,
}

impl From<MemoryCacheConfig> for MemoryCache {
    fn from(config: MemoryCacheConfig) -> Self {
        println!("初始化内存缓存: max_size={}", config.max_size);
        Self { config }
    }
}

impl ConfigReloader<MemoryCacheConfig> for MemoryCache {
    fn reload_config(&mut self, config: MemoryCacheConfig) -> anyhow::Result<()> {
        println!("更新内存缓存配置: max_size: {} -> {}",
            self.config.max_size, config.max_size);
        self.config = config;
        Ok(())
    }
}

impl Cache for MemoryCache {
    fn get(&self, key: &str) -> Option<String> {
        println!("{} Memory GET: {}", self.name(), key);
        Some(format!("value_from_memory_{}", self.config.max_size))
    }

    fn set(&self, key: &str, value: &str) {
        println!("{} Memory SET: {} = {}", self.name(), key, value);
    }

    fn name(&self) -> &str {
        "Memory"
    }
}

impl From<Box<MemoryCache>> for Box<dyn Cache> {
    fn from(cache: Box<MemoryCache>) -> Self {
        cache as Box<dyn Cache>
    }
}

fn main() -> anyhow::Result<()> {
    // 1. 注册到 trait registry
    register_trait::<RedisCache, dyn Cache, RedisCacheConfig>("redis")?;
    register_trait::<MemoryCache, dyn Cache, MemoryCacheConfig>("memory")?;

    // 2. 创建文件配置源
    let source = FileSource::new(FileSourceConfig {
        base_path: "examples/cfg/configs".to_string(),
    });

    // 3. 从配置创建并自动监听 trait object
    println!("=== 创建 Trait Object 并监听配置变化 ===");
    let cache: Arc<RwLock<Box<dyn Cache>>> =
        source.create_trait_with_watch::<dyn Cache, RedisCacheConfig>("cache")?;

    // 4. 使用缓存
    {
        let guard = cache.read().unwrap();
        guard.get("user:123");
        guard.set("user:123", "alice");
    }

    println!("\n提示: 修改 examples/cfg/configs/cache.json 文件中的 type 字段切换缓存实现");
    println!("  - 修改为 \"type\": \"redis\" 使用 Redis 缓存");
    println!("  - 修改为 \"type\": \"memory\" 使用内存缓存");
    println!("按 Ctrl+C 退出");

    // 保持程序运行以观察配置变化
    std::thread::park();

    Ok(())
}
