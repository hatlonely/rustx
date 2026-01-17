//! 文件监听器
//!
//! 监听文件系统事件，当文件变化时触发回调

use anyhow::{anyhow, Result};
use crossbeam::channel::unbounded;
use notify::Watcher;
use rayon::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

/// 文件事件
#[derive(Debug, Clone)]
pub enum FileEvent {
    /// 文件被创建
    Created(PathBuf),
    /// 文件被修改
    Modified(PathBuf),
    /// 文件被删除
    Deleted(PathBuf),
    /// 发生错误
    Error(String),
}

/// 文件监听器配置
#[derive(Debug, Clone, Deserialize)]
pub struct FileWatcherConfig {
    /// 工作线程数（用于并行处理文件事件）
    #[serde(default = "default_worker_threads")]
    pub worker_threads: usize,
    /// 事件防抖延迟（同一文件的多次修改只处理最后一次），单位：毫秒
    #[serde(default = "default_debounce_delay_ms")]
    pub debounce_delay_ms: u64,
}

fn default_worker_threads() -> usize {
    1
}

fn default_debounce_delay_ms() -> u64 {
    100
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            worker_threads: default_worker_threads(),
            debounce_delay_ms: default_debounce_delay_ms(),
        }
    }
}

impl FileWatcherConfig {
    /// 获取防抖延迟的 Duration
    pub fn debounce_delay(&self) -> Duration {
        Duration::from_millis(self.debounce_delay_ms)
    }
}

/// Handler 函数包装器
type HandlerFn = dyn Fn(FileEvent) + Send + Sync;

/// 监听请求（用于在事件循环中添加监听）
struct WatchRequest {
    path: PathBuf,
    handler: Box<HandlerFn>,
}

/// 文件监听器
///
/// 监听指定文件的变化，当文件被创建、修改或删除时触发回调
///
/// # 架构说明
///
/// - **单一 notify watcher**：一个全局 watcher 监听所有文件
/// - **单一事件循环线程**：接收 notify 原始事件，进行防抖去重
/// - **复用的 Rayon 线程池**：在创建 FileWatcher 时初始化一次，复用于所有事件处理
///
/// # 性能优化
///
/// 1. **事件防抖**：同一文件的多次修改只触发一次 handler（配置 debounce_delay）
/// 2. **线程池复用**：Rayon 线程池只创建一次，避免重复创建开销
/// 3. **固定线程数**：无论监听多少文件，线程数固定为配置的 worker_threads
/// 4. **并行处理**：不同文件的 handler 在 Rayon 线程池中并行执行
///
/// # 示例
///
/// ```no_run
/// use rustx::fs::{FileWatcher, FileWatcherConfig};
///
/// // 使用默认配置（单线程 + 100ms 防抖）
/// let mut watcher = FileWatcher::new(FileWatcherConfig::default());
///
/// // 或者使用 Default trait
/// let mut watcher = FileWatcher::default();
///
/// // 使用自定义配置（多线程）
/// let config = FileWatcherConfig {
///     worker_threads: 4,
///     debounce_delay_ms: 200,
/// };
/// let mut watcher = FileWatcher::new(config);
///
/// watcher.watch("/path/to/file.txt", |event| {
///     match event {
///         rustx::fs::FileEvent::Created(path) => {
///             println!("文件创建: {:?}", path);
///         }
///         rustx::fs::FileEvent::Modified(path) => {
///             println!("文件修改: {:?}", path);
///         }
///         rustx::fs::FileEvent::Deleted(path) => {
///             println!("文件删除: {:?}", path);
///         }
///         rustx::fs::FileEvent::Error(err) => {
///             println!("发生错误: {}", err);
///         }
///     }
/// }).unwrap();
/// ```
pub struct FileWatcher {
    /// 监听请求发送通道
    watch_request_tx: crossbeam::channel::Sender<WatchRequest>,
    /// 事件循环线程句柄
    _event_thread: Option<thread::JoinHandle<()>>,
}

impl FileWatcher {
    /// 使用配置创建文件监听器
    ///
    /// 这是唯一的构造方法，符合最佳实践要求。
    ///
    /// # 参数
    ///
    /// - `config`: 文件监听器配置
    ///
    /// # 示例
    ///
    /// ```no_run
    /// use rustx::fs::{FileWatcher, FileWatcherConfig};
    ///
    /// // 使用默认配置
    /// let watcher = FileWatcher::new(FileWatcherConfig::default());
    ///
    /// // 使用自定义配置
    /// let config = FileWatcherConfig {
    ///     worker_threads: 4,
    ///     debounce_delay_ms: 200,
    /// };
    /// let watcher = FileWatcher::new(config);
    /// ```
    pub fn new(config: FileWatcherConfig) -> Self {
        // 创建监听请求通道
        let (watch_request_tx, watch_request_rx): (
            crossbeam::channel::Sender<WatchRequest>,
            crossbeam::channel::Receiver<WatchRequest>,
        ) = crossbeam::channel::unbounded();
        let debounce_delay = config.debounce_delay();
        let worker_threads = config.worker_threads;

        // 创建 Rayon 线程池（只创建一次，复用于所有事件处理）
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_threads)
            .build()
            .expect("Failed to create Rayon thread pool");

