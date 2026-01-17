# kv::loader 模块实现方案

> 高性能 KV 数据加载器 - 从文件加载数据并监听变化

## 1. 设计目标

### 1.1 核心需求
- 支持从文本文件加载 KV 数据（每行一条记录）
- 支持文件变化监听，自动重新加载数据
- **高性能**: 100+ 文件同时监听时，只使用 2 个线程（1 watcher + 1 dispatcher）
- **零耦合**: 严格遵循 `src/cfg/README.md` 的设计模式
- **类型安全**: 支持泛型 K,V，自动类型推导

### 1.2 性能指标

| 方案 | 线程数 (100文件) | 内存开销 | 上下文切换 |
|------|-----------------|---------|-----------|
| Go 版本 | 100 goroutine | ~200KB | 极低 |
| 优化前 (避免) | 200 线程 | ~1.6GB | 极高 |
| **目标方案** | **2 线程** | ~16MB | 极低 |

## 2. 架构设计

### 2.1 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                         应用层                                    │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │KvFileLoader 1│  │KvFileLoader 2│  │KvFileLoader N│          │
│  │/tmp/data1.txt│  │/tmp/data2.txt│  │/tmp/dataN.txt│          │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │
│         │ on_change        │ on_change        │ on_change      │
│         └─────────┬────────┴─────────┬────────┘                │
│                   │ 注册 listener     │                         │
│                   ▼                   ▼                         │
├─────────────────────────────────────────────────────────────────┤
│                         全局单例层                                │
│                                                                  │
│  ┌────────────────────────────┐    ┌──────────────────────┐    │
│  │   GlobalFileWatcher        │    │  EventDispatcher     │    │
│  │   (单例, 1 线程)            │───▶│  (单例, 1 线程)      │    │
│  │                            │    │                      │    │
│  │  - notify::Watcher         │    │  - HashMap<Path,     │    │
│  │  - HashMap<Path, Sender[]> │    │    Vec<Listener>>    │    │
│  └────────────────────────────┘    └──────────────────────┘    │
│         ▲                                   │                   │
│         │ 文件系统事件                      │ 调用 listener      │
│         │                                   ▼                   │
│  ┌──────┴───────────────────────────────────┴───────────┐      │
│  │           notify crate (fs event)                    │      │
│  └──────────────────────────────────────────────────────┘      │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 核心组件

#### 2.2.1 GlobalFileWatcher (全局单例)
**职责**: 监听文件系统事件
**线程数**: 1
**实现**:
```rust
pub struct GlobalFileWatcher {
    watcher: notify::RecommendedWatcher,
    subscriptions: Arc<Mutex<HashMap<PathBuf, Vec<Sender<FileEvent>>>>>,
}
```

**功能**:
- 使用 `notify` crate 监听文件系统
- 维护文件路径到订阅者的映射
- 分发文件事件给订阅者

#### 2.2.2 EventDispatcher (全局单例)
**职责**: 分发事件并调用 listener 回调
**线程数**: 1
**实现**:
```rust
pub struct EventDispatcher<K, V> {
    listeners: Arc<Mutex<HashMap<PathBuf, Vec<Listener<K, V>>>>>,
    event_rx: Receiver<(PathBuf, FileEvent)>,
}
```

**功能**:
- 接收来自 GlobalFileWatcher 的事件
- 查找对应文件的 listener 列表
- 调用每个 listener（同步或线程池）

#### 2.2.3 KvFileLoader
**职责**: 文件数据加载器
**线程数**: 0 (不创建线程)
**实现**:
```rust
pub struct KvFileLoader<K, V> {
    file_path: PathBuf,
    parser: Box<dyn Parser<K, V>>,
    listener_id: Option<usize>,
}
```

**功能**:
- 初始加载文件数据
- 注册 listener 到 EventDispatcher
- 关闭时取消注册

### 2.3 数据流

```
文件变化
  │
  ▼
GlobalFileWatcher 线程检测到事件
  │
  ▼ 通过 channel 发送
EventDispatcher 线程接收事件
  │
  ▼ 查找 HashMap<Path, Vec<Listener>>
找到对应的 listener
  │
  ▼ 创建 KvFileStream
调用 listener(&stream)
  │
  ▼
用户代码处理数据
```

## 3. 文件结构

```
src/kv/loader/
├── mod.rs                      # 模块导出
├── plan.md                     # 本文件：设计方案
├── core.rs                     # 核心 trait 和常量
├── global_watcher.rs           # 全局文件监听器 (单例)
├── event_dispatcher.rs         # 全局事件分发器 (单例)
├── kv_file_loader.rs           # KvFileLoader + Config
├── kv_file_stream.rs           # KvFileStream 实现
├── file_trigger.rs             # FileTrigger + Config
├── empty_kv_stream.rs          # EmptyKVStream 实现
└── loader_factory.rs           # 工厂函数
```

