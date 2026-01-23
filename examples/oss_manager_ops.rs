//! OSS 管理器跨存储操作示例
//!
//! 演示 ObjectStoreManager 的多存储配置和 URI 操作

use anyhow::Result;
use rustx::oss::{ ObjectStoreManager, ObjectStoreManagerConfig };

#[tokio::main]
async fn main() -> Result<()> {
    // 多存储配置
    let config: ObjectStoreManagerConfig = json5::from_str(r#"
    {
        stores: [
            {
                type: "AwsS3ObjectStore",
                options: {
                    bucket: "my-backup-bucket",
                    region: "us-east-1"
                }
            },
            {
                type: "AliOssObjectStore",
                options: {
                    bucket: "my-data-bucket",
                    endpoint: "oss-cn-hangzhou.aliyuncs.com"
                }
            }
        ]
    }
    "#)?;

    let mut manager = ObjectStoreManager::new(config);

    // 本地 -> 远程复制
    manager.cp(
        "./local_data.csv",
        "s3://my-backup-bucket/2024/data.csv",
        Default::default()
    ).await?;

    // 列举远程对象
    let objects = manager.ls(
        "oss://my-data-bucket/logs/",
        Default::default()
    ).await?;
    for obj in objects {
        println!("{}: {} bytes", obj.key, obj.size);
    }

    // 跨存储复制 (S3 -> OSS)
    manager.cp(
        "s3://my-backup-bucket/file.txt",
        "oss://my-data-bucket/backup/file.txt",
        Default::default()
    ).await?;

    // 获取对象元数据
    let meta = manager.stat("s3://my-backup-bucket/2024/data.csv").await?;
    println!("size: {}, last_modified: {:?}", meta.size, meta.last_modified);

    // 删除对象
    manager.rm(
        "s3://my-backup-bucket/temp.txt",
        Default::default()
    ).await?;

    Ok(())
}
