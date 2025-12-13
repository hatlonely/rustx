# CFG - 配置管理库

一个基于类型注册机制的Rust配置管理库，支持多种配置格式和动态对象创建。

## 特性

- 🚀 **基于类型的配置反序列化** - 通过类型名称动态创建对象
- 📝 **多格式支持** - JSON、YAML、TOML配置文件解析
- ⏱️ **Duration人性化格式** - 支持`30s`、`1m`、`1h30m`等格式
- 🔧 **简单易用** - 最少代码实现配置管理
- 🔒 **线程安全** - 全局类型注册表支持并发访问
- ⚡ **零成本抽象** - 编译时优化的性能

## 快速开始

### 添加依赖

```toml
[dependencies]
cfg = { path = "path/to/cfg" }
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"
```

### 基本用法

```rust
use cfg::*;
use cfg::duration::{serde_as, HumanDur};
use serde::{Deserialize, Serialize};
use anyhow::Result;
use std::any::Any;
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

// 2. 定义服务类型
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

// 3. 实现Configurable trait
impl Configurable for Service {
    type Config = ServiceConfig;
    
    fn from_config(config: Self::Config) -> Result<Box<dyn Any + Send + Sync>> {
        Ok(Box::new(Service::new(config)))
    }
    
    fn type_name() -> &'static str {
        "service"
    }
}

// 4. 使用配置
fn main() -> Result<()> {
    // 注册类型
    register::<Service>()?;
    
    // 从JSON配置创建服务
    let json_config = r#"
    {
        "type": "service",
        "options": {
            "name": "web-api",
            "host": "localhost", 
            "port": 8080,
            "timeout": "30s",
            "max_connections": 100
        }
    }"#;
    
    let type_options = TypeOptions::from_json(json_config)?;
    let service_obj = create_from_type_options(&type_options)?;
    
    // 类型转换
    if let Some(service) = service_obj.downcast_ref::<Service>() {
        println!("✅ 服务创建成功");
    }
    
    Ok(())
}
```

## 核心概念

### 1. Configurable Trait

所有可配置的类型都需要实现`Configurable` trait：

```rust
pub trait Configurable: Send + Sync + 'static {
    type Config: DeserializeOwned + Clone;
    
    fn from_config(config: Self::Config) -> Result<Box<dyn Any + Send + Sync>>;
    fn type_name() -> &'static str;
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

### 3. 类型注册

在使用前需要注册类型：

```rust
register::<MyService>()?;

// 或者手动注册
register_type("my_service", |config: MyConfig| {
    Ok(Box::new(MyService::new(config)))
})?;
```

## 支持的配置格式

### JSON

```rust
let json_config = r#"
{
    "type": "service",
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
type: service
options:
  name: "web-api"
  timeout: "30s"
"#;

let type_options = TypeOptions::from_yaml(yaml_config)?;
```

### TOML

```rust
let toml_config = r#"
type = "service"

[options]
name = "web-api"
timeout = "30s"
"#;

let type_options = TypeOptions::from_toml(toml_config)?;
```

## Duration 人性化格式

cfg库内置支持Duration的人性化格式：

```rust
use cfg::duration::{serde_as, HumanDur};

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

## API 参考

### 核心函数

- `register<T: Configurable>()` - 注册类型
- `register_type<C>(type_name, constructor)` - 手动注册类型
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

## 许可证

根据项目的许可证条款分发。

## 贡献

欢迎提交Issues和Pull Requests！