# kv::store - 通用 KV 存储

提供内存和分布式 KV 存储实现。

## 快速开始

```rust
use rustx::kv::store::{register_hash_stores, Store, SetOptions};
use rustx::cfg::{TypeOptions, create_trait_from_type_options};

// 1. 注册 Store 类型
register_hash_stores::<String, String>()?;

// 2. 通过配置创建 Store
let opts = TypeOptions::from_json(r#"{
    "type": "RwLockHashMapStore",
    "options": {
        "initial_capacity": 1000
    }
}"#)?;

let store: Box<dyn Store<String, String>> = create_trait_from_type_options(&opts)?;

// 3. 使用 Store
store.set(&"key".to_string(), &"value".to_string(), &SetOptions::new()).await?;
let value = store.get(&"key".to_string()).await?;
```

## Store 配置选项

### UnsafeHashMapStore - 内存存储（非线程安全）

基于 `UnsafeCell<HashMap>` 实现，**仅适用于写一次读多次的场景**，不支持并发写入。

```json5
{
    "type": "UnsafeHashMapStore",
    "options": {
        // 初始容量（可选，默认无）
        "initial_capacity": 1000
    }
}
```

### RwLockHashMapStore - 内存存储（线程安全）

基于 `RwLock<HashMap>` 实现，支持并发读写。

```json5
{
    "type": "RwLockHashMapStore",
    "options": {
        // 初始容量（可选，默认无）
        "initial_capacity": 1000
    }
}
```

### DashMapStore - 内存存储（线程安全，高并发优化）

基于 `DashMap` 实现，使用分片锁技术，高并发读写场景性能优于 RwLockHashMapStore。

```json5
{
    "type": "DashMapStore",
    "options": {
        // 初始容量（可选，默认无）
        "initial_capacity": 1000
    }
}
```

### LoadableSyncStore - 可加载数据的同步存储装饰器

通过 Loader 从外部数据源（文件等）加载数据到内存 Store。支持两种加载策略：
- **inplace**：增量更新，直接在当前 store 上执行 set/del
- **replace**：全量替换，创建新 store 加载完数据后原子替换旧 store（使用 `arc-swap` 实现无锁切换）

**使用前需注册**：`register_parsers`、`register_loaders`、`register_sync_stores`。

```json5
{
    "type": "LoadableSyncStore",
    "options": {
        // 底层 SyncStore 配置（支持所有内存 Store 类型）
        "store": {
            "type": "RwLockHashMapStore",
            "options": {
                "initial_capacity": 1000
            }
        },
        // Loader 配置（数据来源）
        "loader": {
            "type": "KvFileLoader",
            "options": {
                "file_path": "/path/to/data.txt",
                "parser": {
                    "type": "LineParser",
                    "options": { "separator": "\t" }
                }
            }
        },
        // 加载策略: "inplace"（增量，默认）或 "replace"（全量替换）
        "load_strategy": "inplace"
    }
}
```

### RedisStore - Redis 分布式存储

基于 Redis 实现的分布式 KV 存储，支持 TTL 和批量操作。**使用前需先注册序列化器**。

```json5
{
    "type": "RedisStore",
    "options": {
        // 单机模式：Redis 地址
        "endpoint": "localhost:6379",

        // 集群模式：节点地址列表（二选一）
        // "endpoints": ["node1:6379", "node2:6379"],

        // 认证配置（可选）
        "username": "default",
        "password": "secret",

        // 数据库编号（可选，默认 0）
        "db": 0,

        // 默认 TTL（秒，可选，默认 0 即不过期）
        "default_ttl": 3600,

        // 超时配置（可选，默认 5 秒）
        "connection_timeout": 5,
        "command_timeout": 3,

        // 序列化器配置（可选，默认 JsonSerializer）
        "key_serializer": {
            "type": "JsonSerializer",
            "options": {}
        },
        "val_serializer": {
            "type": "JsonSerializer",
            "options": {}
        }
    }
}
```

## Store 接口

| 方法 | 说明 |
|------|------|
| `set(key, value, options)` | 设置键值对 |
| `get(key)` | 获取值 |
| `del(key)` | 删除键 |
| `batch_set(keys, values, options)` | 批量设置 |
| `batch_get(keys)` | 批量获取 |
| `batch_del(keys)` | 批量删除 |
| `close()` | 关闭存储 |

## SetOptions 配置

```rust
use std::time::Duration;
use rustx::kv::store::SetOptions;

// 基本配置
let opts = SetOptions::new();

// 设置过期时间（10 秒）
let opts = SetOptions::new().with_expiration(Duration::from_secs(10));

// 仅在键不存在时设置（类似 Redis SETNX）
let opts = SetOptions::new().with_if_not_exist();
```

## 注册函数说明

| 函数 | 支持的 Store | 前置条件 |
|------|-------------|---------|
| `register_hash_stores<K, V>()` | UnsafeHashMapStore, RwLockHashMapStore, DashMapStore, LoadableSyncStore | 无（LoadableSyncStore 使用时需先注册 Parser、Loader） |
| `register_sync_stores<K, V>()` | 同上（注册为 `dyn SyncStore`） | 同上 |
| `register_redis_stores<K, V>()` | RedisStore | 需先注册序列化器 |

## 使用示例

### RedisStore 完整示例

```rust
use rustx::kv::store::{register_redis_stores, Store, SetOptions};
use rustx::kv::serializer::register_serde_serializers;
use rustx::cfg::{TypeOptions, create_trait_from_type_options};

// 先注册序列化器
register_serde_serializers::<String>()?;

// 再注册 Store
register_redis_stores::<String, String>()?;

// 创建 RedisStore
let opts = TypeOptions::from_json(r#"{
    "type": "RedisStore",
    "options": {
        "endpoint": "localhost:6379",
        "password": "secret",
        "db": 0,
        "default_ttl": 3600
    }
}"#)?;

let store: Box<dyn Store<String, String>> = create_trait_from_type_options(&opts)?;

// 使用 Store
store.set(
    &"user:1".to_string(),
    &"Alice".to_string(),
    &SetOptions::new().with_expiration(Duration::from_secs(3600))
).await?;

let value = store.get(&"user:1".to_string()).await?;
```

### 批量操作示例

```rust
let keys = vec!["key1".to_string(), "key2".to_string()];
let values = vec!["value1".to_string(), "value2".to_string()];

// 批量设置
let results = store.batch_set(&keys, &values, &SetOptions::new()).await?;

// 批量获取
let (values, errors) = store.batch_get(&keys).await?;

// 批量删除
let results = store.batch_del(&keys).await?;
```
