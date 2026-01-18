use crate::log::appender::LogAppender;
use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// FileAppender 配置
#[derive(Debug, Clone, Deserialize)]
pub struct FileAppenderConfig {
    /// 日志文件路径
    pub file_path: String,
}

/// 文件输出器
///
/// 将日志输出到文件
pub struct FileAppender {
    file: Arc<Mutex<tokio::fs::File>>,
    config: FileAppenderConfig,
}

impl FileAppender {
    /// 从配置创建 FileAppender（异步方法，推荐使用）
    pub async fn from_config(config: FileAppenderConfig) -> Result<Self> {
        let path = PathBuf::from(&config.file_path);

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            config,
        })
    }

    /// 同步构造方法（使用阻塞 I/O，用于支持 From trait）
    pub fn new(config: FileAppenderConfig) -> Self {
        use std::fs::OpenOptions;

        let path = PathBuf::from(&config.file_path);

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .expect("Failed to open log file");

        Self {
            file: Arc::new(Mutex::new(tokio::fs::File::from_std(file))),
            config,
        }
    }

    /// 获取日志文件路径
    pub fn path(&self) -> &str {
        &self.config.file_path
    }
}

#[async_trait::async_trait]
impl LogAppender for FileAppender {
    async fn append(&self, formatted_message: &str) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let mut file = self.file.lock().await;
        file.write_all(formatted_message.as_bytes()).await?;
        file.write_all(b"\n").await?;
        let file_ref = &mut *file;
        tokio::io::AsyncWriteExt::flush(file_ref).await?;
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        let mut file = self.file.lock().await;
        let file_ref = &mut *file;
        tokio::io::AsyncWriteExt::flush(file_ref).await?;
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

        let appender = FileAppender::from_config(config).await?;

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

        let appender = FileAppender::from_config(config).await?;
        appender.append("Message").await?;
        appender.flush().await?;

        Ok(())
    }

    #[test]
    fn test_file_appender_sync() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let config = FileAppenderConfig {
            file_path: temp_file.path().to_string_lossy().to_string(),
        };

        let appender = FileAppender::new(config);
        assert_eq!(appender.path(), temp_file.path().to_string_lossy().as_ref());
    }

    #[test]
    fn test_file_appender_from_config() {
        let config = FileAppenderConfig {
            file_path: "/tmp/test.log".to_string(),
        };

        let appender = FileAppender::from(config);
        assert_eq!(appender.config.file_path, "/tmp/test.log");
    }

    #[tokio::test]
    async fn test_file_appender_creates_directory() -> Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let log_path = temp_dir.path().join("nested").join("dir").join("test.log");

        let config = FileAppenderConfig {
            file_path: log_path.to_string_lossy().to_string(),
        };

        let appender = FileAppender::from_config(config).await?;
        appender.append("Test").await?;

        // 验证文件被创建
        assert!(log_path.exists());

        Ok(())
    }
}
