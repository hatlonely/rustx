use serde::Deserialize;
use std::marker::PhantomData;

use super::{ChangeType, ParseValue, Parser, ParserError};

/// LineParser 配置（遵循 cfg/README.md 最佳实践）
///
/// 配置行解析器的行为，定义分隔符等参数。
#[derive(Debug, Clone, Deserialize)]
pub struct LineParserConfig {
    /// 字段分隔符（默认：制表符）
    #[serde(default = "default_separator")]
    pub separator: String,
}

fn default_separator() -> String {
    "\t".to_string()
}

/// 行解析器（对应 Golang LineParser[K, V]）
///
/// 解析格式：`key<separator>value<separator>changeType?`
///
/// # 示例
/// ```ignore
/// use rustx::kv::parser::{LineParser, LineParserConfig, Parser};
///
/// let config = LineParserConfig {
///     separator: "\t".to_string(),
/// };
/// let parser = LineParser::<String, String>::new(config);
///
/// // 解析 "hello\tworld"
/// let (ct, key, value) = parser.parse(b"hello\tworld").unwrap();
/// assert_eq!(key, "hello");
/// assert_eq!(value, "world");
/// ```
pub struct LineParser<K, V> {
    separator: String,
    _phantom: PhantomData<(K, V)>,
}

impl<K, V> LineParser<K, V> {
    /// 唯一的构造方法（遵循 cfg/README.md 最佳实践）
    ///
    /// # 参数
    /// - config: 解析器配置
    pub fn new(config: LineParserConfig) -> Self {
        Self {
            separator: config.separator,
            _phantom: PhantomData,
        }
    }
}

// 实现 From trait（注册系统需要）
impl<K, V> From<LineParserConfig> for LineParser<K, V> {
    fn from(config: LineParserConfig) -> Self {
        Self::new(config)
    }
}

// 实现 From<Box<LineParser>> for Box<dyn Parser>（注册系统需要）
impl<K, V> From<Box<LineParser<K, V>>> for Box<dyn super::Parser<K, V>>
where
    K: ParseValue + Send + Sync + 'static,
    V: ParseValue + Send + Sync + 'static,
{
    fn from(source: Box<LineParser<K, V>>) -> Self {
        source as Box<dyn super::Parser<K, V>>
    }
}

impl<K, V> Parser<K, V> for LineParser<K, V>
where
    K: ParseValue + Send + Sync,
    V: ParseValue + Send + Sync,
{
    fn parse(&self, buf: &[u8]) -> Result<(ChangeType, K, V), ParserError> {
        let line = std::str::from_utf8(buf)
            .map_err(|e| ParserError::ParseFailed(format!("invalid UTF-8: {}", e)))?;

        let parts: Vec<&str> = line.splitn(3, &self.separator).collect();

        if parts.len() < 2 {
            return Ok((
                ChangeType::Unknown,
                ParseValue::parse_value("")?,
                ParseValue::parse_value("")?,
            ));
        }

        let key = K::parse_value(parts[0])
            .map_err(|e| ParserError::ParseFailed(format!("failed to parse key: {}", e)))?;

        let value = V::parse_value(parts[1])
            .map_err(|e| ParserError::ParseFailed(format!("failed to parse value: {}", e)))?;

        let change_type = if parts.len() >= 3 && !parts[2].is_empty() {
            parse_change_type(parts[2])?
        } else {
            ChangeType::Add
        };

        Ok((change_type, key, value))
    }
}

