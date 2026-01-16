# cfg 模块 - 零耦合配置管理

提供统一的配置管理系统，支持多种配置来源和类型注册。

## 快速开始

### 1. 基本使用 - 文件配置源

```rust
use rustx::cfg::{ConfigSource, FileSource, FileSourceConfig, ConfigValue};
use serde::Deserialize;

#[derive(Deserialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
}

// 创建文件配置源
let source = FileSource::new(FileSourceConfig {
    base_path: "config".to_string(),
});

// 加载配置文件 config/database.json
let config: DatabaseConfig = source.load("database")?.into_type()?;
println!("数据库: {}:{}", config.host, config.port);
```

### 2. 监听配置变化

```rust
use rustx::cfg::{ConfigChange, ConfigSource, FileSource, FileSourceConfig};

let source = FileSource::new(FileSourceConfig {
    base_path: "config".to_string(),
});

source.watch("database", Box::new(|change| {
    match change {
        ConfigChange::Updated(value) => {
            println!("配置已更新: {:?}", value.as_value());
        }
        ConfigChange::Deleted => println!("配置已删除"),
        ConfigChange::Error(msg) => eprintln!("错误: {}", msg),
    }
}))?;
```

### 3. Apollo 配置中心

```rust
use rustx::cfg::{ApolloSource, ApolloSourceConfig, ConfigSource};

let source = ApolloSource::new(ApolloSourceConfig {
    server_url: "http://localhost:8080".to_string(),
    app_id: "my-app".to_string(),
    namespace: "application".to_string(),
    cluster: "default".to_string(),
})?;

let config: DatabaseConfig = source.load("database")?.into_type()?;
```

### 4. 类型注册系统

```rust
use rustx::cfg::{register_auto, TypeOptions, create_from_type_options};
use serde::Deserialize;

#[derive(Deserialize)]
struct RedisConfig {
    host: String,
    port: u16,
}

struct RedisClient {
    host: String,
    port: u16,
}

impl From<RedisConfig> for RedisClient {
    fn from(config: RedisConfig) -> Self {
        Self { host: config.host, port: config.port }
    }
}

// 自动注册类型
register_auto::<RedisClient, RedisConfig>()?;

// 从配置创建实例
let type_options = TypeOptions {
    type_name: "RedisClient".to_string(),
    options: serde_json::json!({
        "host": "localhost",
        "port": 6379
    }),
};

let client = create_from_type_options(&type_options)?
    .downcast::<RedisClient>()
    .unwrap();
```

### 5. Trait 注册

```rust
use rustx::cfg::{register_trait, TypeOptions, create_trait_from_type_options};

trait Storage: Send + Sync {
    fn save(&self, data: &str);
}

struct FileStorage;
struct RedisStorage;

impl Storage for FileStorage {
    fn save(&self, data: &str) { /* 保存到文件 */ }
}

impl Storage for RedisStorage {
    fn save(&self, data: &str) { /* 保存到 Redis */ }
}

// 注册 trait 实现
register_trait::<FileStorage, dyn Storage, serde_json::Value>("file")?;
register_trait::<RedisStorage, dyn Storage, serde_json::Value>("redis")?;

// 动态创建实例
let storage: Box<dyn Storage> = create_trait_from_type_options(&TypeOptions {
    type_name: "redis".to_string(),
    options: serde_json::json!({}),
})?;
```

## 支持格式

- **JSON**: `.json`
- **YAML**: `.yaml`, `.yml`  
- **TOML**: `.toml`

## 核心特性

- **零耦合**: 配置源和业务逻辑完全解耦
- **统一接口**: 所有配置源实现相同的 `ConfigSource` trait
- **热重载**: 支持配置文件和 Apollo 配置的实时监听
- **类型安全**: 强类型配置解析，编译期检查
- **自动发现**: 支持多种文件格式的自动检测
- **工厂模式**: 基于类型名称和配置的动态实例创建