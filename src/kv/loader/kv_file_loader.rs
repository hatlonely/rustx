use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cfg::TypeOptions;
use crate::fs::{watch, FileEvent};
use crate::kv::loader::core::{Loader, Listener, LoaderError};
use crate::kv::parser::Parser;

/// KvFileLoader 配置（遵循 cfg/README.md 最佳实践）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct KvFileLoaderConfig {
    /// 文件路径
    pub file_path: String,

    /// Parser 配置
    pub parser: TypeOptions,

    /// 是否跳过脏数据（默认：false，遇到脏数据时直接报错并返回）
    #[serde(default)]
    pub skip_dirty_rows: bool,

    /// Scanner buffer 最小大小（默认：65536）
    #[serde(default = "default_scanner_buffer_min_size")]
    pub scanner_buffer_min_size: usize,

    /// Scanner buffer 最大大小（默认：4194304）
    #[serde(default = "default_scanner_buffer_max_size")]
    pub scanner_buffer_max_size: usize,
}

fn default_scanner_buffer_min_size() -> usize {
    65536
}

fn default_scanner_buffer_max_size() -> usize {
    4194304
}

/// KV 文件加载器：从文件加载 KV 数据，支持文件变化监听
///
/// 该文件必须是文本文件，且每行一个 KV 数据，格式由 Parser 定义。
///
/// 使用全局 FileWatcher 实例，共享线程池。
pub struct KvFileLoader<K, V> {
    file_path: String,
    parser: Arc<dyn Parser<K, V>>,
    skip_dirty_rows: bool,
    scanner_buffer_min_size: usize,
    scanner_buffer_max_size: usize,
    is_running: Arc<AtomicBool>,
}

impl<K, V> KvFileLoader<K, V>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// 唯一的构造方法（遵循 cfg/README.md 最佳实践）
    ///
    /// # 参数
    /// - `config`: 加载器配置，包含 parser 配置
    pub fn new(config: KvFileLoaderConfig) -> Result<Self, LoaderError> {
        if config.scanner_buffer_min_size == 0 {
            return Err(LoaderError::LoadFailed(
                "scanner_buffer_min_size must be greater than 0".to_string(),
            ));
        }

        if config.scanner_buffer_max_size == 0 {
            return Err(LoaderError::LoadFailed(
                "scanner_buffer_max_size must be greater than 0".to_string(),
            ));
        }

        // 从 config.parser 创建 parser 实例
        let parser: Box<dyn Parser<K, V>> =
            crate::cfg::create_trait_from_type_options(&config.parser).map_err(|e| {
                LoaderError::LoadFailed(format!("Failed to create parser: {}", e))
            })?;

        Ok(Self {
            file_path: config.file_path,
            parser: parser.into(),
            skip_dirty_rows: config.skip_dirty_rows,
            scanner_buffer_min_size: config.scanner_buffer_min_size,
            scanner_buffer_max_size: config.scanner_buffer_max_size,
            is_running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 触发数据加载（内部方法）
    fn trigger_load(&self, listener: &Listener<K, V>) {
        let stream = Arc::new(
            super::kv_file_stream::KvFileStream::new(
                &self.file_path,
                self.parser.clone(),
                self.skip_dirty_rows,
            )
            .with_buffer_sizes(self.scanner_buffer_min_size, self.scanner_buffer_max_size),
        );

        if let Err(e) = listener(stream) {
            log::error!("listener failed: {}", e);
        }
    }
}

impl<K, V> Loader<K, V> for KvFileLoader<K, V>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn on_change(&mut self, listener: Listener<K, V>) -> Result<(), LoaderError> {
        // 立即加载初始数据
        self.trigger_load(&listener);

        // 设置运行标志
        self.is_running.store(true, Ordering::SeqCst);

        // 创建文件监听器
        let file_path = self.file_path.clone();
        let file_path_for_log = file_path.clone();
        let listener_clone = listener.clone();
        let parser_clone = self.parser.clone();
        let skip_dirty_rows = self.skip_dirty_rows;
        let scanner_buffer_min_size = self.scanner_buffer_min_size;
        let scanner_buffer_max_size = self.scanner_buffer_max_size;
        let is_running = self.is_running.clone();

        // 使用全局 watch 方法
        watch(&file_path, move |event| {
            // 检查是否还在运行
            if !is_running.load(Ordering::SeqCst) {
                return;
            }

            match event {
                FileEvent::Created(_) | FileEvent::Modified(_) => {
                    log::debug!("file changed: {}", file_path_for_log);
                    // 创建新的 stream 并调用 listener
                    let stream = Arc::new(
                        super::kv_file_stream::KvFileStream::new(
                            &file_path_for_log,
                            parser_clone.clone(),
                            skip_dirty_rows,
                        )
                        .with_buffer_sizes(scanner_buffer_min_size, scanner_buffer_max_size),
                    );

                    if let Err(e) = listener_clone(stream) {
                        log::error!("listener failed: {}", e);
                    }
                }
                FileEvent::Deleted(_) => {
                    log::warn!("file deleted: {}", file_path_for_log);
                }
                FileEvent::Error(err) => {
                    log::error!("watcher error: {}", err);
                }
            }
        })?;

        Ok(())
    }

    fn close(&mut self) -> Result<(), LoaderError> {
        // 设置停止标志，回调将不再处理事件
        self.is_running.store(false, Ordering::SeqCst);
        Ok(())
    }
}

// 实现 From trait（cfg 模块注册系统需要）
// 由于 new 返回 Result，这里使用 expect 处理错误
impl<K, V> From<KvFileLoaderConfig> for KvFileLoader<K, V>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(config: KvFileLoaderConfig) -> Self {
        Self::new(config).expect("Failed to create KvFileLoader")
    }
}

