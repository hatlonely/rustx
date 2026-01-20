//! FileWatcher 使用示例
//!
//! 演示如何使用全局 watch() 函数监听文件变化

use rustx::fs::{watch, FileEvent};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建临时文件用于演示
    let temp_file = PathBuf::from("/tmp/file_watcher_example.txt");
    fs::write(&temp_file, "初始内容")?;

    println!("🚀 FileWatcher 使用示例\n");
    println!("监听文件: {:?}", temp_file);
    println!("\n请尝试修改或删除该文件，按 Ctrl+C 退出...\n");

    // 使用全局 watch() 函数监听文件
    watch(&temp_file, |event| {
        match event {
            FileEvent::Created(path) => {
                println!("✅ 文件创建: {:?}", path);
            }
            FileEvent::Modified(path) => {
                println!("🔄 文件修改: {:?}", path);
                // 尝试读取并显示文件内容
                if let Ok(content) = fs::read_to_string(&path) {
                    println!("   新内容: {}", content);
                }
            }
            FileEvent::Deleted(path) => {
                println!("🗑️  文件删除: {:?}", path);
            }
            FileEvent::Error(err) => {
                println!("❌ 发生错误: {}", err);
            }
        }
    })?;

    println!("开始监听...\n");

    // 保持程序运行
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
