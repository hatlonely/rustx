//! OSS 流式/文件/目录操作示例
//!
//! 演示流式上传下载、文件操作、目录批量操作

use anyhow::Result;
use rustx::oss::{ AliOssObjectStore, AliOssObjectStoreConfig, ObjectStore };
use std::path::Path;

#[tokio::main]
async fn main() -> Result<()> {
    let config: AliOssObjectStoreConfig = json5::from_str(r#"
    {
        bucket: "my-bucket",
        endpoint: "oss-cn-hangzhou.aliyuncs.com"
    }
    "#)?;

    let store = AliOssObjectStore::new(config)?;

    // 上传本地文件
    store.put_file(
        "test/upload/data.bin",
        Path::new("./local_file.bin"),
        Default::default()
    ).await?;

    // 下载到本地文件
    store.get_file(
        "test/upload/data.bin",
        Path::new("./downloaded.bin"),
        Default::default()
    ).await?;

    // 上传目录（并发、过滤、进度回调）
    store.put_directory(
        "test/backup/",
        Path::new("./data_dir"),
        Default::default()
    ).await?;

    // 下载目录
    store.get_directory(
        "test/backup/",
        Path::new("./restored_dir"),
        Default::default()
    ).await?;

    Ok(())
}
