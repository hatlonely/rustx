# OSS 对象存储

统一的云对象存储抽象层，支持 AWS S3、阿里云 OSS 和 Google Cloud Storage。

## 快速开始

### 场景一：直接使用 ObjectStore 上传下载文件

```rust
use rustx::oss::{AwsS3ObjectStore, AwsS3ObjectStoreConfig, ObjectStore};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 从 JSON5 配置创建 S3 客户端
    let config_json = r#"
    {
      bucket: "my-bucket",
      region: "us-east-1"
    }
    "#;
    let config: AwsS3ObjectStoreConfig = json5::from_str(config_json)?;
    let store = AwsS3ObjectStore::new(config)?;

    // 上传文件
    store.put_object(
        "test/file.txt",
        Bytes::from("Hello, World!"),
        Default::default()
    ).await?;

    // 下载文件
    let data = store.get_object("test/file.txt", Default::default()).await?;
    println!("Downloaded: {}", String::from_utf8_lossy(&data));

    // 上传本地大文件（自动使用分片上传）
    store.put_file(
        "test/large.bin",
        std::path::Path::new("./local_file.bin"),
        Default::default()
    ).await?;

    Ok(())
}
```

### 场景二：使用 ObjectStoreManager 进行跨存储操作

```rust
use rustx::oss::{ObjectStoreManager, ObjectStoreManagerConfig, register_object_store};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 从 JSON5 配置创建管理器
    let config_json = r#"
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
    "#;
    let config: ObjectStoreManagerConfig = json5::from_str(config_json)?;
    let mut manager = ObjectStoreManager::new(config);

    // 上传本地文件到 S3
    manager.cp(
        "./local_data.csv",
        "s3://my-backup-bucket/2024/data.csv",
        Default::default()
    ).await?;

    // 列举 OSS 上的文件
    let objects = manager.ls(
        "oss://my-data-bucket/logs/",
        Default::default()
    ).await?;
    for obj in objects {
        println!("{}: {} bytes", obj.key, obj.size);
    }

    // 跨存储复制（S3 -> OSS）
    manager.cp(
        "s3://my-backup-bucket/file.txt",
        "oss://my-data-bucket/backup/file.txt",
        Default::default()
    ).await?;

    Ok(())
}
```

## 配置说明

### AwsS3ObjectStoreConfig - AWS S3 配置

```json5
{
  // 存储桶名称（必填）
  "bucket": "my-bucket",

  // AWS 区域
  // 也可通过环境变量 AWS_REGION 或 AWS_DEFAULT_REGION 设置
  "region": "us-east-1",

  // 自定义端点（可选）
  // 用于兼容 S3 的存储，如 MinIO
  // "endpoint": "http://localhost:9000",

  // 是否使用 path-style URL（可选）
  // 设置 endpoint 时通常需要设为 true
  // "force_path_style": true,

  // Access Key ID（可选）
  // 如不配置，会自动使用默认凭证链：
  //   - 环境变量 AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY
  //   - ~/.aws/credentials
  //   - ECS 容器凭证
  //   - EC2 实例元数据
  // "access_key_id": "AKIAIOSFODNN7EXAMPLE",

  // Secret Access Key（可选）
  // "secret_access_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
}
```

### AliOssObjectStoreConfig - 阿里云 OSS 配置

```json5
{
  // 存储桶名称（必填）
  "bucket": "my-bucket",

  // 区域端点（可选）
  // 如不配置，会自动从 ECS metadata 获取 region 并使用 internal endpoint
  // "endpoint": "oss-cn-hangzhou.aliyuncs.com",

  // Access Key ID（可选）
  // 如不配置，按以下顺序获取：
  //   1. 环境变量 ALIBABA_CLOUD_ACCESS_KEY_ID / ALIBABA_CLOUD_ACCESS_KEY_SECRET
  //      （兼容旧版 ALIYUN_OSS_ACCESS_KEY_ID / ALIYUN_OSS_ACCESS_KEY_SECRET）
  //   2. ECS 实例角色（通过元数据服务获取）
  // "access_key_id": "LTAI5txxxxxx",

  // Access Key Secret（可选）
  // "access_key_secret": "xxxxxxxxxx",

  // ECS 实例角色名称（可选）
  // 不配置时会自动从元数据服务获取
  // "ecs_ram_role": "my-ecs-role",

  // 是否使用 HTTPS（默认：true）
  // "https": true
}
```

