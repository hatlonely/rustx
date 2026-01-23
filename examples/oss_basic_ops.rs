//! OSS 基础操作示例
//!
//! 演示 ObjectStore trait 的基础操作：put/get/delete/head/list

use anyhow::Result;
use bytes::Bytes;
use rustx::oss::{ AwsS3ObjectStore, AwsS3ObjectStoreConfig, ObjectStore };

#[tokio::main]
async fn main() -> Result<()> {
    // 从配置创建 S3 客户端
    let config: AwsS3ObjectStoreConfig = json5::from_str(r#"
    {
        bucket: "my-bucket",
        region: "us-east-1"
    }
    "#)?;

    let store = AwsS3ObjectStore::new(config)?;

    // 上传对象
    store.put_object(
        "test/basic/file.txt",
        Bytes::from("Hello, World!"),
        Default::default()
    ).await?;

    // 获取对象
    let data = store.get_object("test/basic/file.txt", Default::default()).await?;
    assert_eq!(data, Bytes::from("Hello, World!"));

    // 获取对象元数据
    let meta = store.head_object("test/basic/file.txt").await?.unwrap();
    println!("size: {}, etag: {:?}", meta.size, meta.etag);

    // 列举对象
    let objects = store.list_objects(Some("test/basic/"), None).await?;
    for obj in objects {
        println!("key: {}, size: {}", obj.key, obj.size);
    }

    // 删除对象
    store.delete_object("test/basic/file.txt").await?;

    Ok(())
}