## 4. 核心类型定义

### 4.1 核心 Trait (core.rs)

```rust
/// KV 数据流：用于遍历 KV 数据
pub trait KvStream<K, V>: Send + Sync {
    fn each(&self, callback: &dyn Fn(ChangeType, K, V) -> Result<(), LoaderError>)
        -> Result<(), LoaderError>;
}

/// 监听器：处理 KV 数据变更的回调
pub type Listener<K, V> = Box<dyn Fn(&dyn KvStream<K, V>) -> Result<(), LoaderError> + Send + Sync>;

/// 核心加载器 trait
pub trait Loader<K, V>: Send + Sync {
    fn on_change(&mut self, listener: Listener<K, V>) -> Result<(), LoaderError>;
    fn close(&mut self) -> Result<(), LoaderError>;
}
```

### 4.2 配置结构体 (遵循 cfg/README.md 规范)

```rust
// kv_file_loader.rs
pub struct KvFileLoaderConfig {
    pub file_path: String,
    pub parser: TypeOptions,  // 用于创建 Parser
    pub skip_dirty_rows: bool,
    pub scanner_buffer_size: usize,
}

// file_trigger.rs
pub struct FileTriggerConfig {
    pub file_path: String,
}
```

## 5. 实现计划

### 5.1 Phase 1: 核心基础 (1-2天)

**任务**:
- [ ] 更新 `core.rs`：简化为同步 API
- [ ] 实现 `global_watcher.rs`：GlobalFileWatcher 单例
- [ ] 实现 `event_dispatcher.rs`：EventDispatcher 单例
- [ ] 单元测试：watcher 和 dispatcher 基本功能

**验收标准**:
- GlobalFileWatcher 可以监听文件变化
- EventDispatcher 可以注册和分发事件
- 只创建 1 个线程

### 5.2 Phase 2: KvFileStream (1天)

**任务**:
- [ ] 实现 `kv_file_stream.rs`：KvFileStream
- [ ] 支持可配置的 scanner buffer size
- [ ] 支持 skip_dirty_rows 选项
- [ ] 单元测试：文件读取和解析

**验收标准**:
- 可以正确读取和解析文件
- 支持跳过脏数据
- 错误处理完善

### 5.3 Phase 3: KvFileLoader (1-2天)

**任务**:
- [ ] 实现 `kv_file_loader.rs`：KvFileLoader 及 Config
- [ ] 实现 `new` 方法接受 KvFileLoaderConfig
- [ ] 实现 `Loader<K, V>` trait
- [ ] 实现 `From<KvFileLoaderConfig> for KvFileLoader`
- [ ] 集成测试：完整的加载和监听流程

**验收标准**:
- 初始加载正确
- 文件变化时自动重新加载
- 不创建线程（使用全局 dispatcher）
- 可正确关闭

### 5.4 Phase 4: FileTrigger (1天)

**任务**:
- [ ] 实现 `file_trigger.rs`：FileTrigger 及 Config
- [ ] 实现 `empty_kv_stream.rs`：EmptyKVStream
- [ ] 单元测试

**验收标准**:
- 只触发通知，不读取文件
- EmptyKVStream 不调用 handler

### 5.5 Phase 5: 类型系统集成 (1天)

**任务**:
- [ ] 实现 `loader_factory.rs`：工厂函数
- [ ] 注册到类型系统：`register::<KvFileLoader, KvFileLoaderConfig>`
- [ ] 实现 `create_loader_from_type_options`
- [ ] 集成测试：通过配置创建 loader

**验收标准**:
- 可以通过 JSON 配置创建 loader
- 符合 cfg/README.md 的设计模式
- 类型名称规范正确

### 5.6 Phase 6: 文档和示例 (1天)

**任务**:
- [ ] 更新 `src/kv/loader/mod.rs` 的文档注释
- [ ] 添加使用示例到各模块
- [ ] 编写 README.md（如果需要）
- [ ] 性能测试：100 文件监听

## 6. 关键技术点

### 6.1 单例模式实现

使用 `OnceLock` (Rust 1.70+) 或 `lazy_static`:

```rust
use std::sync::OnceLock;

static INSTANCE: OnceLock<Arc<Mutex<GlobalFileWatcher>>> = OnceLock::new();

impl GlobalFileWatcher {
    pub fn instance() -> Arc<Mutex<Self>> {
        INSTANCE.get_or_init(|| {
            let watcher = notify::recommended_watcher(...).unwrap();
            Arc::new(Mutex::new(GlobalFileWatcher { watcher, ... }))
        }).clone()
    }
}
```