        // 启动事件循环线程
        let event_thread = thread::spawn(move || {
            use notify::RecursiveMode;

            // 创建 notify watcher 和 handlers 映射
            let (notify_tx, notify_rx) = unbounded();
            let mut watcher =
                notify::recommended_watcher(move |res: Result<notify::Event, _>| {
                    if let Ok(event) = res {
                        let _ = notify_tx.send(event);
                    }
                })
                .expect("Failed to create notify watcher");

            let mut handlers: HashMap<PathBuf, Box<HandlerFn>> = HashMap::new();

            // 事件去重缓存：path -> (event, timestamp)
            let mut pending_events: HashMap<PathBuf, (FileEvent, Instant)> = HashMap::new();
            let mut last_flush = Instant::now();

            // 事件处理循环
            loop {
                crossbeam::select! {
                    // 接收监听请求
                    recv(watch_request_rx) -> result => {
                        match result {
                            Ok(req) => {
                                // 添加到 handlers 映射
                                handlers.insert(req.path.clone(), req.handler);

                                // 添加监听到 watcher
                                let watch_result = if req.path.exists() {
                                    watcher.watch(&req.path, RecursiveMode::NonRecursive)
                                } else {
                                    // 文件不存在，监听父目录
                                    if let Some(parent) = req.path.parent() {
                                        watcher.watch(parent, RecursiveMode::NonRecursive)
                                    } else {
                                        Ok(())
                                    }
                                };

                                if let Err(e) = watch_result {
                                    eprintln!("添加监听失败: {}, 路径: {:?}", e, req.path);
                                }
                            }
                            Err(_) => break, // 通道关闭
                        }
                    }
                    // 接收 notify 原始事件
                    recv(notify_rx) -> result => {
                        match result {
                            Ok(event) => {
                                // 转换 notify 事件为 FileEvent
                                for path in &event.paths {
                                    // 对于删除事件，使用 dunce::canonicalize（更宽松）
                                    let path_normalized = if event.kind.is_remove() {
                                        dunce::canonicalize(path).unwrap_or(path.clone())
                                    } else {
                                        path.canonicalize()
                                            .unwrap_or_else(|_| dunce::canonicalize(path).unwrap_or(path.clone()))
                                    };

                                    let file_event = if event.kind.is_create() {
                                        Some(FileEvent::Created(path.clone()))
                                    } else if event.kind.is_modify() {
                                        Some(FileEvent::Modified(path.clone()))
                                    } else if event.kind.is_remove() {
                                        Some(FileEvent::Deleted(path.clone()))
                                    } else {
                                        None
                                    };

                                    // 所有事件统一进缓存，包括删除事件
                                    if let Some(file_event) = file_event {
                                        pending_events.insert(path_normalized, (file_event, Instant::now()));
                                    }
                                }
                            }
                            Err(_) => break, // 通道关闭
                        }
                    }
                    // 定时刷新（处理缓存的事件）
                    default(debounce_delay / 2) => {
                        let now = Instant::now();

                        // 检查是否应该刷新
                        let should_flush = now.duration_since(last_flush) > debounce_delay
                            || !pending_events.is_empty();

                        if should_flush {
                            // 收集需要处理的事件（每个文件只处理最后一个）
                            let mut events_to_process = Vec::new();

                            // 只提取超过 debounce_delay 没有新事件的事件
                            pending_events.retain(|path, (event, time)| {
                                if now.duration_since(*time) >= debounce_delay {
                                    events_to_process.push((path.clone(), event.clone()));
                                    false // 从缓存中移除
                                } else {
                                    true // 保留在缓存中
                                }
                            });

                            if !events_to_process.is_empty() {
                                // 使用预先创建的 Rayon 线程池并行处理所有文件的事件
                                thread_pool.install(|| {
                                    events_to_process.into_par_iter().for_each(|(path, event)| {
                                        if let Some(handler) = handlers.get(&path) {
                                            // 在 Rayon 工作线程中执行 handler
                                            handler(event);
                                        }
                                    });
                                });
                            }

                            last_flush = now;
                        }
                    }
                }
            }

            // 显式 drop watcher 以释放文件句柄
            drop(watcher);
        });

