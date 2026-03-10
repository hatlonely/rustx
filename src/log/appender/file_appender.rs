use crate::log::appender::LogAppender;
use anyhow::Result;
use serde::Deserialize;
use smart_default::SmartDefault;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::sync::Mutex as StdMutex;

/// FileAppender 配置
#[derive(Debug, Clone, Deserialize, SmartDefault)]
pub struct FileAppenderConfig {
    /// 日志文件路径
    #[default = "app.log"]
    pub file_path: String,
}

/// 文件输出器
///
/// 将日志输出到文件，同时支持同步和异步调用
pub struct FileAppender {
    sync_file: Arc<StdMutex<std::fs::File>>,
    async_file: Arc<Mutex<tokio::fs::File>>,
    config: FileAppenderConfig,
}

impl FileAppender {
    pub fn new(config: FileAppenderConfig) -> Self {
        use std::fs::OpenOptions;

        let path = PathBuf::from(&config.file_path);

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let std_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .expect("Failed to open log file");

        // 克隆文件句柄（共享同一个文件描述符）
        let std_file_clone = std_file
            .try_clone()
            .expect("Failed to clone file handle");

        Self {
            sync_file: Arc::new(StdMutex::new(std_file)),
            async_file: Arc::new(Mutex::new(tokio::fs::File::from_std(std_file_clone))),
            config,
        }
    }

    /// 获取日志文件路径
    pub fn path(&self) -> &str {
        &self.config.file_path
    }

    /// 获取同步文件句柄（用于外部直接访问）
    pub fn sync_file(&self) -> &Arc<StdMutex<std::fs::File>> {
        &self.sync_file
    }

    /// 获取异步文件句柄（用于外部直接访问）
    pub fn async_file(&self) -> &Arc<Mutex<tokio::fs::File>> {
        &self.async_file
    }
}

#[async_trait::async_trait]
impl LogAppender for FileAppender {
    async fn append(&self, formatted_message: &str) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let mut file = self.async_file.lock().await;
        file.write_all(formatted_message.as_bytes()).await?;
        file.write_all(b"\n").await?;
        let file_ref = &mut *file;
        tokio::io::AsyncWriteExt::flush(file_ref).await?;
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        let mut file = self.async_file.lock().await;
        let file_ref = &mut *file;
        tokio::io::AsyncWriteExt::flush(file_ref).await?;
        Ok(())
    }

    fn append_sync(&self, formatted_message: &str) -> Result<()> {
        use std::io::Write;
        let mut file = self.sync_file.lock().unwrap();
        writeln!(file, "{}", formatted_message)?;
        file.flush()?;
        Ok(())
    }

    fn flush_sync(&self) -> Result<()> {
        use std::io::Write;
        let mut file = self.sync_file.lock().unwrap();
        file.flush()?;
        Ok(())
    }
}

// 使用 new 方法（同步）实现 From trait
crate::impl_from!(FileAppenderConfig => FileAppender);
crate::impl_box_from!(FileAppender => dyn LogAppender);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_file_appender_async() -> Result<()> {
        let temp_file = tempfile::NamedTempFile::new()?;
        let config = FileAppenderConfig {
            file_path: temp_file.path().to_string_lossy().to_string(),
        };

        let appender = FileAppender::new(config);

        appender.append("First message").await?;
        appender.append("Second message").await?;

        // 验证文件内容
        let contents = tokio::fs::read_to_string(temp_file.path()).await?;
        assert!(contents.contains("First message"));
        assert!(contents.contains("Second message"));

        Ok(())
    }

    #[tokio::test]
    async fn test_file_appender_flush() -> Result<()> {
        let temp_file = tempfile::NamedTempFile::new()?;
        let config = FileAppenderConfig {
            file_path: temp_file.path().to_string_lossy().to_string(),
        };

        let appender = FileAppender::new(config);
        appender.append("Message").await?;
        appender.flush().await?;

        Ok(())
    }

    #[test]
    fn test_file_appender_from_config() {
        let config = FileAppenderConfig {
            file_path: "/tmp/test.log".to_string(),
        };

        let appender = FileAppender::from(config);
        assert_eq!(appender.config.file_path, "/tmp/test.log");
    }

    #[test]
    fn test_file_appender_sync() -> Result<()> {
        let temp_file = tempfile::NamedTempFile::new()?;
        let config = FileAppenderConfig {
            file_path: temp_file.path().to_string_lossy().to_string(),
        };

        let appender = FileAppender::new(config);

        // 测试同步 append
        appender.append_sync("First sync message")?;
        appender.append_sync("Second sync message")?;

        // 刷新
        appender.flush_sync()?;

        // 验证文件内容
        let contents = std::fs::read_to_string(temp_file.path())?;
        assert!(contents.contains("First sync message"));
        assert!(contents.contains("Second sync message"));

        Ok(())
    }

    #[test]
    fn test_file_appender_mixed_sync_async() -> Result<()> {
        let temp_file = tempfile::NamedTempFile::new()?;
        let config = FileAppenderConfig {
            file_path: temp_file.path().to_string_lossy().to_string(),
        };

        let appender = Arc::new(FileAppender::new(config));
        let appender_clone = appender.clone();

        // 在异步任务中写入
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async move {
            appender.append("Async message").await?;
            Ok::<(), anyhow::Error>(())
        })?;

        // 在同步上下文中写入
        appender_clone.append_sync("Sync message")?;
        appender_clone.flush_sync()?;

        // 验证文件内容
        let contents = std::fs::read_to_string(temp_file.path())?;
        assert!(contents.contains("Async message"));
        assert!(contents.contains("Sync message"));

        Ok(())
    }

    #[tokio::test]
    async fn test_file_appender_creates_directory() -> Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let log_path = temp_dir.path().join("nested").join("dir").join("test.log");

        let config = FileAppenderConfig {
            file_path: log_path.to_string_lossy().to_string(),
        };

        let appender = FileAppender::new(config);
        appender.append("Test").await?;

        // 验证文件被创建
        assert!(log_path.exists());

        Ok(())
    }
}
