# CFG - 零耦合配置管理库

一个现代化的 Rust 配置管理库，提供零耦合的类型注册机制，支持多种配置格式和动态对象创建。

## ✨ 特性

- 🚀 **零耦合设计** - 业务类型无需知道配置系统存在
- 📝 **多格式支持** - JSON、YAML、TOML配置文件解析
- ⏱️ **Duration人性化格式** - 支持`30s`、`1m`、`1h30m`等格式
- 🔧 **简单易用** - 最小化的接口，最大化的功能
- 🔒 **线程安全** - 全局类型注册表支持并发访问
- ⚡ **零成本抽象** - 编译时优化的性能
- 🎯 **自动类型名** - 直接使用 Rust 原生类型名作为标识

## 🚀 快速开始

### 添加依赖

```toml
[dependencies]
rustx = { path = "path/to/rustx" }
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"
```

### 零耦合示例

```rust
use rustx::cfg::*;
use rustx::cfg::duration::{serde_as, HumanDur};
use serde::{Deserialize, Serialize};
use anyhow::Result;
use std::time::Duration;

// 1. 定义配置结构
#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ServiceConfig {
    name: String,
    host: String,
    port: u16,
    #[serde_as(as = "HumanDur")]
    timeout: Duration,
    max_connections: Option<u32>,
}

// 2. 定义服务类型（完全不需要知道配置系统）
#[derive(Debug)]
struct Service {
    config: ServiceConfig,
}

impl Service {
    fn new(config: ServiceConfig) -> Self {
        println!("创建服务: {} @ {}:{}", 
                config.name, config.host, config.port);
        Self { config }
    }
}

// 3. 实现零耦合配置接口（唯一需要的！）
impl WithConfig<ServiceConfig> for Service {
    fn with_config(config: ServiceConfig) -> Self {
        Service::new(config)
    }
}

// 4. 使用配置
#[tokio::main]
async fn main() -> Result<()> {
    // 零耦合注册 - 自动生成类型名
    register_auto_with_type::<Service, ServiceConfig>()?;
    
    // 获取实际的类型名（用于配置文件）
    let type_name = std::any::type_name::<Service>();
    
    // 从JSON配置创建服务
    let json_config = format!(r#"
    {{
        "type": "{}",
        "options": {{
            "name": "web-api",
            "host": "localhost", 
            "port": 8080,
            "timeout": "30s",
            "max_connections": 100
        }}
    }}"#, type_name);
    
    let type_options = TypeOptions::from_json(&json_config)?;
    let service_obj = create_from_type_options(&type_options)?;
    
    // 类型转换
    if let Some(service) = service_obj.downcast_ref::<Service>() {
        println!("✅ 服务创建成功");
    }
    
    Ok(())
}
```

## 🏗️ 核心概念

### 1. WithConfig Trait

零耦合的配置接口，这是唯一需要实现的：

```rust
pub trait WithConfig<Config> {
    fn with_config(config: Config) -> Self;
}
```

### 2. TypeOptions 结构

配置的通用格式：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeOptions {
    #[serde(rename = "type")]
    pub type_name: String,
    pub options: JsonValue,
}
```

### 3. 零耦合注册

两种注册方式：

```rust
// 自动生成类型名
register_auto_with_type::<MyService, MyConfig>()?;

// 手动指定类型名
register_auto::<MyService, MyConfig>("custom_name")?;
```

## 📝 支持的配置格式

### JSON

```rust
let json_config = r#"
{
    "type": "my_crate::MyService",
    "options": {
        "name": "web-api",
        "timeout": "30s"
    }
}"#;

let type_options = TypeOptions::from_json(json_config)?;
```

### YAML

```rust
let yaml_config = r#"
type: "my_crate::MyService"
options:
  name: "web-api"
  timeout: "30s"
"#;

