//! 文件系统操作模块
//!
//! 提供文件监听等功能

pub mod file_watcher;
pub mod global_file_watcher;

pub use file_watcher::{FileEvent, FileWatcher};
pub use global_file_watcher::{unwatch_all, watch};
