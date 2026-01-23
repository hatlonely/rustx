// Progress bar display for oss-tool

use indicatif::{ProgressBar, ProgressStyle};
use rustx::oss::{DirectoryProgressCallback, DirectoryTransferProgress};
use std::sync::Mutex;

/// Create a progress bar for directory transfer
pub fn create_directory_progress_bar(total_files: usize) -> ProgressBar {
    let pb = ProgressBar::new(total_files as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files ({bytes}/{total_bytes}) {msg}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pb
}

/// Progress callback wrapper for directory operations
pub struct DirectoryProgressBar {
    bar: ProgressBar,
}

/// Lazy progress callback that initializes on first progress call
pub struct LazyDirectoryProgressBar {
    inner: Mutex<Option<DirectoryProgressBar>>,
}

impl LazyDirectoryProgressBar {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    pub fn finish(&self) {
        let inner = self.inner.lock().unwrap();
        if let Some(ref bar) = *inner {
            bar.finish();
        }
    }
}

impl DirectoryProgressCallback for LazyDirectoryProgressBar {
    fn on_progress(&self, progress: &DirectoryTransferProgress) {
        let mut inner = self.inner.lock().unwrap();

        // Initialize on first call
        if inner.is_none() {
            *inner = Some(DirectoryProgressBar::new(
                progress.total_files,
                progress.total_bytes,
            ));
        }

        if let Some(ref bar) = *inner {
            bar.on_progress(progress);
        }
    }

    fn on_file_complete(&self, key: &str, success: bool, error_message: Option<&str>) {
        let inner = self.inner.lock().unwrap();
        if let Some(ref bar) = *inner {
            bar.on_file_complete(key, success, error_message);
        }
    }
}

impl DirectoryProgressBar {
    pub fn new(total_files: usize, _total_bytes: u64) -> Self {
        let bar = create_directory_progress_bar(total_files);
        bar.set_length(total_files as u64);
        Self { bar }
    }

    pub fn finish(&self) {
        self.bar.finish_with_message("done");
    }
}

impl DirectoryProgressCallback for DirectoryProgressBar {
    fn on_progress(&self, progress: &DirectoryTransferProgress) {
        self.bar.set_position(progress.completed_files as u64);
        self.bar.set_message(format!(
            "{} ({}/{})",
            progress.current_file,
            format_bytes(progress.transferred_bytes),
            format_bytes(progress.total_bytes)
        ));
    }

    fn on_file_complete(&self, key: &str, success: bool, error_message: Option<&str>) {
        if !success {
            if let Some(err) = error_message {
                self.bar.println(format!("Failed: {} - {}", key, err));
            } else {
                self.bar.println(format!("Failed: {}", key));
            }
        }
    }
}

/// Format bytes into human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format timestamp into human-readable string
pub fn format_timestamp(timestamp: &chrono::DateTime<chrono::Utc>) -> String {
    timestamp.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
        assert_eq!(format_bytes(1099511627776), "1.00 TB");
    }
}