let type_options = TypeOptions::from_yaml(yaml_config)?;
```

### TOML

```rust
let toml_config = r#"
type = "my_crate::MyService"

[options]
name = "web-api"
timeout = "30s"
"#;

let type_options = TypeOptions::from_toml(toml_config)?;
```

## ⏱️ Duration 人性化格式

cfg库内置支持Duration的人性化格式：

```rust
use rustx::cfg::duration::{serde_as, HumanDur};

#[serde_as]
#[derive(Deserialize)]
struct Config {
    #[serde_as(as = "HumanDur")]
    timeout: Duration,
    #[serde_as(as = "HumanDur")]
    retry_interval: Duration,
}
```

支持的格式：
- `3s` - 3秒
- `100ms` - 100毫秒
- `2m` - 2分钟
- `1h` - 1小时
- `1h30m45s` - 1小时30分钟45秒
- `2d` - 2天

## 🔧 实际使用案例

### MapStore 示例

```rust
use rustx::kv::store::{MapStore, MapStoreConfig};
use rustx::cfg::*;

// MapStore 完全不知道配置系统的存在
// 只需要实现 WithConfig trait
impl<K, V> WithConfig<MapStoreConfig> for MapStore<K, V> 
where 
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn with_config(config: MapStoreConfig) -> Self {
        MapStore::with_config(config)  // 复用已有方法
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 注册不同类型的 MapStore
    register_auto_with_type::<MapStore<String, String>, MapStoreConfig>()?;
    register_auto_with_type::<MapStore<String, i32>, MapStoreConfig>()?;
    
    let config = r#"
    {
        "type": "rustx::kv::store::memory::MapStore<alloc::string::String, alloc::string::String>",
        "options": {
            "initial_capacity": 1000,
            "enable_stats": true
        }
    }"#;
    
    let type_options = TypeOptions::from_json(config)?;
    let store_obj = create_from_type_options(&type_options)?;
    
    if let Some(store) = store_obj.downcast_ref::<MapStore<String, String>>() {
        store.set("key".to_string(), "value".to_string(), Default::default()).await?;
        let value = store.get("key".to_string()).await?;
        println!("Value: {}", value);
    }
    
    Ok(())
}
```

## 📚 API 参考

### 核心函数

- `register_auto_with_type::<T, Config>()` - 自动注册类型（推荐）
- `register_auto::<T, Config>(type_name)` - 指定类型名注册
- `create_from_type_options(type_options)` - 从配置创建对象

### TypeOptions 方法

- `TypeOptions::from_json(json_str)` - 从JSON字符串解析
- `TypeOptions::from_yaml(yaml_str)` - 从YAML字符串解析
- `TypeOptions::from_toml(toml_str)` - 从TOML字符串解析
- `type_options.to_json()` - 转换为JSON字符串
- `type_options.to_yaml()` - 转换为YAML字符串
- `type_options.to_toml()` - 转换为TOML字符串

### Duration 工具函数

- `parse_duration(s)` - 解析时间字符串
- `format_duration(duration)` - 格式化Duration为字符串

## 🎯 设计原则

1. **零耦合** - 业务代码不依赖配置系统
2. **最小接口** - 只需实现 `WithConfig` trait
3. **自动化** - 自动生成类型名，减少手工配置
4. **类型安全** - 编译时类型检查
5. **性能优先** - 零成本抽象

## 🤝 与其他库的对比

| 特性 | CFG | config-rs | figment |
|-----|-----|-----------|---------|
| 零耦合 | ✅ | ❌ | ❌ |
| 类型注册 | ✅ | ❌ | ❌ |
| 动态创建 | ✅ | ❌ | ❌ |
| 多格式 | ✅ | ✅ | ✅ |
| Duration格式 | ✅ | ❌ | ❌ |

## 📄 许可证

根据项目的许可证条款分发。

## 🤝 贡献

欢迎提交Issues和Pull Requests！

---

更多示例请参考 `examples/` 目录。