### 6.2 泛型单例的挑战

**问题**: EventDispatcher 是泛型 `<K, V>`，每个类型组合需要不同的实例

**解决方案**:
- 方案A: 使用 `typemap` crate
- 方案B: 每个具体的 K,V 组合注册自己的单例
- 方案C: 使用 `Any` 类型擦除（推荐）

```rust
// 方案 C: 类型擦除
type ErasedListener = Box<dyn Any + Send + Sync>;

pub struct EventDispatcher {
    // Path -> Vec<Box<dyn Any>>
    listeners: Arc<Mutex<HashMap<PathBuf, Vec<ErasedListener>>>>,
}
```

### 6.3 线程安全

- 所有共享状态使用 `Arc<Mutex<T>>` 保护
- Channel 用于线程间通信
- 避免死锁：固定的加锁顺序

### 6.4 优雅关闭

```rust
impl<K, V> Loader<K, V> for KvFileLoader<K, V> {
    fn close(&mut self) -> Result<(), LoaderError> {
        // 1. 从 dispatcher 取消注册
        if let Some(id) = self.listener_id.take() {
            EventDispatcher::instance().unregister(&self.file_path, id);
        }

        // 2. 从 watcher 取消订阅
        GlobalFileWatcher::instance().unwatch(&self.file_path);

        Ok(())
    }
}
```

## 7. 依赖项

```toml
[dependencies]
notify = "6.1"          # 文件系统监听
thiserror = "1.0"       # 错误处理
log = "0.4"             # 日志
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 内部依赖
crate::kv::parser = { path = "../parser" }
crate::cfg = { path = "../../cfg" }
```

## 8. 测试策略

### 8.1 单元测试
- `global_watcher.rs`: 测试文件监听和订阅
- `event_dispatcher.rs`: 测试事件分发
- `kv_file_stream.rs`: 测试文件读取
- `kv_file_loader.rs`: 测试加载逻辑

### 8.2 集成测试
- 完整流程：创建 loader → 初始加载 → 修改文件 → 触发重新加载
- 多文件并发：100 个文件同时监听
- 错误处理：文件不存在、解析失败等

### 8.3 性能测试
- 线程数验证：确保只有 2 个线程
- 内存占用：监控 100 个文件的内存使用
- 事件延迟：文件变化到触发 listener 的时间

## 9. 使用示例

```rust
use rustx::kv::loader::{KvFileLoader, KvFileLoaderConfig, Loader};
use rustx::cfg::TypeOptions;

// 1. 准备配置
let config = KvFileLoaderConfig {
    file_path: "/tmp/data.txt".to_string(),
    parser: TypeOptions::from_json(r#{
        "type": "LineParser",
        "options": {
            "separator": "\t"
        }
    }"#)?,
    skip_dirty_rows: true,
    scanner_buffer_size: 65536,
};

// 2. 创建 loader
let mut loader = KvFileLoader::<String, String>::new(config)?;

// 3. 注册监听器
loader.on_change(Box::new(|stream| {
    stream.each(|ct, key, value| {
        println!("{:?}: {} = {}", ct, key, value);
        Ok(())
    })
}))?;

// 4. 使用中...
// 当 /tmp/data.txt 文件变化时，自动触发重新加载

// 5. 关闭
loader.close()?;
```

## 10. 风险和挑战

### 10.1 技术风险
- **泛型单例**: 需要仔细设计类型擦除方案
- **线程安全**: 避免死锁和数据竞争
- **文件监听**: notify 在不同平台的差异

### 10.2 缓解措施
- 充分的单元测试和集成测试
- 使用成熟的 crate（notify、crossbeam）
- 代码审查和性能测试

## 11. 时间估算

| 阶段 | 工作量 | 依赖 |
|------|--------|------|
| Phase 1: 核心基础 | 1-2天 | - |
| Phase 2: KvFileStream | 1天 | Phase 1 |
| Phase 3: KvFileLoader | 1-2天 | Phase 1,2 |
| Phase 4: FileTrigger | 1天 | Phase 3 |
| Phase 5: 类型系统 | 1天 | Phase 3,4 |
| Phase 6: 文档测试 | 1天 | Phase 5 |
| **总计** | **6-8天** | - |

## 12. 参考资料

- Go 版本实现: `/Users/hatlonely/Documents/github.com/hatlonely/gox/kv/loader`
- cfg 模块设计: `src/cfg/README.md`
- parser 模块设计: `src/kv/parser/README.md`
- notify crate: https://docs.rs/notify/

## 13. 版本历史

- v1.0 (2025-01-17): 初始设计方案