/// 解析变更类型
///
/// 支持两种格式：
/// - 数字：1=Add, 2=Update, 3=Delete
/// - 字符串：add/update/delete（不区分大小写）
fn parse_change_type(s: &str) -> Result<ChangeType, ParserError> {
    // 尝试解析为数字
    if let Ok(n) = s.parse::<i32>() {
        return Ok(match n {
            1 => ChangeType::Add,
            2 => ChangeType::Update,
            3 => ChangeType::Delete,
            _ => ChangeType::Unknown,
        });
    }

    // 尝试解析为字符串
    Ok(match s.to_lowercase().as_str() {
        "add" => ChangeType::Add,
        "update" => ChangeType::Update,
        "delete" => ChangeType::Delete,
        _ => ChangeType::Unknown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let config = LineParserConfig {
            separator: "\t".to_string(),
        };
        let parser = LineParser::<String, String>::new(config);

        let (ct, key, value) = parser.parse(b"hello\tworld").unwrap();
        assert_eq!(ct, ChangeType::Add);
        assert_eq!(key, "hello");
        assert_eq!(value, "world");
    }

    #[test]
    fn test_parse_with_change_type() {
        let config = LineParserConfig {
            separator: "\t".to_string(),
        };
        let parser = LineParser::<String, String>::new(config);

        // 测试数字变更类型
        let (ct, _, _) = parser.parse(b"key1\tvalue1\t2").unwrap();
        assert_eq!(ct, ChangeType::Update);

        // 测试字符串变更类型
        let (ct, _, _) = parser.parse(b"key2\tvalue2\tdelete").unwrap();
        assert_eq!(ct, ChangeType::Delete);
    }

    #[test]
    fn test_parse_custom_separator() {
        let config = LineParserConfig {
            separator: ",".to_string(),
        };
        let parser = LineParser::<String, i32>::new(config);

        let (ct, key, value) = parser.parse(b"count,42").unwrap();
        assert_eq!(ct, ChangeType::Add);
        assert_eq!(key, "count");
        assert_eq!(value, 42);
    }

    #[test]
    fn test_parse_insufficient_fields() {
        let config = LineParserConfig {
            separator: "\t".to_string(),
        };
        let parser = LineParser::<String, String>::new(config);

        // 只有 key
        let (ct, key, value) = parser.parse(b"only_key").unwrap();
        assert_eq!(ct, ChangeType::Unknown);
        assert_eq!(key, "");
        assert_eq!(value, "");
    }

    #[test]
    fn test_parse_change_type_number() {
        assert_eq!(parse_change_type("1").unwrap(), ChangeType::Add);
        assert_eq!(parse_change_type("2").unwrap(), ChangeType::Update);
        assert_eq!(parse_change_type("3").unwrap(), ChangeType::Delete);
        assert_eq!(parse_change_type("0").unwrap(), ChangeType::Unknown);
        assert_eq!(parse_change_type("99").unwrap(), ChangeType::Unknown);
    }

    #[test]
    fn test_parse_change_type_string() {
        assert_eq!(parse_change_type("add").unwrap(), ChangeType::Add);
        assert_eq!(parse_change_type("ADD").unwrap(), ChangeType::Add);
        assert_eq!(parse_change_type("update").unwrap(), ChangeType::Update);
        assert_eq!(parse_change_type("UPDATE").unwrap(), ChangeType::Update);
        assert_eq!(parse_change_type("delete").unwrap(), ChangeType::Delete);
        assert_eq!(parse_change_type("DELETE").unwrap(), ChangeType::Delete);
        assert_eq!(parse_change_type("invalid").unwrap(), ChangeType::Unknown);
    }

    #[test]
    fn test_line_parser_config_default() {
        // 测试 serde 默认值
        let json = r#"{}"#;
        let config: LineParserConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.separator, "\t");
    }

    #[test]
    fn test_line_parser_from_config() {
        let config = LineParserConfig {
            separator: "|".to_string(),
        };

        let parser: LineParser<String, i32> = LineParser::new(config.clone());
        let parser_from: LineParser<String, i32> = LineParser::from(config);

        // 两种方式创建的 parser 应该功能相同
        let (ct1, k1, v1) = parser.parse(b"test|123").unwrap();
        let (ct2, k2, v2) = parser_from.parse(b"test|123").unwrap();

        assert_eq!(ct1, ct2);
        assert_eq!(k1, k2);
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_parse_with_json_value() {
        // LineParser 可以将 value 解析为 serde_json::Value
        let config = LineParserConfig {
            separator: "\t".to_string(),
        };
        let parser = LineParser::<String, serde_json::Value>::new(config);

        let line = r#"user123	{"name":"Alice","age":30}"#;
        let (ct, key, value) = parser.parse(line.as_bytes()).unwrap();

        assert_eq!(ct, ChangeType::Add);
        assert_eq!(key, "user123");
        assert_eq!(value["name"], "Alice");
        assert_eq!(value["age"], 30);
    }

    #[test]
    fn test_parse_with_custom_struct() {
        use serde::Deserialize;
        use rustx_macros::ParseValue;

        // 使用派生宏自动实现 ParseValue trait
        #[derive(Debug, Deserialize, PartialEq, ParseValue)]
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

        assert_eq!(ct, ChangeType::Add);
        assert_eq!(key, "user123");
        assert_eq!(
            value,
            User {
                name: "Alice".to_string(),
                age: 30
            }
        );
    }
}
