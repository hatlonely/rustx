# kv::parser - 通用数据解析器

提供三种数据格式的解析器：行分隔符、JSON、BSON。

## 快速开始

```rust
use rustx::kv::parser::register_parsers;
use rustx::cfg::{TypeOptions, create_trait_from_type_options};
use rustx_macros::ParseValue;

#[derive(Debug, Deserialize, ParseValue)]
struct User {
    name: String,
    age: i32,
}

// 1. 注册 Parser 类型
register_parsers::<String, User>()?;

// 2. 通过配置创建 Parser
let opts = TypeOptions::from_json(r#"{
    "type": "LineParser",
    "options": {
        "separator": "\t"
    }
}"#)?;

let parser: Box<dyn Parser<String, serde_json::Value>> = create_trait_from_type_options(&opts)?;

// 3. 解析数据
let (ct, key, value) = parser.parse(b"hello\tworld")?;
```

## Parser 配置选项

### LineParser - 分隔符行解析

**解析格式**: `key<separator>value<separator>changeType?`

```json5
{
    // Parser 类型，固定为 "LineParser"
    "type": "LineParser",
    "options": {
        // 字段分隔符，可选，默认 "\t" (制表符)
        "separator": "\t"
    }
}
```

### JsonParser - JSON 解析

```json5
{
    // Parser 类型，固定为 "JsonParser"
    "type": "JsonParser",
    "options": {
        // 用于生成 key 的字段路径列表，可选，默认 ["id"]
        // 支持嵌套字段，如 ["user.id", "post.id"]
        "key_fields": ["id"],

        // key 字段间的分隔符，可选，默认 "_"
        "key_separator": "_",

        // 变更类型规则列表，可选，默认 []
        "change_type_rules": [
            {
                // 条件列表
                "conditions": [
                    {
                        // 字段路径，支持嵌套如 "user.status"
                        "field": "status",
                        // 期望值
                        "value": "deleted"
                    }
                ],
                // 逻辑关系，"AND" 或 "OR"，默认 "AND"
                "logic": "AND",
                // 变更类型：1=Add, 2=Update, 3=Delete
                "type": 3
            }
        ]
    }
}
```

### BsonParser - BSON 解析

配置与 JsonParser 相同，支持相同的 `key_fields`、`key_separator` 和 `change_type_rules`。

```json5
{
    // Parser 类型，固定为 "BsonParser"
    "type": "BsonParser",
    "options": {
        // 用于生成 key 的字段路径列表，可选，默认 ["id"]
        // 支持嵌套字段，如 ["user.id", "post.id"]
        "key_fields": ["id"],

        // key 字段间的分隔符，可选，默认 "_"
        "key_separator": "_",

        // 变更类型规则列表，可选，默认 []
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
```

### LineParser - 自定义结构体支持

使用 `#[derive(ParseValue)]` 派生宏，可以让 LineParser 直接解析为自定义结构体：

```rust
use serde::Deserialize;
use rustx_macros::ParseValue;

#[derive(Debug, Deserialize, ParseValue)]
struct User {
    name: String,
    age: i32,
}
```

## ChangeType 说明

| 值 | 常量 | 说明 |
|------|------|------|
| 0 | `Unknown` | 未知 |
| 1 | `Add` | 新增 |
| 2 | `Update` | 更新 |
| 3 | `Delete` | 删除 |

