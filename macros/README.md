# rustx-macros

RustX 的派生宏库，为自定义类型提供自动实现 `ParseValue` trait 的能力。

## ParseValue 派生宏

`#[derive(ParseValue)]` 宏可以为实现了 `Deserialize` 的结构体自动生成 `ParseValue` trait 的实现。

### 使用示例

```rust
use serde::Deserialize;
use rustx_macros::ParseValue;

#[derive(Debug, Deserialize, ParseValue)]
struct User {
    name: String,
    age: i32,
    email: Option<String>,
}
```

这会自动生成如下代码：

```rust
impl ParseValue for User {
    fn parse_value(s: &str) -> Result<Self, ParserError> {
        serde_json::from_str(s).map_err(|e| {
            ParserError::ParseFailed(
                format!("failed to parse User: {}", e)
            )
        })
    }
}
```

### 在 LineParser 中使用

```rust
use rustx::kv::parser::{LineParser, LineParserConfig, Parser};

#[derive(Debug, Deserialize, ParseValue)]
struct User {
    name: String,
    age: i32,
}

let config = LineParserConfig {
    separator: "\t".to_string(),
};
let parser = LineParser::<String, User>::new(config);

let line = r#"user123	{"name":"Alice","age":30}"#;
let (ct, key, value) = parser.parse(line.as_bytes()).unwrap();
```

### 泛型支持

宏也支持泛型结构体：

```rust
#[derive(Debug, Deserialize, ParseValue)]
struct Container<T: Deserialize> {
    data: T,
    timestamp: i64,
}
```

## 实现细节

- 宏使用 `serde_json::from_str` 进行 JSON 反序列化
- 错误信息包含类型名称，便于调试
- 使用 `crate` 相对路径，确保在任何上下文中都能正确引用

## 注意事项

1. 结构体必须实现 `Deserialize` trait
2. 解析时使用 JSON 格式
3. 对于基本类型（如 `String`, `i32` 等），已经有内置实现，不需要使用派生宏
