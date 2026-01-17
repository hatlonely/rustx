# kv::serializer - 通用数据序列化器

提供四种数据格式的序列化器：JSON、MessagePack、BSON、Protobuf。

## 快速开始

```rust
use rustx::kv::serializer::register_serializers;
use rustx::cfg::{TypeOptions, create_trait_from_type_options};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    name: String,
    age: i32,
}

// 1. 注册 Serializer 类型
register_serializers::<User>()?;

// 2. 通过配置创建 Serializer
let opts = TypeOptions::from_json(r#"{
    "type": "JsonSerializer",
    "options": {
        "pretty": true
    }
}"#)?;

let serializer: Box<dyn Serializer<User, Vec<u8>>> = create_trait_from_type_options(&opts)?;

// 3. 序列化和反序列化
let user = User { name: "Alice".to_string(), age: 30 };
let bytes = serializer.serialize(user).await?;
let restored_user = serializer.deserialize(bytes).await?;
```

## Serializer 配置选项

### JsonSerializer - JSON 序列化

支持将任意实现了 `Serialize + Deserialize` 的类型序列化为 JSON 字节数组。

```json5
{
    // Serializer 类型，固定为 "JsonSerializer"
    "type": "JsonSerializer",
    "options": {
        // 是否格式化输出（美化 JSON），可选，默认 false
        "pretty": false
    }
}
```

### MsgPackSerializer - MessagePack 序列化

支持将任意实现了 `Serialize + Deserialize` 的类型序列化为 MessagePack 二进制格式。

```json5
{
    // Serializer 类型，固定为 "MsgPackSerializer"
    "type": "MsgPackSerializer",
    "options": {
        // 是否使用命名字段（struct 字段名），可选，默认 true
        // true: 使用字段名序列化（推荐，更易调试）
        // false: 使用字段索引序列化（体积更小）
        "named": true
    }
}
```

### BsonSerializer - BSON 序列化

支持将任意实现了 `Serialize + Deserialize` 的类型序列化为 BSON 二进制格式（MongoDB 使用的格式）。

```json5
{
    // Serializer 类型，固定为 "BsonSerializer"
    "type": "BsonSerializer",
    "options": {
        // BSON 序列化器暂无配置选项
    }
}
```

### ProtobufSerializer - Protobuf 序列化

支持将实现了 `prost::Message + Default` 的类型序列化为 Protobuf 二进制格式。

```json5
{
    // Serializer 类型，固定为 "ProtobufSerializer"
    "type": "ProtobufSerializer",
    "options": {
        // Protobuf 序列化器暂无配置选项
    }
}
```

## 结构体支持

### JsonSerializer / MsgPackSerializer / BsonSerializer - Serde 支持

这些序列化器要求类型实现 Serde 的 `Serialize` 和 `Deserialize` traits：

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct User {
    name: String,
    age: i32,
}

// 使用 register_serde_serializers 注册这三种序列化器
register_serde_serializers::<User>()?;
```

### ProtobufSerializer - Prost 支持

Protobuf 序列化器要求类型实现 `prost::Message + Default` traits，通常由 `.proto` 文件编译生成：

```rust
// 由 .proto 文件自动生成，自动实现 prost::Message + Default
#[derive(Debug, PartialEq, Message)]
struct User {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(int32, tag = "2")]
    age: i32,
}

// 使用 register_protobuf_serializers 注册
register_protobuf_serializers::<User>()?;
```

### 同时支持所有序列化器

如果类型同时实现了 Serde 和 Prost traits，可以使用 `register_serializers` 一次性注册所有四种序列化器：

```rust
use serde::{Deserialize, Serialize};
use prost::Message;

#[derive(Debug, Serialize, Deserialize, Message, PartialEq)]
struct User {
    #[prost(string, tag = "1")]
    name: String,
    #[serde(rename = "name")]
    #[prost(int32, tag = "2")]
    age: i32,
}

// 一次性注册所有四种序列化器
register_serializers::<User>()?;
```

## 注册函数说明

| 函数 | 支持的序列化器 | Trait 要求 |
|------|--------------|-----------|
| `register_serde_serializers<T>()` | JsonSerializer, MsgPackSerializer, BsonSerializer | `Serialize + Deserialize` |
| `register_protobuf_serializers<T>()` | ProtobufSerializer | `prost::Message + Default` |
| `register_serializers<T>()` | 所有四种序列化器 | 同时满足上述两个 trait bounds |
