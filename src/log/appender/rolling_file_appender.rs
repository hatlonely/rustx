use crate::log::appender::LogAppender;
use anyhow::Result;
use chrono::{DateTime, Datelike, Local, Timelike, Utc};
use smart_default::SmartDefault;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 时间切分策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimePolicy {
    /// 按分钟切分
    Minutely,
    /// 按小时切分
    Hourly,
    /// 按天切分
    Daily,
}

impl TimePolicy {
    /// 获取当前时间周期标识
    fn current_period(&self) -> String {
        let now = Local::now();
        match self {
            TimePolicy::Minutely => now.format("%Y-%m-%d-%H-%M").to_string(),
            TimePolicy::Hourly => now.format("%Y-%m-%d-%H").to_string(),
            TimePolicy::Daily => now.format("%Y-%m-%d").to_string(),
        }
    }

    /// 判断两个时间戳是否属于同一周期
    #[allow(dead_code)]
    fn is_same_period(&self, ts1: i64, ts2: i64) -> bool {
        // 将时间戳转换为 DateTime<Utc>，然后转为 Local
        let dt1 = DateTime::<Utc>::from_timestamp(ts1, 0)
            .map(|dt| dt.with_timezone(&Local))
            .unwrap();
        let dt2 = DateTime::<Utc>::from_timestamp(ts2, 0)
            .map(|dt| dt.with_timezone(&Local))
            .unwrap();

        match self {
            TimePolicy::Minutely => {
                dt1.year() == dt2.year()
                    && dt1.month() == dt2.month()
                    && dt1.day() == dt2.day()
                    && dt1.hour() == dt2.hour()
                    && dt1.minute() == dt2.minute()
            }
            TimePolicy::Hourly => {
                dt1.year() == dt2.year()
                    && dt1.month() == dt2.month()
                    && dt1.day() == dt2.day()
                    && dt1.hour() == dt2.hour()
            }
            TimePolicy::Daily => {
                dt1.year() == dt2.year() && dt1.month() == dt2.month() && dt1.day() == dt2.day()
            }
        }
    }
}

/// RollingFileAppender 配置
#[derive(Debug, Clone, serde::Deserialize, SmartDefault)]
#[serde(default)]
pub struct RollingFileAppenderConfig {
    // ========== 基本信息 ==========
    /// 日志文件路径
    #[default("app.log".to_string())]
    pub file_path: String,

    // ========== 切分策略 ==========
    /// 单个文件最大大小（字节），None 表示不按大小切分
    #[default(None)]
    pub max_size: Option<usize>,

    /// 时间切分策略，None 表示不按时间切分
    #[default(Some(TimePolicy::Hourly))]
    pub time_policy: Option<TimePolicy>,

    // ========== 清理策略 ==========
    /// 保留的最大文件数量，None 表示不按数量清理
    #[default(None)]
    pub max_files: Option<usize>,

    /// 最大保留时间（小时），None 表示不按时间清理
    #[default(None)]
    pub max_hours: Option<usize>,

    // ========== 其他选项 ==========
    /// 是否压缩旧日志文件
    #[default(false)]
    pub compress: bool,
}

/// 切分模式（内部使用）
#[derive(Debug, Clone)]
enum RollingMode {
    Both(TimePolicy, usize),  // 时间 + 大小
    TimeOnly(TimePolicy),     // 仅时间
    SizeOnly(usize),          // 仅大小
    None,                     // 不切分
}

/// 当前文件信息
struct CurrentFile {
    file: tokio::fs::File,
    path: PathBuf,
    size: usize,
    current_period: String,
    sequence: usize,
}

/// 滚动文件输出器
pub struct RollingFileAppender {
    config: RollingFileAppenderConfig,
    current_file: Arc<Mutex<CurrentFile>>,
    mode: RollingMode,
    base_path: PathBuf,
    file_prefix: String,
}

