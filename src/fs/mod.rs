//! 文件系统操作模块
//!
//! 提供文件监听等功能

pub mod global;
pub mod watcher;

pub use global::{unwatch_all, watch};
pub use watcher::{FileEvent, FileWatcher};