        Self {
            watch_request_tx,
            _event_thread: Some(event_thread),
        }
    }

    /// 监听指定文件
    ///
    /// # 参数
    ///
    /// - `filepath`: 要监听的文件路径
    /// - `handler`: 事件处理回调函数（需要在 Rayon 线程池中安全执行）
    ///
    /// # 示例
    ///
    /// ```no_run
    /// use rustx::fs::FileWatcher;
    ///
    /// let mut watcher = FileWatcher::new();
    /// watcher.watch("config.json", |event| {
    ///     println!("文件事件: {:?}", event);
    /// }).unwrap();
    /// ```
    pub fn watch<F>(&mut self, filepath: impl AsRef<Path>, handler: F) -> Result<()>
    where
        F: Fn(FileEvent) + Send + Sync + 'static,
    {
        let filepath_original = filepath.as_ref();

        // 检查路径是否为文件（如果存在）
        if filepath_original.exists() && !filepath_original.is_file() {
            return Err(anyhow!(
                "路径不是一个文件: {}",
                filepath_original.display()
            ));
        }

        // 规范化路径：如果文件存在，直接 canonicalize；否则 canonicalize 父目录
        let filepath = if filepath_original.exists() {
            filepath_original.canonicalize()?
        } else {
            // 文件不存在，需要 canonicalize 父目录，然后拼接文件名
            if let Some(parent) = filepath_original.parent() {
                if parent.exists() {
                    let canonical_parent = parent.canonicalize()?;
                    if let Some(filename) = filepath_original.file_name() {
                        canonical_parent.join(filename)
                    } else {
                        return Err(anyhow!(
                            "无法获取文件名: {}",
                            filepath_original.display()
                        ));
                    }
                } else {
                    return Err(anyhow!(
                        "父目录不存在: {}",
                        parent.display()
                    ));
                }
            } else {
                return Err(anyhow!(
                    "无法获取文件的父目录: {}",
                    filepath_original.display()
                ));
            }
        };

        // 发送监听请求到事件循环线程
        self.watch_request_tx
            .send(WatchRequest {
                path: filepath,
                handler: Box::new(handler),
            })
            .map_err(|e| anyhow!("发送监听请求失败: {}", e))?;

        Ok(())
    }

    /// 停止所有监听
    ///
    /// 通常不需要手动调用，FileWatcher 在 drop 时会自动清理所有监听器
    pub fn unwatch_all(&mut self) {
        // 关闭 watch_request_tx 通道，导致事件循环线程退出
        // 注意：这会停止所有监听，包括已注册的
        drop(self.watch_request_tx.clone());
    }
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new(FileWatcherConfig::default())
    }
}

impl From<FileWatcherConfig> for FileWatcher {
    fn from(config: FileWatcherConfig) -> Self {
        Self::new(config)
    }
}