impl RollingFileAppender {
    /// 检查是否需要切分
    async fn should_rollover(&self, size: usize) -> bool {
        match &self.mode {
            RollingMode::Both(policy, max_size) => {
                let current = self.current_file.lock().await;
                let size_exceeded = size + current.size >= *max_size;
                let period_changed = self.is_period_changed(&current.current_period, policy).await;
                size_exceeded || period_changed
            }
            RollingMode::TimeOnly(policy) => {
                let current = self.current_file.lock().await;
                self.is_period_changed(&current.current_period, policy).await
            }
            RollingMode::SizeOnly(max_size) => {
                let current = self.current_file.lock().await;
                size + current.size >= *max_size
            }
            RollingMode::None => false,
        }
    }

    /// 检查时间周期是否变化
    async fn is_period_changed(&self, current_period: &str, policy: &TimePolicy) -> bool {
        let new_period = policy.current_period();
        new_period != *current_period
    }

    /// 生成新文件路径
    fn generate_file_path(&self, period: &str, sequence: usize) -> PathBuf {
        let filename = if period.is_empty() {
            // 纯大小切分: app.log.1, app.log.2
            if sequence == 0 {
                format!("{}.log", self.file_prefix)
            } else {
                format!("{}.log.{}", self.file_prefix, sequence)
            }
        } else {
            // 时间切分: app.2025-01-19-10.log
            if sequence == 0 {
                format!("{}.{}.log", self.file_prefix, period)
            } else {
                format!("{}.{}.log.{}", self.file_prefix, period, sequence)
            }
        };

        self.base_path.join(filename)
    }

    /// 执行切分
    async fn do_rollover(&self) -> Result<()> {
        let mut current = self.current_file.lock().await;

        // 刷新当前文件
        use tokio::io::AsyncWriteExt;
        current.file.flush().await?;

        // 确定新的周期和序号
        let (new_period, new_sequence, new_path) = match &self.mode {
            RollingMode::Both(policy, _) => {
                let new_period = policy.current_period();
                if new_period != current.current_period {
                    // 新的周期，序号重置为 0
                    let path = self.generate_file_path(&new_period, 0);
                    (new_period, 0, path)
                } else {
                    // 同一周期，序号递增
                    let new_seq = current.sequence + 1;
                    let path = self.generate_file_path(&current.current_period, new_seq);
                    (current.current_period.clone(), new_seq, path)
                }
            }
            RollingMode::TimeOnly(policy) => {
                let new_period = policy.current_period();
                let path = self.generate_file_path(&new_period, 0);
                (new_period, 0, path)
            }
            RollingMode::SizeOnly(_) => {
                // 纯大小切分需要重命名所有现有文件
                drop(current);
                self.increment_size_based_files().await?;
                let new_seq = 0;
                let path = self.generate_file_path("", new_seq);
                return self.open_new_file(path, String::new(), new_seq).await;
            }
            RollingMode::None => return Ok(()),
        };

        // 如果是纯大小切分，需要处理序号递增
        let final_sequence = if matches!(self.mode, RollingMode::SizeOnly(_)) {
            0 // 当前文件始终是 .0
        } else {
            new_sequence
        };

        let path_clone = new_path.clone();
        let period_clone = new_period.clone();
        drop(current);

        self.open_new_file(path_clone, period_clone, final_sequence).await?;

        // 执行清理
        self.cleanup_old_files().await?;

        Ok(())
    }

    /// 打开新文件并更新状态
    async fn open_new_file(&self, new_path: PathBuf, new_period: String, new_sequence: usize) -> Result<()> {
        let mut current = self.current_file.lock().await;

        // 打开新文件
        let new_file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_path)
            .await?;

        // 更新当前文件信息
        *current = CurrentFile {
            file: new_file,
            path: new_path,
            size: 0,
            current_period: new_period,
            sequence: new_sequence,
        };