// 实现 From<Box<KvFileLoader>> for Box<dyn Loader>（注册系统需要）
impl<K, V> From<Box<KvFileLoader<K, V>>> for Box<dyn super::Loader<K, V>>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<KvFileLoader<K, V>>) -> Self {
        source as Box<dyn super::Loader<K, V>>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use tempfile::NamedTempFile;

    #[test]
    fn test_kv_file_loader_config_default() {
        let json = r#"{
            "file_path": "/tmp/test.txt",
            "parser": {
                "type": "LineParser",
                "options": {
                    "separator": "\t"
                }
            }
        }"#;

        let config: KvFileLoaderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.file_path, "/tmp/test.txt");
        assert_eq!(config.skip_dirty_rows, false);
        assert_eq!(config.scanner_buffer_min_size, 65536);
        assert_eq!(config.scanner_buffer_max_size, 4194304);
    }

    #[test]
    fn test_kv_file_loader_config_custom() {
        let json = r#"{
            "file_path": "/tmp/test.txt",
            "parser": {
                "type": "LineParser",
                "options": {
                    "separator": ","
                }
            },
            "skip_dirty_rows": true,
            "scanner_buffer_min_size": 1024,
            "scanner_buffer_max_size": 2048
        }"#;

        let config: KvFileLoaderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.file_path, "/tmp/test.txt");
        assert_eq!(config.skip_dirty_rows, true);
        assert_eq!(config.scanner_buffer_min_size, 1024);
        assert_eq!(config.scanner_buffer_max_size, 2048);
    }

    #[test]
    fn test_kv_file_loader_initial_load() {
        // 创建临时文件
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "key1\tvalue1").unwrap();
        writeln!(temp_file, "key2\tvalue2").unwrap();
        writeln!(temp_file, "key3\tvalue3").unwrap();

        // 注册 Parser 类型
        crate::kv::parser::register_parsers::<String, String>().unwrap();

        // 创建 loader config（使用一个有效的 parser 配置）
        let parser_opts = crate::cfg::TypeOptions::from_json(
            r#"{"type": "LineParser", "options": {"separator": "\t"}}"#,
        )
        .unwrap();
        let config = KvFileLoaderConfig {
            file_path: temp_file.path().to_string_lossy().to_string(),
            parser: parser_opts,
            skip_dirty_rows: false,
            scanner_buffer_min_size: 65536,
            scanner_buffer_max_size: 4194304,
        };

        // 创建 loader（parser 会从 config.parser 自动创建）
        let mut loader = KvFileLoader::new(config).unwrap();

        // 测试初始加载
        let call_count = Arc::new(Mutex::new(0));
        let results = Arc::new(Mutex::new(Vec::new()));

        let call_count_clone = call_count.clone();
        let results_clone = results.clone();

        let listener: Listener<String, String> = Arc::new(move |stream| {
            let mut count = call_count_clone.lock().unwrap();
            *count += 1;
            drop(count);

            stream.each(&|_change_type, key, value| {
                let mut results = results_clone.lock().unwrap();
                results.push(format!("{}:{}", key, value));
                Ok(())
            })
        });

        // 启动监听（会立即加载初始数据）
        let _ = loader.on_change(listener);

        thread::sleep(Duration::from_millis(100));

        // 验证初始加载
        let count = call_count.lock().unwrap();
        assert!(*count >= 1);

        let results = results.lock().unwrap();
        assert!(results.contains(&"key1:value1".to_string()));
        assert!(results.contains(&"key2:value2".to_string()));
        assert!(results.contains(&"key3:value3".to_string()));

        // 清理
        let _ = loader.close();
    }

    #[test]
    fn test_kv_file_loader_close() {
        // 注册 Parser 类型
        crate::kv::parser::register_parsers::<String, String>().unwrap();

        let parser_opts = crate::cfg::TypeOptions::from_json(
            r#"{"type": "LineParser", "options": {"separator": "\t"}}"#,
        )
        .unwrap();
        let config = KvFileLoaderConfig {
            file_path: "/tmp/test.txt".to_string(),
            parser: parser_opts,
            skip_dirty_rows: false,
            scanner_buffer_min_size: 65536,
            scanner_buffer_max_size: 4194304,
        };

        let mut loader = KvFileLoader::new(config).unwrap();

        // 创建一个空的 listener
        let listener: Listener<String, String> = Arc::new(|_stream| Ok(()));

        // 由于文件不存在，初始加载会失败，但这不影响 watcher 的启动
        // 我们可以测试 close 不 panic
        let _ = loader.on_change(listener);
        let close_result = loader.close();
        assert!(close_result.is_ok());
    }
}
