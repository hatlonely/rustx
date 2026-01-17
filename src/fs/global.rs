//! 全局文件监听器单例
//!
//! 提供便捷的全局函数来监听文件，无需手动创建 FileWatcher 实例

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use std::path::Path;
use std::sync::Mutex;

use crate::fs::watcher::{FileEvent, FileWatcher};

/// 全局文件监听器单例
static GLOBAL_WATCHER: Lazy<Mutex<FileWatcher>> =
    Lazy::new(|| Mutex::new(FileWatcher::default()));

/// 监听文件变化（使用全局单例）
///
/// 这是一个便捷函数，使用内部的 `FileWatcher` 单例来监听文件。
/// 适合简单的使用场景，不需要手动管理 `FileWatcher` 实例。
///
/// # 参数
///
/// - `filepath`: 要监听的文件路径
/// - `handler`: 事件处理回调函数
///
/// # 示例
///
/// ```no_run
/// use rustx::fs::watch;
///
/// // 直接监听文件，无需创建 FileWatcher 实例
/// watch("config.json", |event| {
///     println!("文件事件: {:?}", event);
/// }).unwrap();
/// ```
///
/// # 注意
///
/// - 全局单例在程序整个生命周期内存在
/// - 如需清理所有监听，请使用 `unwatch_all()`
/// - 如果需要多个独立的监听器，请使用 `FileWatcher::new()` 创建实例
pub fn watch<F>(filepath: impl AsRef<Path>, handler: F) -> Result<()>
where
    F: Fn(FileEvent) + Send + Sync + 'static,
{
    GLOBAL_WATCHER
        .lock()
        .map_err(|e| anyhow!("获取全局锁失败: {}", e))?
        .watch(filepath, handler)
}

/// 停止所有文件监听（使用全局单例）
///
/// 停止全局监听器中的所有监听任务。
///
/// # 示例
///
/// ```no_run
/// use rustx::fs::{watch, unwatch_all};
///
/// watch("file1.txt", |_| {}).unwrap();
/// watch("file2.txt", |_| {}).unwrap();
///
/// // 停止所有监听
/// unwatch_all();
/// ```
pub fn unwatch_all() {
    if let Ok(mut watcher) = GLOBAL_WATCHER.lock() {
        watcher.unwatch_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_global_watch() -> Result<()> {
        // 清理全局状态
        unwatch_all();
        thread::sleep(Duration::from_millis(100));

        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("global_test.txt");

        fs::write(&file_path, "initial content")?;

        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        // 使用全局函数监听
        watch(&file_path, move |event| {
            events_clone.lock().unwrap().push(event);
        })?;

        thread::sleep(Duration::from_millis(200));

        fs::write(&file_path, "modified content")?;
        thread::sleep(Duration::from_millis(1500)); // 增加等待时间，确保防抖处理完成

        let events_vec = events.lock().unwrap();
        let has_modify = events_vec
            .iter()
            .any(|e| matches!(e, FileEvent::Modified(_)));

        assert!(has_modify, "应该收到文件修改事件");

        Ok(())
    }

    #[test]
    #[serial]
    fn test_global_unwatch_all() -> Result<()> {
        // 清理全局状态
        unwatch_all();
        thread::sleep(Duration::from_millis(100));

        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("global_unwatch_test.txt");

        fs::write(&file_path, "content")?;

        // 使用全局函数监听
        watch(&file_path, |event| {
            println!("事件: {:?}", event);
        })?;

        thread::sleep(Duration::from_millis(100));

        // 停止所有监听
        unwatch_all();

        thread::sleep(Duration::from_millis(100));

        Ok(())
    }

    #[test]
    #[serial]
    fn test_multiple_global_watches() -> Result<()> {
        // 清理全局状态
        unwatch_all();
        thread::sleep(Duration::from_millis(100));

        let temp_dir = TempDir::new()?;
        let file1 = temp_dir.path().join("global_multi_1.txt");
        let file2 = temp_dir.path().join("global_multi_2.txt");

        fs::write(&file1, "content1")?;
        fs::write(&file2, "content2")?;

        let count = Arc::new(Mutex::new(0));
        let count_clone = count.clone();

        // 监听多个文件
        watch(&file1, move |_event| {
            let mut c = count_clone.lock().unwrap();
            *c += 1;
        })?;

        let count_clone2 = count.clone();
        watch(&file2, move |_event| {
            let mut c = count_clone2.lock().unwrap();
            *c += 1;
        })?;

        thread::sleep(Duration::from_millis(200));

        // 修改两个文件
        fs::write(&file1, "modified1")?;
        thread::sleep(Duration::from_millis(200));
        fs::write(&file2, "modified2")?;

        thread::sleep(Duration::from_millis(1500)); // 增加等待时间，确保防抖处理完成

        // 验证两个文件的事件都被触发
        let event_count = *count.lock().unwrap();
        assert!(event_count >= 2, "应该收到至少2个事件");

        Ok(())
    }
}