        Ok(())
    }

    /// 纯大小切分时递增所有文件序号
    async fn increment_size_based_files(&self) -> Result<()> {
        // 查找所有相关文件
        let entries = self.find_log_files().await?;

        // 按序号分组
        let mut files: Vec<(usize, PathBuf)> = Vec::new();
        for entry in entries {
            if let Some(seq) = self.extract_sequence(&entry) {
                files.push((seq, entry));
            }
        }

        // 按序号降序排序，从最大的开始重命名
        files.sort_by(|a, b| b.0.cmp(&a.0));

        for (seq, old_path) in files {
            let new_seq = seq + 1;
            let new_path = self.generate_file_path("", new_seq);

            // 如果目标文件已存在，先删除
            if new_path.exists() {
                tokio::fs::remove_file(&new_path).await?;
            }

            tokio::fs::rename(&old_path, &new_path).await?;

            // 如果需要压缩
            if self.config.compress && new_seq > 0 {
                self.compress_file(&new_path).await?;
            }
        }

        Ok(())
    }

    /// 从文件路径中提取序号
    fn extract_sequence(&self, path: &Path) -> Option<usize> {
        let filename = path.file_name()?.to_str()?;
        let stem = filename.strip_suffix(".gz").unwrap_or(filename);

        // app.log.1 -> 1
        // app.log.2.gz -> 2
        if let Some(suffix) = stem.strip_prefix(&format!("{}.log.", self.file_prefix)) {
            suffix.parse().ok()
        } else if stem == format!("{}.log", self.file_prefix) {
            Some(0)
        } else {
            None
        }
    }

    /// 查找所有相关的日志文件
    async fn find_log_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with(&self.file_prefix) && filename.contains(".log") {
                    files.push(path);
                }
            }
        }

        Ok(files)
    }

    /// 压缩文件
    async fn compress_file(&self, path: &Path) -> Result<()> {
        let compressed_path = PathBuf::from(format!("{}.gz", path.display()));

        // 读取原始文件内容
        let content = tokio::fs::read(path).await?;

        // 使用 flate2 压缩
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&content)?;
        let compressed = encoder.finish()?;

        // 写入压缩文件
        tokio::fs::write(&compressed_path, compressed).await?;

        // 删除原始文件
        tokio::fs::remove_file(path).await?;

        Ok(())
    }

    /// 清理旧文件
    async fn cleanup_old_files(&self) -> Result<()> {
        // 根据配置自动判断清理策略
        let has_max_files = self.config.max_files.is_some();
        let has_max_hours = self.config.max_hours.is_some();

        match (has_max_files, has_max_hours) {
            (true, false) => self.cleanup_by_count().await?,
            (false, true) => self.cleanup_by_time().await?,
            (true, true) => {
                self.cleanup_by_count().await?;
                self.cleanup_by_time().await?;
            }
            (false, false) => {
                // 都不设置，不清理
            }
        }

        Ok(())
    }

    /// 按文件数量清理
    async fn cleanup_by_count(&self) -> Result<()> {
        let max_files = match self.config.max_files {
            Some(n) => n,
            None => return Ok(()),
        };

        let mut files = self.find_log_files().await?;

        // 按修改时间排序（最新的在前）
        files.sort_by_key(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::now())
        });
        files.reverse();

        // 排除当前文件
        let current_path = {
            let current = self.current_file.lock().await;
            current.path.clone()
        };

        let old_files: Vec<PathBuf> = files
            .into_iter()
            .filter(|p| p != &current_path)
            .collect();

        // 删除超出数量限制的文件
        let to_remove = old_files.iter().skip(max_files);
        for path in to_remove {
            tokio::fs::remove_file(path).await.ok();
        }

        Ok(())
    }

    /// 按时间清理
    async fn cleanup_by_time(&self) -> Result<()> {
        let max_hours = match self.config.max_hours {
            Some(n) => n,
            None => return Ok(()),
        };

        use std::time::{SystemTime, UNIX_EPOCH};

        let files = self.find_log_files().await?;
        let max_duration = std::time::Duration::from_secs(max_hours as u64 * 3600);
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .saturating_sub(max_duration);

        for path in files {
            if let Ok(metadata) = tokio::fs::metadata(&path).await {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                        if duration < cutoff {
                            tokio::fs::remove_file(&path).await.ok();
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// 同步构造方法
    pub fn new(config: RollingFileAppenderConfig) -> Self {
        use std::fs::OpenOptions;

        let path = PathBuf::from(&config.file_path);
        let base_path = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let file_prefix = file_name
            .strip_suffix(".log")
            .unwrap_or(&file_name)
            .to_string();

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        // 判断切分模式
        let mode = match (&config.time_policy, config.max_size) {
            (Some(policy), Some(size)) => RollingMode::Both(*policy, size),
            (Some(policy), None) => RollingMode::TimeOnly(*policy),
            (None, Some(size)) => RollingMode::SizeOnly(size),
            (None, None) => RollingMode::None,
        };

        let current_period = config
            .time_policy
            .map_or_else(|| String::new(), |p| p.current_period());

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .expect("Failed to open log file");

        let file_size = file.metadata().map(|m| m.len() as usize).unwrap_or(0);

        let current_file = CurrentFile {
            file: tokio::fs::File::from_std(file),
            path,
            size: file_size,
            current_period,
            sequence: 0,
        };

        Self {
            config,
            current_file: Arc::new(Mutex::new(current_file)),
            mode,
            base_path,
            file_prefix,
        }
    }
}

use std::io::Write;

#[async_trait::async_trait]
impl LogAppender for RollingFileAppender {
    async fn append(&self, formatted_message: &str) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let message_size = formatted_message.as_bytes().len() + 1; // +1 for newline

        // 检查是否需要切分
        if self.should_rollover(message_size).await {
            self.do_rollover().await?;
        }

        // 写入日志
        let mut current = self.current_file.lock().await;
        current.file.write_all(formatted_message.as_bytes()).await?;
        current.file.write_all(b"\n").await?;
        current.file.flush().await?;
        current.size += message_size;

        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let mut current = self.current_file.lock().await;
        current.file.flush().await?;
        Ok(())
    }
}

// 使用 new 方法（同步）实现 From trait
crate::impl_from!(RollingFileAppenderConfig => RollingFileAppender);
crate::impl_box_from!(RollingFileAppender => dyn LogAppender);

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_time_policy_current_period() {
        let policy = TimePolicy::Daily;
        let period = policy.current_period();
        assert!(period.len() > 0);
        assert!(period.contains('-'));
    }

    #[test]
    fn test_time_policy_same_period() {
        let policy = TimePolicy::Daily;
        let now = Local::now().timestamp();
        assert!(policy.is_same_period(now, now));

        let yesterday = now - 86400;
        assert!(!policy.is_same_period(now, yesterday));
    }

    #[test]
    fn test_config_default() {
        let config: RollingFileAppenderConfig = RollingFileAppenderConfig::default();
        assert_eq!(config.file_path, "app.log");
        assert_eq!(config.max_size, None);
        assert_eq!(config.max_files, None);
        assert!(config.compress == false);
        assert_eq!(config.time_policy, Some(TimePolicy::Hourly));
        assert_eq!(config.max_hours, None);
    }

    #[tokio::test]
    async fn test_rolling_file_appender_create() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let log_path = temp_dir.path().join("test.log");

        let config = RollingFileAppenderConfig {
            file_path: log_path.to_string_lossy().to_string(),
            max_size: Some(100),
            max_files: Some(5),
            compress: false,
            time_policy: None,
            ..Default::default()
        };

        let _appender = RollingFileAppender::new(config);
        assert!(log_path.exists());

        Ok(())
    }

    #[tokio::test]
    async fn test_rolling_file_appender_size_based() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let log_path = temp_dir.path().join("test.log");

        let config = RollingFileAppenderConfig {
            file_path: log_path.to_string_lossy().to_string(),
            max_size: Some(50), // 50 bytes
            max_files: Some(3),
            compress: false,
            time_policy: None,  // 不按时间切分
            ..Default::default()
        };

        let appender = RollingFileAppender::new(config);

        // 写入超过 50 字节的数据
        for i in 0..10 {
            appender.append(&format!("Test message number {}", i)).await?;
        }

        // 检查是否创建了多个文件
        let mut files = Vec::new();
        let mut entries = tokio::fs::read_dir(temp_dir.path()).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                if filename.starts_with("test.log") {
                    files.push(entry.path());
                }
            }
        }

        assert!(files.len() > 1, "Expected multiple log files, but found only {}", files.len());

        Ok(())
    }
}
