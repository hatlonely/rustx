use anyhow::Result;
use std::hash::Hash;

use crate::cfg::register_trait;

use super::{
    Parser, LineParser, LineParserConfig, JsonParser, JsonParserConfig, BsonParser,
    BsonParserConfig, ParseValue,
};

// 实现 From<Box<Parser>> for Box<dyn Parser>（注册系统需要）
impl<K, V> From<Box<LineParser<K, V>>> for Box<dyn Parser<K, V>>
where
    K: ParseValue + Send + Sync + 'static,
    V: ParseValue + Send + Sync + 'static,
{
    fn from(source: Box<LineParser<K, V>>) -> Self {
        source as Box<dyn Parser<K, V>>
    }
}

impl<K, V> From<Box<JsonParser<K, V>>> for Box<dyn Parser<K, V>>
where
    K: ParseValue + Send + Sync + 'static,
    V: for<'de> serde::Deserialize<'de> + Send + Sync + 'static,
{
    fn from(source: Box<JsonParser<K, V>>) -> Self {
        source as Box<dyn Parser<K, V>>
    }
}

impl<K, V> From<Box<BsonParser<K, V>>> for Box<dyn Parser<K, V>>
where
    K: ParseValue + Send + Sync + 'static,
    V: for<'de> serde::Deserialize<'de> + Send + Sync + 'static,
{
    fn from(source: Box<BsonParser<K, V>>) -> Self {
        source as Box<dyn Parser<K, V>>
    }
}

/// 注册所有基础 Parser 实现
///
/// 为指定的 K, V 类型组合注册所有可用的 Parser 实现。
/// 由于 `Parser<K, V>` 是泛型 trait，不同的 K, V 组合会产生不同的 TypeId，
/// 因此可以使用相同的类型名称注册，不会冲突。
///
/// # 类型参数
/// - `K`: 键类型，需要满足 `Clone + Send + Sync + Eq + Hash + 'static`
/// - `V`: 值类型，需要满足 `Clone + Send + Sync + 'static`
///
/// # 注册的类型
/// - `LineParser` - 分隔符行解析器
/// - `JsonParser` - JSON 解析器
/// - `BsonParser` - BSON 解析器
///
/// # 示例
/// ```ignore
/// use rustx::kv::parser::{register_parsers, Parser};
/// use rustx::cfg::{TypeOptions, create_trait_from_type_options};
///
/// // 注册 String -> String 类型的 Parser
/// register_parsers::<String, String>()?;
///
/// // 通过配置创建实例
/// let opts = TypeOptions::from_json(r#"{
///     "type": "LineParser",
///     "options": {
///         "separator": "\t"
///     }
/// }"#)?;
///
/// let parser: Box<dyn Parser<String, String>> = create_trait_from_type_options(&opts)?;
/// ```
pub fn register_parsers<K, V>() -> Result<()>
where
    K: Clone + Send + Sync + Eq + Hash + ParseValue + 'static,
    V: Clone + Send + Sync + ParseValue + for<'de> serde::Deserialize<'de> + 'static,
{
    register_trait::<LineParser<K, V>, dyn Parser<K, V>, LineParserConfig>("LineParser")?;
    register_trait::<JsonParser<K, V>, dyn Parser<K, V>, JsonParserConfig>("JsonParser")?;
    register_trait::<BsonParser<K, V>, dyn Parser<K, V>, BsonParserConfig>("BsonParser")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{create_trait_from_type_options, TypeOptions};

    #[test]
    fn test_register_parsers_line_parser() -> Result<()> {
        register_parsers::<String, String>()?;

        let opts = TypeOptions::from_json(
            r#"{
            "type": "LineParser",
            "options": {
                "separator": "\t"
            }
        }"#,
        )?;

        let parser: Box<dyn Parser<String, String>> = create_trait_from_type_options(&opts)?;

        // 验证 parser 可以正常使用
        let (ct, key, value) = parser.parse(b"hello\tworld").unwrap();
        assert_eq!(key, "hello");
        assert_eq!(value, "world");
        assert_eq!(ct, crate::kv::parser::ChangeType::Add);

        Ok(())
    }

    #[test]
    fn test_register_parsers_json_parser() -> Result<()> {
        register_parsers::<String, serde_json::Value>()?;

        let opts = TypeOptions::from_json(
            r#"{
            "type": "JsonParser",
            "options": {
                "key_fields": ["id"],
                "key_separator": "_"
            }
        }"#,
        )?;

        let parser: Box<dyn Parser<String, serde_json::Value>> =
            create_trait_from_type_options(&opts)?;

        // 验证 parser 可以正常使用
        let (ct, key, value) = parser.parse(r#"{"id":"user123","name":"Alice"}"#.as_bytes())?;
        assert_eq!(key, "user123");
        assert_eq!(value["id"], "user123");
        assert_eq!(value["name"], "Alice");
        assert_eq!(ct, crate::kv::parser::ChangeType::Add);

        Ok(())
    }

    #[test]
    fn test_register_multiple_type_combinations() -> Result<()> {
        // 注册多种类型组合
        register_parsers::<String, String>()?;
        register_parsers::<String, i64>()?;
        register_parsers::<i32, String>()?;

        // 验证各类型组合都能正常工作
        let opts_str_str =
            TypeOptions::from_json(r#"{"type": "LineParser", "options": {}}"#)?;
        let opts_str_i64 =
            TypeOptions::from_json(r#"{"type": "LineParser", "options": {}}"#)?;

        let _parser1: Box<dyn Parser<String, String>> =
            create_trait_from_type_options(&opts_str_str)?;
        let _parser2: Box<dyn Parser<String, i64>> =
            create_trait_from_type_options(&opts_str_i64)?;

        Ok(())
    }

    #[test]
    fn test_register_parsers_custom_separator() -> Result<()> {
        register_parsers::<String, i32>()?;

        let opts = TypeOptions::from_json(
            r#"{
            "type": "LineParser",
            "options": {
                "separator": ","
            }
        }"#,
        )?;

        let parser: Box<dyn Parser<String, i32>> = create_trait_from_type_options(&opts)?;

        // 测试自定义分隔符
        let (ct, key, value) = parser.parse(b"count,42").unwrap();
        assert_eq!(key, "count");
        assert_eq!(value, 42);
        assert_eq!(ct, crate::kv::parser::ChangeType::Add);

        Ok(())
    }

    #[test]
    fn test_register_parsers_json_with_change_type_rules() -> Result<()> {
        register_parsers::<String, serde_json::Value>()?;

        let opts = TypeOptions::from_json(
            r#"{
            "type": "JsonParser",
            "options": {
                "key_fields": ["id"],
                "key_separator": "_",
                "change_type_rules": [
                    {
                        "conditions": [
                            {
                                "field": "operation",
                                "value": "delete"
                            }
                        ],
                        "logic": "AND",
                        "type": 3
                    }
                ]
            }
        }"#,
        )?;

        let parser: Box<dyn Parser<String, serde_json::Value>> =
            create_trait_from_type_options(&opts)?;

        // 测试变更类型规则
        let (ct, key, _) = parser
            .parse(r#"{"id":"user123","operation":"delete"}"#.as_bytes())
            .unwrap();
        assert_eq!(key, "user123");
        // 注意：规则匹配可能失败，默认返回 Add
        // 这可能是因为值类型不匹配或其他原因
        if ct != crate::kv::parser::ChangeType::Delete {
            eprintln!("Warning: Expected Delete but got {:?}. Rule matching may have failed.", ct);
        }
        assert_eq!(ct, crate::kv::parser::ChangeType::Delete);

        Ok(())
    }
}
