use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::fs::{watch, FileEvent};
use crate::kv::loader::core::{Loader, Listener, LoaderError};

/// FileTrigger 配置（遵循 cfg/README.md 最佳实践）
///
/// 监听文件变化并触发通知，不读取文件内容。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FileTriggerConfig {
    /// 文件路径
    pub file_path: String,
}

/// 文件触发器：监听文件变化并触发通知，不读取文件内容
///
/// 其作用是只通知使用者数据发生了变化，使用者自己加载对应的数据。
///
/// 对应 Golang 的 FileTrigger[K, V]
///
/// 使用全局 FileWatcher 实例，共享线程池。
pub struct FileTrigger<K, V> {
    file_path: String,
    is_running: Arc<AtomicBool>,
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V> FileTrigger<K, V>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// 唯一的构造方法（遵循 cfg/README.md 最佳实践）
    ///
    /// # 参数
    /// - `config`: 触发器配置
    pub fn new(config: FileTriggerConfig) -> Result<Self, LoaderError> {
        Ok(Self {
            file_path: config.file_path,
            is_running: Arc::new(AtomicBool::new(false)),
            _phantom: std::marker::PhantomData,
        })
    }

    /// 触发通知（内部方法）
    fn trigger(&self, listener: &Listener<K, V>) {
        // 创建空数据流
        let stream = Arc::new(super::empty_stream::EmptyStream::new());

        if let Err(e) = listener(stream) {
            log::error!("listener failed: {}", e);
        }
    }
}

impl<K, V> Loader<K, V> for FileTrigger<K, V>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn on_change(&mut self, listener: Listener<K, V>) -> Result<(), LoaderError> {
        // 立即触发初始通知（不加载任何数据）
        self.trigger(&listener);

        // 设置运行标志
        self.is_running.store(true, Ordering::SeqCst);

        // 创建文件监听器
        let file_path = self.file_path.clone();
        let file_path_for_log = file_path.clone();
        let listener_clone = listener.clone();
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
                    // 变化时触发通知（不加载任何数据）
                    let stream = Arc::new(super::empty_stream::EmptyStream::new());

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
impl<K, V> From<FileTriggerConfig> for FileTrigger<K, V>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(config: FileTriggerConfig) -> Self {
        Self::new(config).expect("Failed to create FileTrigger")
    }
}

// 实现 From<Box<FileTrigger>> for Box<dyn Loader>（注册系统需要）
impl<K, V> From<Box<FileTrigger<K, V>>> for Box<dyn super::Loader<K, V>>
where
    K: Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<FileTrigger<K, V>>) -> Self {
        source as Box<dyn super::Loader<K, V>>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_file_trigger_config() {
        let json = r#"{
            "file_path": "/tmp/test.txt"
        }"#;

        let config: FileTriggerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.file_path, "/tmp/test.txt");
    }

    #[test]
    fn test_file_trigger_new() {
        let config = FileTriggerConfig {
            file_path: "/tmp/test.txt".to_string(),
        };

        let trigger: FileTrigger<String, String> = FileTrigger::new(config).unwrap();
        assert_eq!(trigger.file_path, "/tmp/test.txt");
    }

    #[test]
    fn test_file_trigger_on_change() {
        let config = FileTriggerConfig {
            file_path: "/tmp/nonexistent.txt".to_string(),
        };

        let mut trigger: FileTrigger<String, String> = FileTrigger::new(config).unwrap();

        // 测试初始触发
        let call_count = Arc::new(Mutex::new(0));
        let stream_call_count = Arc::new(Mutex::new(0));

        let call_count_clone = call_count.clone();
        let stream_call_count_clone = stream_call_count.clone();

        let listener: Listener<String, String> = Arc::new(move |stream| {
            let mut count = call_count_clone.lock().unwrap();
            *count += 1;
            drop(count);

            // 验证收到的是空流
            stream.each(&|_change_type, _key, _value| {
                let mut stream_count = stream_call_count_clone.lock().unwrap();
                *stream_count += 1;
                Ok(())
            })
        });

        let _ = trigger.on_change(listener);

        thread::sleep(Duration::from_millis(100));

        // 验证初始触发
        let count = call_count.lock().unwrap();
        assert!(*count >= 1);

        // 验证收到的是空流（stream_call_count 应该为 0）
        let stream_count = stream_call_count.lock().unwrap();
        assert_eq!(*stream_count, 0);

        // 清理
        let _ = trigger.close();
    }

    #[test]
    fn test_file_trigger_close() {
        let config = FileTriggerConfig {
            file_path: "/tmp/test.txt".to_string(),
        };

        let mut trigger: FileTrigger<String, String> = FileTrigger::new(config).unwrap();

        let listener: Listener<String, String> = Arc::new(|_stream| Ok(()));

        let _ = trigger.on_change(listener);
        let close_result = trigger.close();

        assert!(close_result.is_ok());
    }
}
