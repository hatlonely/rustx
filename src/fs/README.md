# 文件系统监听 (File System Watcher)

监听文件变化，当文件被创建、修改或删除时触发回调。

## 快速开始

```rust
use rustx::fs::{watch, FileEvent};

// 监听单个文件
watch("config.json", |event| {
    match event {
        FileEvent::Created(path) => println!("文件创建: {:?}", path),
        FileEvent::Modified(path) => println!("文件修改: {:?}", path),
        FileEvent::Deleted(path) => println!("文件删除: {:?}", path),
        FileEvent::Error(err) => println!("错误: {}", err),
    }
}).unwrap();
```

## 监听多个文件

```rust
use rustx::fs::watch;

// 监听多个文件
watch("config.json", |_| {}).unwrap();
watch("data.yaml", |_| {}).unwrap();
watch("log.txt", |_| {}).unwrap();
```

## 停止所有监听

```rust
use rustx::fs::unwatch_all;

// 停止所有文件监听
unwatch_all();
```

## API 文档

### `watch(filepath, handler)`

监听指定文件的变化。

**参数：**
- `filepath`: 要监听的文件路径（支持 `&str`, `String`, `PathBuf` 等）
- `handler`: 事件处理回调函数，接收 `FileEvent`

**返回：** `Result<()>`

**示例：**
```rust
watch("/path/to/file", |event| {
    // 处理事件
})?;
```

### `unwatch_all()`

停止所有文件监听。

**示例：**
```rust
unwatch_all();
```

### `FileEvent`

文件事件枚举：

- `Created(PathBuf)` - 文件被创建
- `Modified(PathBuf)` - 文件被修改
- `Deleted(PathBuf)` - 文件被删除
- `Error(String)` - 发生错误

## 高级用法：独立监听器

如果需要多个独立的监听器（可以分别停止），使用 `FileWatcher` 实例：

```rust
use rustx::fs::{FileWatcher, FileEvent};

let mut watcher = FileWatcher::new();

watcher.watch("file1.txt", |event| {
    // 处理 file1 的事件
})?;

watcher.watch("file2.txt", |event| {
    // 处理 file2 的事件
})?;

// watcher 在 drop 时自动清理
```

## 注意事项

1. **防抖处理**：文件修改事件有 100ms 的防抖延迟，避免重复触发
2. **跨平台**：自动处理不同平台的路径差异（如 macOS 的符号链接）
3. **不存在文件**：可以监听不存在的文件，会在文件创建时触发 `Created` 事件
4. **线程安全**：全局函数使用互斥锁，线程安全

## 运行示例

```bash
cargo run --example fs_file_watcher_usage
```
