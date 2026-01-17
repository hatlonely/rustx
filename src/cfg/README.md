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
use rustx::cfg::{register, TypeOptions, create_from_type_options};
use serde::Deserialize;

// 1. 定义配置结构体
#[derive(Deserialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
    username: String,
    password: String,
}

// 2. 定义业务类，并提供唯一的 new 方法接受 Config
struct Database {
    host: String,
    port: u16,
    connection: String,
}

impl Database {
    // 唯一的构造方法，使用 Config 结构创建
    // 如果构造过程可能失败，可以返回 Result<Self, Error>
    pub fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        if config.port == 0 {
            return Err(DatabaseError::InvalidPort);
        }
        Ok(Self {
            host: config.host.clone(),
            port: config.port,
            connection: format!("{}:{}", config.host, config.port),
        })
    }
}

// 实现 From trait，这是注册系统要求的
// 如果 new 返回 Result，使用 expect 处理错误
impl From<DatabaseConfig> for Database {
    fn from(config: DatabaseConfig) -> Self {
        Database::new(config).expect("Failed to create Database")
    }
}

// 3. 注册到类型系统
register::<Database, DatabaseConfig>("Database")?;

// 4. 通过类型选项创建对象
let type_options = TypeOptions {
    type_name: "Database".to_string(),
    options: serde_json::json!({
        "host": "localhost",
        "port": 3306,
        "username": "admin",
        "password": "secret"
    }),
};

let db = create_from_type_options(&type_options)?
    .downcast::<Database>()
    .unwrap();
```

### 5. Trait 注册

```rust
use rustx::cfg::{register_trait, TypeOptions, create_trait_from_type_options};
use serde::Deserialize;

// 定义 trait
trait Cache: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
}

// 配置结构体
#[derive(Deserialize)]
struct RedisCacheConfig {
    host: String,
    port: u16,
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
- **注册名称规范**：注册的类型名称必须严格与类名保持一致，如 `register::<Database, DatabaseConfig>("Database")`
- 配置结构体使用 `serde::Deserialize` 进行自动反序列化
- 通过 `register`/`register_trait` 注册到类型系统
- 使用 `create_from_type_options`/`create_trait_from_type_options` 进行动态创建
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