# kv::loader - 通用数据加载器

提供从文件加载 KV 数据并监听变化的加载器。

## 核心概念

### Loader - 加载器

负责从数据源加载数据并监听变化，当数据发生变化时通知监听器。

### KvStream - 数据流

表示一批 KV 数据的集合，提供遍历接口。

### Listener - 监听器

处理数据变更的回调函数，接收 `KvStream` 并处理其中的数据。

## 快速开始

```rust
use rustx::kv::loader::{register_loaders, Loader, Listener};
use rustx::kv::parser::register_parsers;
use rustx::cfg::{TypeOptions, create_trait_from_type_options};

// 1. 注册 Parser 和 Loader 类型
register_parsers::<String, String>()?;
register_loaders::<String, String>()?;

// 2. 通过配置创建 Loader
let opts = TypeOptions::from_json(r#"{
    "type": "KvFileLoader",
    "options": {
        "file_path": "/tmp/data.txt",
        "parser": {
            "type": "LineParser",
            "options": {
                "separator": "\t"
            }
        },
        "skip_dirty_rows": false
    }
}"#)?;

let mut loader: Box<dyn Loader<String, String>> = create_trait_from_type_options(&opts)?;

// 3. 注册监听器
let listener: Listener<String, String> = Arc::new(move |stream| {
    stream.each(&|change_type, key, value| {
        println!("change_type: {}, key: {}, value: {}", change_type, key, value);
        Ok(())
    })
});

loader.on_change(listener)?;

// 4. 使用完毕后关闭
// loader.close()?;
```

## Loader 配置选项

### KvFileLoader - KV 文件加载器

从文件中逐行读取并解析 KV 数据，支持文件变化监听和自动重新加载。

```json5
{
    // Loader 类型，固定为 "KvFileLoader"
    "type": "KvFileLoader",
    "options": {
        // 文件路径（必需）
        "file_path": "/tmp/data.txt",

        // Parser 配置（必需）
        "parser": {
            "type": "LineParser",
            "options": {
                "separator": "\t"
            }
        },

        // 是否跳过脏数据（可选，默认 false）
        // true: 遇到解析错误的行时记录日志并跳过
        // false: 遇到解析错误时立即返回错误
        "skip_dirty_rows": false,

        // Scanner buffer 最小大小（可选，默认 65536）
        "scanner_buffer_min_size": 65536,

        // Scanner buffer 最大大小（可选，默认 4194304）
        "scanner_buffer_max_size": 4194304
    }
}
```

**工作流程**：
1. 启动时立即加载文件内容并触发监听器
2. 监听文件变化（创建、修改、删除）
3. 文件变化时重新加载所有内容并触发监听器
4. 支持通过 `close()` 方法停止监听

### FileTrigger - 文件触发器

监听文件变化并触发通知，但不读取文件内容。适用于需要自己控制数据加载逻辑的场景。

```json5
{
    // Loader 类型，固定为 "FileTrigger"
    "type": "FileTrigger",
    "options": {
        // 文件路径（必需）
        "file_path": "/tmp/data.txt"
    }
}
```

**工作流程**：
1. 启动时立即触发一次通知（传递空数据流）
2. 监听文件变化
3. 文件变化时触发通知（传递空数据流）
4. 监听器收到通知后自行决定如何加载数据

**使用场景**：
- 需要使用自定义的数据加载逻辑
- 需要从数据库或其他数据源加载数据
- 文件只是触发信号，实际数据存储在其他地方

## KvStream 类型

### KvFileStream - KV 文件数据流

由 `KvFileLoader` 使用，从文件中逐行读取并解析 KV 数据。

### EmptyKvStream - 空 KV 数据流

由 `FileTrigger` 使用，不包含任何数据，仅用于触发通知。

## LoadStrategy 说明

加载策略常量（预留，当前版本未使用）：

| 常量 | 值 | 说明 |
|------|-----|------|
| `LOAD_STRATEGY_REPLACE` | "replace" | 替换模式：清空旧数据后加载新数据 |
| `LOAD_STRATEGY_INPLACE` | "inplace" | 原地更新模式：直接更新到现有数据上 |

## ChangeType 说明

数据变更类型（由 Parser 解析得出）：

| 值 | 常量 | 说明 |
|------|------|------|
| 0 | `Unknown` | 未知 |
| 1 | `Add` | 新增 |
| 2 | `Update` | 更新 |
| 3 | `Delete` | 删除 |

## 完整示例

### 使用 KvFileLoader 自动加载文件数据

```rust
use rustx::kv::loader::{register_loaders, Loader};
use rustx::kv::parser::register_parsers;
use rustx::cfg::{TypeOptions, create_trait_from_type_options};
use std::sync::{Arc, Mutex};

// 注册类型
register_parsers::<String, String>()?;
register_loaders::<String, String>()?;

// 创建加载器配置
let opts = TypeOptions::from_json(r#"{
    "type": "KvFileLoader",
    "options": {
        "file_path": "/tmp/users.txt",
        "parser": {
            "type": "LineParser",
            "options": {
                "separator": "\t"
            }
        }
    }
}"#)?;

let mut loader: Box<dyn Loader<String, String>> = create_trait_from_type_options(&opts)?;

// 使用 Mutex 在监听器中共享状态
let data_store = Arc::new(Mutex::new(Vec::new()));
let store_clone = data_store.clone();

// 注册监听器
let listener = Arc::new(move |stream| {
    stream.each(&|_change_type, key, value| {
        let mut store = store_clone.lock().unwrap();
        store.push((key, value));
        println!("loaded: {} => {}", key, value);
        Ok(())
    })
});

loader.on_change(listener)?;

// 当文件 /tmp/users.txt 变化时，会自动重新加载
// ...
```

### 使用 FileTrigger 自定义加载逻辑

```rust
use rustx::kv::loader::{register_loaders, Loader};
use rustx::cfg::{TypeOptions, create_trait_from_type_options};

register_loaders::<String, String>()?;

let opts = TypeOptions::from_json(r#"{
    "type": "FileTrigger",
    "options": {
        "file_path": "/tmp/signal.txt"
    }
}"#)?;

let mut trigger: Box<dyn Loader<String, String>> = create_trait_from_type_options(&opts)?;

let listener = Arc::new(move |_stream| {
    // 文件变化时，从数据库加载最新数据
    println!("File changed, reloading data from database...");
    // 自定义加载逻辑
    Ok(())
});

trigger.on_change(listener)?;
```