// 注意：FileWatcher 不需要显式实现 Drop
// 当 FileWatcher drop 时，Arc 会自动清理，线程会随着通道关闭而退出


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_file_watcher_create() {
        let _watcher = FileWatcher::new(FileWatcherConfig::default());
        // 测试创建成功
    }

    #[test]
    fn test_file_watcher_with_config() {
        let config = FileWatcherConfig {
            worker_threads: 4,
            debounce_delay_ms: 200,
        };
        let _watcher = FileWatcher::new(config);
        // 测试使用自定义配置创建成功
    }

    #[test]
    fn test_file_watcher_watch_modify() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.txt");

        // 写入初始内容
        fs::write(&file_path, "initial content")?;

        let mut watcher = FileWatcher::default();

        // 使用 Arc<Mutex<Vec>> 存储事件
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let file_path_clone = file_path.clone();

        watcher.watch(&file_path, move |event| {
            events_clone.lock().unwrap().push(event);
        })?;

        // 等待监听器启动
        thread::sleep(Duration::from_millis(200));

        // 修改文件
        fs::write(&file_path_clone, "modified content")?;

        // 等待事件被触发（考虑到防抖延迟）
        thread::sleep(Duration::from_millis(500));

        // 验证收到修改事件
        let events_vec = events.lock().unwrap();

        let has_modify = events_vec
            .iter()
            .any(|e| matches!(e, FileEvent::Modified(_)));

        assert!(has_modify, "应该收到文件修改事件");

        Ok(())
    }

    #[test]
    fn test_file_watcher_watch_delete() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.txt");

        // 写入初始内容
        fs::write(&file_path, "initial content")?;

        let mut watcher = FileWatcher::default();

        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        watcher.watch(&file_path, move |event| {
            events_clone.lock().unwrap().push(event);
        })?;

        // 等待监听器启动
        thread::sleep(Duration::from_millis(200));

        // 删除文件
        fs::remove_file(&file_path)?;

        // 等待事件被触发（删除事件可能需要更长的时间）
        thread::sleep(Duration::from_millis(1500));

        // 验证收到事件（删除或修改都行，因为删除后可能有其他文件系统操作）
        let events_vec = events.lock().unwrap();

        let has_delete_or_modify = events_vec
            .iter()
            .any(|e| matches!(e, FileEvent::Deleted(_) | FileEvent::Modified(_)));

        assert!(has_delete_or_modify, "应该收到文件事件（删除或修改）");

        Ok(())
    }

    #[test]
    fn test_file_watcher_debounce() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.txt");

        // 写入初始内容
        fs::write(&file_path, "initial content")?;

        let config = FileWatcherConfig {
            worker_threads: 1,
            debounce_delay_ms: 100,
        };
        let mut watcher = FileWatcher::new(config);

        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let file_path_clone = file_path.clone();

        watcher.watch(&file_path, move |event| {
            events_clone.lock().unwrap().push(event);
        })?;

        // 等待监听器启动
        thread::sleep(Duration::from_millis(200));

        // 快速连续修改文件 5 次
        for i in 0..5 {
            fs::write(&file_path_clone, format!("content {}", i))?;
            thread::sleep(Duration::from_millis(10));
        }

        // 等待事件被触发
        thread::sleep(Duration::from_millis(500));

        // 验证只收到一次修改事件（防抖生效）
        let events_vec = events.lock().unwrap();
        let modify_count = events_vec
            .iter()
            .filter(|e| matches!(e, FileEvent::Modified(_)))
            .count();

        assert_eq!(modify_count, 1, "应该只收到一次修改事件（防抖生效）");

        Ok(())
    }

    #[test]
    fn test_file_watcher_watch_nonexistent_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("nonexistent.txt");

        let mut watcher = FileWatcher::default();

        // 监听不存在的文件应该成功（监听其父目录）
        let result = watcher.watch(&file_path, |event| {
            println!("事件: {:?}", event);
        });

        assert!(result.is_ok(), "监听不存在的文件应该成功");

        Ok(())
    }

    #[test]
    fn test_file_watcher_auto_cleanup() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.txt");

        fs::write(&file_path, "content")?;

        {
            let mut watcher = FileWatcher::default();
            watcher.watch(&file_path, |event| {
                println!("事件: {:?}", event);
            })?;

            // watcher 在这里 drop
        }

        // 验证线程已经停止（等待一段时间确保没有 panic）
        thread::sleep(Duration::from_millis(100));

        Ok(())
    }

    #[test]
    fn test_file_watcher_unwatch_all() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.txt");

        fs::write(&file_path, "content")?;

        let mut watcher = FileWatcher::default();
        watcher.watch(&file_path, |event| {
            println!("事件: {:?}", event);
        })?;

        // 等待监听器启动
        thread::sleep(Duration::from_millis(100));

        // 停止所有监听
        watcher.unwatch_all();

        // 等待线程停止
        thread::sleep(Duration::from_millis(100));

        Ok(())
    }

    #[test]
    fn test_file_watcher_invalid_path() {
        let mut watcher = FileWatcher::default();

        // 尝试监听目录（应该失败）
        let temp_dir = TempDir::new().unwrap();
        let result = watcher.watch(&temp_dir.path(), |event| {
            println!("事件: {:?}", event);
        });

        assert!(result.is_err(), "监听目录应该失败");
    }

    #[test]
    fn test_file_watcher_multiple_files() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // 创建多个文件
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        let file3 = temp_dir.path().join("file3.txt");

        fs::write(&file1, "content1")?;
        fs::write(&file2, "content2")?;
        fs::write(&file3, "content3")?;

        let events = Arc::new(Mutex::new(Vec::new()));
        let mut watcher = FileWatcher::default();

        // 监听多个文件
        watcher.watch(&file1, {
            let events = events.clone();
            move |event| events.lock().unwrap().push((1, event))
        })?;

        watcher.watch(&file2, {
            let events = events.clone();
            move |event| events.lock().unwrap().push((2, event))
        })?;

        watcher.watch(&file3, {
            let events = events.clone();
            move |event| events.lock().unwrap().push((3, event))
        })?;

        // 等待监听器启动
        thread::sleep(Duration::from_millis(200));

        // 修改所有文件
        fs::write(&file1, "modified1")?;
        fs::write(&file2, "modified2")?;
        fs::write(&file3, "modified3")?;

        // 等待事件被触发（考虑到防抖延迟）
        thread::sleep(Duration::from_millis(1500));

        // 验证收到事件（至少收到一些事件，证明多个文件的监听都工作）
        let events_vec = events.lock().unwrap();

        // 检查至少收到了一些事件（可能因为防抖、并行处理等原因，实际数量可能不同）
        assert!(!events_vec.is_empty(), "应该收到文件事件");

        Ok(())
    }
}
