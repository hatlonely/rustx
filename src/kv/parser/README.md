# kv::parser - 通用数据解析器

提供三种数据格式的解析器：行分隔符、JSON、BSON。

## 快速开始

```rust
use rustx::kv::parser::register_parsers;
use rustx::cfg::{TypeOptions, create_trait_from_type_options};

// 1. 注册 Parser 类型
register_parsers::<String, serde_json::Value>()?;

// 2. 通过配置创建 Parser
let opts = TypeOptions::from_json(r#"{
    "type": "LineParser",
    "options": {
        "separator": "\t"
    }
}"#)?;

let parser: Box<dyn Parser<String, serde_json::Value>> =
    create_trait_from_type_options(&opts)?;

// 3. 解析数据
let (ct, key, value) = parser.parse(b"hello\tworld")?;
```

## Parser 配置选项

### LineParser - 分隔符行解析

**解析格式**: `key<separator>value<separator>changeType?`

```json
{
    "type": "LineParser",
    "options": {
        "separator": "\t"
    }
}
```

**字段说明**:
- `separator` (string, 可选): 字段分隔符，默认 `"\t"` (制表符)

**示例**:
```rust
// 使用逗号分隔
let opts = TypeOptions::from_json(r#"{
    "type": "LineParser",
    "options": {
        "separator": ","
    }
}"#)?;
```

### JsonParser - JSON 解析

```json
{
    "type": "JsonParser",
    "options": {
        "key_fields": ["id"],
        "key_separator": "_",
        "change_type_rules": []
    }
}
```

**字段说明**:
- `key_fields` (array<string>, 可选): 用于生成 key 的字段路径，默认 `["id"]`，支持嵌套如 `["user.id", "post.id"]`
- `key_separator` (string, 可选): key 字段间的分隔符，默认 `"_"`
- `change_type_rules` (array<object>, 可选): 变更类型规则列表，默认 `[]`

**基本示例**:
```rust
let opts = TypeOptions::from_json(r#"{
    "type": "JsonParser",
    "options": {
        "key_fields": ["id"],
        "key_separator": "_"
    }
}"#)?;

// JSON: {"id":"user123","name":"Alice"}
// 生成 key: "user123"
```

**多字段组合 key**:
```rust
let opts = TypeOptions::from_json(r#"{
    "type": "JsonParser",
    "options": {
        "key_fields": ["user.id", "post.id"],
        "key_separator": "_"
    }
}"#)?;

// JSON: {"user":{"id":"u1"},"post":{"id":"p1"}}
// 生成 key: "u1_p1"
```

**条件规则匹配**:
```rust
let opts = TypeOptions::from_json(r#"{
    "type": "JsonParser",
    "options": {
        "key_fields": ["id"],
        "key_separator": "_",
        "change_type_rules": [
            {
                "conditions": [
                    {
                        "field": "status",
                        "value": "deleted"
                    }
                ],
                "logic": "AND",
                "type": 3
            }
        ]
    }
}"#)?;

// 当 JSON 中 status="deleted" 时，返回 ChangeType::Delete (3)
```

**条件规则说明**:
- `conditions`: 条件列表
  - `field` (string): 字段路径，支持嵌套如 `"user.status"`
  - `value` (any): 期望值
- `logic` (string): 逻辑关系，`"AND"` 或 `"OR"`，默认 `"AND"`
- `type` (number): 变更类型，`1`=Add, `2`=Update, `3`=Delete

### BsonParser - BSON 解析

配置与 JsonParser 相同，支持相同的 `key_fields`、`key_separator` 和 `change_type_rules`。

```json
{
    "type": "BsonParser",
    "options": {
        "key_fields": ["id"],
        "key_separator": "_",
        "change_type_rules": []
    }
}
```

## 结构体支持

### JsonParser / BsonParser - 原生支持

```rust
#[derive(Deserialize)]
struct User {
    id: String,
    name: String,
    age: i32,
}

register_parsers::<String, User>()?;

let opts = TypeOptions::from_json(r#"{
    "type": "JsonParser",
    "options": {
        "key_fields": ["id"]
    }
}"#)?;

let parser: Box<dyn Parser<String, User>> = create_trait_from_type_options(&opts)?;
let (ct, key, user) = parser.parse(br#"{"id":"123","name":"Alice","age":30}"#)?;
```

### LineParser - 使用 serde_json::Value

```rust
register_parsers::<String, serde_json::Value>()?;

let parser: Box<dyn Parser<String, serde_json::Value>> =
    create_trait_from_type_options(&opts)?;

// 解析: "user123\t{\"name\":\"Alice\",\"age\":30}"
let (ct, key, value) = parser.parse(b"user123\t{\"name\":\"Alice\",\"age\":30}")?;
```

## ChangeType 说明

| 值 | 常量 | 说明 |
|------|------|------|
| 0 | `Unknown` | 未知 |
| 1 | `Add` | 新增 |
| 2 | `Update` | 更新 |
| 3 | `Delete` | 删除 |

