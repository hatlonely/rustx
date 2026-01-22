# cfg 模块 - 零耦合配置管理

提供统一的配置管理系统，支持多种配置来源和类型注册。

## 快速开始

### 1. 基本使用 - 文件配置源

```rust
use rustx::cfg::{ConfigSource, FileSource, FileSourceConfig, ConfigValue};
use serde::Deserialize;
use smart_default::SmartDefault;

#[derive(Deserialize, SmartDefault)]
#[serde(default)]
struct DatabaseConfig {
    #[default = "localhost"]
    host: String,
    #[default = 3306]
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

### 2. 自动创建与热更新

```rust
use rustx::cfg::{ConfigReloader, Configurable, FileSource, FileSourceConfig};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
struct DatabaseConfig {
    host: String,
    port: u16,
}

struct DatabaseService {
    config: DatabaseConfig,
}

impl DatabaseService {
    fn connection_info(&self) -> String {
        format!("{}:{}", self.config.host, self.config.port)
    }
}

// 从配置创建
impl From<DatabaseConfig> for DatabaseService {
    fn from(config: DatabaseConfig) -> Self {
        Self { config }
    }
}

// 实现热更新
impl ConfigReloader<DatabaseConfig> for DatabaseService {
    fn reload_config(&mut self, config: DatabaseConfig) -> anyhow::Result<()> {
        self.config = config;
        Ok(())
    }
}

let source = FileSource::new(FileSourceConfig {
    base_path: "config".to_string(),
});

// 一次性创建
let service: DatabaseService = source.create::<DatabaseService, DatabaseConfig>("database")?;

// 创建并自动监听配置变化
let service = source.create_with_watch::<DatabaseService, DatabaseConfig>("database")?;
// 修改 config/database.json 后自动调用 reload_config
```

**Trait Object 热更新**：通过配置的 `type` 字段动态选择实现

```rust
use rustx::cfg::{ConfigReloader, Configurable, FileSource, FileSourceConfig, register_trait};
use serde::Deserialize;

// 定义 trait
trait Cache: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
}

// Redis 实现
#[derive(Deserialize, Clone)]
struct RedisConfig { host: String, port: u16 }

struct RedisCache { config: RedisConfig }

impl From<RedisConfig> for RedisCache { fn from(config: RedisConfig) -> Self { Self { config } } }
impl ConfigReloader<RedisConfig> for RedisCache {
    fn reload_config(&mut self, config: RedisConfig) -> anyhow::Result<()> { self.config = config; Ok(()) }
}
impl Cache for RedisCache { fn get(&self, key: &str) -> Option<String> { None } }
impl From<Box<RedisCache>> for Box<dyn Cache> { fn from(c: Box<RedisCache>) -> Self { c as _ } }

// 注册实现
register_trait::<RedisCache, dyn Cache, RedisConfig>("redis")?;

// 配置文件 cache.json: { "type": "redis", "options": {...} }
let cache: Arc<RwLock<Box<dyn Cache>>> = source.create_trait_with_watch::<dyn Cache, RedisConfig>("cache")?;
// 修改 type 字段可切换不同实现
```

### 3. 监听配置变化

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

### 4. Apollo 配置中心

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

### 5. Trait 注册

```rust
use rustx::cfg::{register_trait, TypeOptions, create_trait_from_type_options};
use serde::Deserialize;
use smart_default::SmartDefault;

// 定义 trait
trait Cache: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
}

// 配置结构体
#[derive(Deserialize, SmartDefault)]
#[serde(default)]
struct RedisCacheConfig {
    #[default = "localhost"]
    host: String,
    #[default = 6379]
    port: u16,
    #[default = 0]
    db: u8,
}

// 实现类
struct RedisCache {
    client: String,
}

impl RedisCache {
    pub fn new(config: RedisCacheConfig) -> Self {
        Self {
            client: format!("redis://{}:{}/{}", config.host, config.port, config.db),
        }
    }
}

impl Cache for RedisCache {
    fn get(&self, key: &str) -> Option<String> {
        // Redis 实现
        None
    }
    
    fn set(&self, key: &str, value: &str) {
        // Redis 实现
    }
}

// 实现必要的 trait
impl From<RedisCacheConfig> for RedisCache {
    fn from(config: RedisCacheConfig) -> Self {
        RedisCache::new(config)
    }
}

impl From<Box<RedisCache>> for Box<dyn Cache> {
    fn from(cache: Box<RedisCache>) -> Self {
        cache as Box<dyn Cache>
    }
}

// 注册 trait 实现
register_trait::<RedisCache, dyn Cache, RedisCacheConfig>("RedisCache")?;

// 创建 trait 对象
let cache: Box<dyn Cache> = create_trait_from_type_options(&TypeOptions {
    type_name: "RedisCache".to_string(),
    options: serde_json::json!({
        "host": "localhost",
        "port": 6379,
        "db": 0
    }),
})?;
```

### 最佳实践 - Config 类设计模式

为了保持代码的一致性和可维护性，建议采用以下设计模式：

**核心原则：**
- 每个类放到单独的文件中，并且文件名以类名的小写下划线格式命名
- 每个类只提供一个 `new` 方法，参数为对应的 `Config` 结构体
- **Config 命名规范**：严格遵循"原类名 + Config"后缀，如 `Database` -> `DatabaseConfig`、`RedisCache` -> `RedisCacheConfig`
- **注册名称规范**：注册的类型名称必须严格与类名保持一致，如 `register_trait::<RedisCache, dyn Cache, RedisCacheConfig>("RedisCache")`
- 配置结构体使用 `serde::Deserialize` 进行自动反序列化
- **使用 SmartDefault 设置默认值**：配置结构体应使用 `#[derive(SmartDefault)]` 并配合 `#[serde(default)]`，为常用配置项设置合理的默认值
  - 字符串字段：`#[default = "value"]`
  - 数值字段：`#[default = value]`
  - 使用 `Default::default()` 或结构体更新语法 `..Default::default()` 构建部分配置
- 通过 `register_trait` 注册到类型系统，使用 `create_trait_from_type_options` 进行动态创建
- **构造方法返回值**：`new` 方法可以返回 `Self` 或 `Result<Self, Error>`，根据是否需要错误处理选择
  - 简单场景：`pub fn new(config: XxxConfig) -> Self`
  - 可能失败的场景：`pub fn new(config: XxxConfig) -> Result<Self, Error>`
  - 从 Config 转换时使用 `impl_from!` 宏，失败场景使用 `expect` 模式

**优势：**
- **统一接口**：所有类型都通过相同的模式创建和配置
- **类型安全**：编译期检查配置参数的正确性
- **依赖注入**：支持通过配置文件或外部系统动态创建对象
- **工厂模式**：基于类型名称和配置的动态实例创建
- **零耦合**：配置与业务逻辑完全分离

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