### GcpGcsObjectStoreConfig - Google Cloud Storage 配置

```json5
{
  // 存储桶名称（必填）
  "bucket": "my-bucket",

  // 服务账号密钥文件路径（可选）
  // 如不配置，按以下顺序获取：
  //   1. 环境变量 GOOGLE_APPLICATION_CREDENTIALS 指向的文件
  //   2. ~/.config/gcloud/application_default_credentials.json
  //   3. GCE/GKE 实例元数据服务
  // "service_account_key_path": "/path/to/key.json",

  // 服务账号密钥 JSON 内容（可选，优先级高于文件路径）
  // "service_account_key_json": "{\"type\": \"service_account\", ...}",

  // 自定义端点（可选）
  // 用于本地模拟器如 fake-gcs-server
  // "endpoint": "http://localhost:4443"
}
```

### ObjectStoreManagerConfig - 管理器配置

```json5
{
  // 对象存储配置列表
  "stores": [
    {
      "type": "AwsS3ObjectStore",
      "options": {
        "bucket": "my-backup-bucket",
        "region": "us-east-1"
      }
    },
    {
      "type": "AliOssObjectStore",
      "options": {
        "bucket": "my-data-bucket",
        "endpoint": "oss-cn-hangzhou.aliyuncs.com"
      }
    },
    {
      "type": "GcpGcsObjectStore",
      "options": {
        "bucket": "my-archive-bucket"
      }
    }
  ],

  // 操作的默认选项
  "defaults": {
    // 并发操作数量（默认：4）
    "concurrency": 4,

    // 分片上传的分片大小，单位：字节（默认：8MB）
    "part_size": 8388608,

    // 启用分片上传的阈值，单位：字节（默认：100MB）
    "multipart_threshold": 104857600
  }
}
```

## 支持的操作

### ObjectStore Trait 接口

| 层级 | 方法 | 说明 |
|------|------|------|
| 基础 CRUD | `put_object` | 上传内存中的数据 |
| | `get_object` | 下载数据到内存 |
| | `delete_object` | 删除对象 |
| | `head_object` | 获取对象元数据 |
| | `list_objects` | 列举对象 |
| 流式接口 | `put_stream` | 流式上传（支持大文件分片） |
| | `get_stream` | 流式下载 |
| 文件操作 | `put_file` | 上传本地文件 |
| | `get_file` | 下载到本地文件 |
| 目录操作 | `put_directory` | 批量上传目录（支持并发、过滤、进度回调） |
| | `get_directory` | 批量下载目录 |

### ObjectStoreManager 操作

| 方法 | 说明 | 示例 |
|------|------|------|
| `cp` | 在本地和远程、或远程之间复制文件/目录 | `manager.cp("./file.txt", "s3://bucket/file.txt", opts).await?` |
| `ls` | 列举远程对象 | `manager.ls("s3://bucket/prefix/", opts).await?` |
| `rm` | 删除远程对象（支持递归） | `manager.rm("s3://bucket/file.txt", opts).await?` |
| `stat` | 获取对象元数据 | `manager.stat("s3://bucket/file.txt").await?` |

### URI 格式支持

- **S3**: `s3://bucket-name/key/path`
- **阿里云 OSS**: `oss://bucket-name/key/path`
- **Google Cloud Storage**: `gcs://bucket-name/key/path`
- **本地路径**: `./local/path` 或 `/absolute/path`
