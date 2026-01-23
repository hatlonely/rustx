# oss

统一的对象存储命令行工具，支持 AWS S3、阿里云 OSS、Google GCS 等多种存储服务。

## 功能特性

- **统一接口**：一套命令操作多种对象存储服务
- **灵活复制**：支持上传、下载、跨存储复制
- **并发操作**：支持并发传输和分片上传
- **文件过滤**：支持 glob 模式的文件包含/排除
- **跨存储**：支持在不同存储服务间直接传输数据

## 编译安装

```bash
cargo build --release --bin oss
```

编译后的二进制文件位于 `target/release/oss`

## 配置

配置文件默认位置：`~/.oss/config.yaml`

可通过 `--config` 或 `-c` 参数指定配置文件路径。

### 配置格式

```yaml
# 对象存储配置列表
stores:
  # AWS S3 存储
  - type: AwsS3ObjectStore
    options:
      bucket: "my-bucket"
      region: "us-east-1"
      # access_key_id: "AKIA..."     # 可选，也可通过环境变量 AWS_ACCESS_KEY_ID 配置
      # secret_access_key: "..."     # 可选，也可通过环境变量 AWS_SECRET_ACCESS_KEY 配置
      # endpoint: "https://s3.amazonaws.com"  # 可选，自定义 endpoint

  # 阿里云 OSS 存储
  - type: AliOssObjectStore
    options:
      bucket: "my-oss-bucket"
      endpoint: "oss-cn-hangzhou.aliyuncs.com"
      # access_key_id: "LTAI..."     # 可选，也可通过环境变量 ALI_OSS_ACCESS_KEY_ID 配置
      # secret_access_key: "..."     # 可选，也可通过环境变量 ALI_OSS_SECRET_ACCESS_KEY 配置

  # Google GCS 存储
  - type: GcpGcsObjectStore
    options:
      bucket: "my-gcs-bucket"
      # service_account_path: "/path/to/key.json"  # 可选，也可通过环境变量 GCP_SERVICE_ACCOUNT_PATH 配置

# 默认操作选项
defaults:
  concurrency: 4                # 并发操作数量
  part_size: 8388608           # 分片大小（8MB）
  multipart_threshold: 104857600  # 分片上传阈值（100MB）
```

## 使用示例

### 上传文件

```bash
# 上传单个文件
oss cp ./local.txt s3://my-bucket/remote.txt

# 递归上传目录
oss cp -r ./data s3://my-bucket/backup/

# 上传并显示进度
oss cp --progress ./large-file.dat s3://my-bucket/files/

# 使用高并发上传
oss cp --concurrency 16 ./data s3://my-bucket/
```

### 下载文件

```bash
# 下载单个文件
oss cp s3://my-bucket/file.txt ./local.txt

# 递归下载目录
oss cp - s3://my-bucket/backup/ ./local-backup/
```

### 跨存储复制

```bash
# S3 到 OSS
oss cp s3://bucket-a/file.txt oss://bucket-b/file.txt

# 递归跨存储复制
oss cp -r s3://bucket-a/ oss://bucket-b/backup/
```

### 列举对象

```bash
# 列举目录内容
oss ls s3://my-bucket/path/

# 递归列举
oss ls -r s3://my-bucket/

# 详细格式
oss ls -l s3://my-bucket/path/

# 限制返回数量
oss ls --max-keys 100 s3://my-bucket/
```

### 删除对象

```bash
# 删除单个文件
oss rm s3://my-bucket/file.txt

# 递归删除目录
oss rm -r s3://my-bucket/path/

# 删除并过滤文件
oss rm -r --include "*.log" s3://my-bucket/logs/

# 强制删除（不确认）
oss rm -f s3://my-bucket/temp.txt
```

### 查看对象信息

```bash
# 显示文件元数据
oss stat s3://my-bucket/file.txt
```

## 命令说明

### cp - 复制文件

支持本地与远程、远程与远程之间的文件传输。

```
USAGE:
    oss cp [OPTIONS] <SOURCE> <DESTINATION>

ARGUMENTS:
    <SOURCE>        源路径（本地路径或远程 URI）
    <DESTINATION>   目标路径（本地路径或远程 URI）

OPTIONS:
    -r, --recursive                 递归复制目录
        --include <PATTERN>         仅包含匹配模式的文件
        --exclude <PATTERN>         排除匹配模式的文件
        --concurrency <NUM>         并发操作数 [default: 4]
        --part-size <SIZE>          分片大小（字节） [default: 8388608]
        --overwrite                 覆盖已存在的文件
        --progress                  显示进度条
```

### ls - 列举对象

列举指定前缀下的对象。

```
USAGE:
    oss ls [OPTIONS] <URI>

ARGUMENTS:
    <URI>    远程 URI（如 s3://bucket/prefix/）

OPTIONS:
    -r, --recursive           递归列举
    -l, --long                显示详细信息
    -H, --human-readable      人类可读的文件大小
        --max-keys <NUM>      限制返回数量
```

### rm - 删除对象

删除远程对象。

```
USAGE:
    oss rm [OPTIONS] <URI>

ARGUMENTS:
    <URI>    远程 URI（如 s3://bucket/path/to/key）

OPTIONS:
    -r, --recursive           递归删除
    -f, --force               强制删除，不确认
        --include <PATTERN>   仅删除匹配模式的文件
        --exclude <PATTERN>   排除匹配模式的文件
```

### stat - 显示对象信息

显示对象的元数据（大小、修改时间、ETag 等）。

```
USAGE:
    oss stat <URI>

ARGUMENTS:
    <URI>    远程 URI（如 s3://bucket/path/to/key）
```

## URI 格式

远程 URI 格式：`<provider>://<bucket>/<key>`

- `s3://` - AWS S3 兼容存储
- `oss://` - 阿里云 OSS
- `gcs://` - Google Cloud Storage

示例：
- `s3://my-bucket/path/to/file.txt`
- `oss://my-bucket/data/2024/report.csv`
- `gcs://my-bucket/logs/app.log`
