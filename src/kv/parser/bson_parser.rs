use bson::Document;
use serde::Deserialize;
use std::marker::PhantomData;

use super::{Parser, ParserError, ChangeType, ParseValue};
use super::json_parser::{Condition, ChangeTypeRule};

#[cfg(test)]
use serde_json::json;

/// BsonParser 配置（遵循 cfg/README.md 最佳实践）
///
/// JsonParser 中的 Condition 和 ChangeTypeRule 被复用。
#[derive(Debug, Clone, Deserialize)]
pub struct BsonParserConfig {
    /// 用于生成 key 的字段路径列表（默认：["id"]）
    #[serde(default = "default_key_fields")]
    pub key_fields: Vec<String>,

    /// key 字段间的分隔符（默认："_"）
    #[serde(default = "default_key_separator")]
    pub key_separator: String,

    /// 变更类型规则列表（按顺序匹配）
    #[serde(default)]
    pub change_type_rules: Vec<ChangeTypeRule>,
}

fn default_key_fields() -> Vec<String> {
    vec!["id".to_string()]
}

fn default_key_separator() -> String {
    "_".to_string()
}

/// BSON 解析器（对应 Golang BsonParser[K, V]）
///
/// 从 BSON 数据中解析键值对，支持：
/// - 多字段组合生成 key
/// - 条件规则匹配变更类型
/// - 嵌套字段访问（如 "user.id"）
///
/// # 示例
/// ```ignore
/// use rustx::kv::parser::{BsonParser, BsonParserConfig, Parser};
/// use bson::doc;
///
/// let config = BsonParserConfig {
///     key_fields: vec!["user.id".to_string(), "post.id".to_string()],
///     key_separator: "_".to_string(),
///     change_type_rules: vec![],
/// };
/// let parser = BsonParser::<String, bson::Document>::new(config);
///
/// let doc = doc! {
///     "user": { "id": "u1" },
///     "post": { "id": "p1" },
///     "title": "hello"
/// };
/// let (ct, key, value) = parser.parse(&doc.to_bytes()?).unwrap();
/// assert_eq!(key, "u1_p1");
/// ```
pub struct BsonParser<K, V> {
    key_fields: Vec<String>,
    key_separator: String,
    change_type_rules: Vec<ChangeTypeRule>,
    _phantom: PhantomData<(K, V)>,
}

impl<K, V> BsonParser<K, V> {
    /// 唯一的构造方法（遵循 cfg/README.md 最佳实践）
    pub fn new(config: BsonParserConfig) -> Self {
        // 规范化逻辑操作符为大写
        let rules = config
            .change_type_rules
            .into_iter()
            .map(|mut rule| {
                rule.logic = rule.logic.to_uppercase();
                rule
            })
            .collect();

        Self {
            key_fields: config.key_fields,
            key_separator: config.key_separator,
            change_type_rules: rules,
            _phantom: PhantomData,
        }
    }

    /// 从 BSON 文档中提取指定路径的字段值
    ///
    /// 支持嵌套路径，如 "user.id" 或 "metadata.timestamp"
    fn get_bson_field_value(doc: &Document, field_path: &str) -> Option<bson::Bson> {
        if field_path.is_empty() {
            return None;
        }

        let parts: Vec<&str> = field_path.split('.').collect();
        let mut current_doc = doc;

        for (i, part) in parts.iter().enumerate() {
            match current_doc.get(part) {
                Some(value) => {
                    if i == parts.len() - 1 {
                        return Some(value.clone());
                    }
                    // 继续向下遍历，检查是否是 Document 类型
                    if let Some(next_doc) = value.as_document() {
                        current_doc = next_doc;
                    } else {
                        return None;
                    }
                }
                None => return None,
            }
        }

        None
    }

    /// 将 BSON 值格式化为字符串
    fn format_bson_value(value: &bson::Bson) -> String {
        match value {
            bson::Bson::Int32(n) => format!("{}", n),
            bson::Bson::Int64(n) => format!("{}", n),
            bson::Bson::Double(n) => {
                if *n == (*n as i64) as f64 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            bson::Bson::String(s) => s.clone(),
            bson::Bson::Boolean(b) => format!("{}", b),
            bson::Bson::Null => "null".to_string(),
            _ => format!("{:?}", value),
        }
    }

    /// 根据配置的字段生成 key
    fn generate_key(&self, doc: &Document) -> Result<String, ParserError> {
        if self.key_fields.is_empty() {
            return Err(ParserError::ParseFailed("no key fields configured".to_string()));
        }

        let mut key_parts = Vec::new();
        for field in &self.key_fields {
            let value = Self::get_bson_field_value(doc, field).ok_or_else(|| {
                ParserError::ParseFailed(format!("key field '{}' not found in BSON", field))
            })?;

            key_parts.push(Self::format_bson_value(&value));
        }

        Ok(key_parts.join(&self.key_separator))
    }

    /// 比较两个 BSON 值是否相等
    fn compare_bson_values(actual: &bson::Bson, expected: &bson::Bson) -> bool {
        if actual == expected {
            return true;
        }

        // 尝试字符串比较
        let actual_str = Self::format_bson_value(actual);
        let expected_str = Self::format_bson_value(expected);
        actual_str == expected_str
    }

    /// 将 serde_json::Value 转换为 bson::Bson
    fn json_to_bson(json_val: &serde_json::Value) -> bson::Bson {
        match json_val {
            serde_json::Value::Null => bson::Bson::Null,
            serde_json::Value::Bool(b) => bson::Bson::Boolean(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    bson::Bson::Int64(i)
                } else if let Some(u) = n.as_u64() {
                    bson::Bson::Int64(u as i64)
                } else {
                    bson::Bson::Double(n.as_f64().unwrap())
                }
            }
            serde_json::Value::String(s) => bson::Bson::String(s.clone()),
            serde_json::Value::Array(arr) => {
                bson::Bson::Array(arr.iter().map(Self::json_to_bson).collect())
            }
            serde_json::Value::Object(obj) => {
                let mut doc = Document::new();
                for (k, v) in obj {
                    doc.insert(k, Self::json_to_bson(v));
                }
                bson::Bson::Document(doc)
            }
        }
    }

    /// 评估单个条件是否满足
    fn evaluate_condition(&self, doc: &Document, condition: &Condition) -> bool {
        match Self::get_bson_field_value(doc, &condition.field) {
            Some(actual_value) => {
                let expected_value = Self::json_to_bson(&condition.value);
                Self::compare_bson_values(&actual_value, &expected_value)
            }
            None => false,
        }
    }

    /// 评估规则是否匹配
    fn evaluate_rule(&self, doc: &Document, rule: &ChangeTypeRule) -> bool {
        if rule.conditions.is_empty() {
            return false;
        }

        match rule.logic.as_str() {
            "OR" => rule
                .conditions
                .iter()
                .any(|c| self.evaluate_condition(doc, c)),
            _ => rule
                .conditions
                .iter()
                .all(|c| self.evaluate_condition(doc, c)),
        }
    }

    /// 根据规则确定变更类型
    fn determine_change_type(&self, doc: &Document) -> ChangeType {
        for rule in &self.change_type_rules {
            if self.evaluate_rule(doc, rule) {
                return rule.r#type;
            }
        }
        ChangeType::Add
    }
}

// 实现 From trait（注册系统需要）
impl<K, V> From<BsonParserConfig> for BsonParser<K, V> {
    fn from(config: BsonParserConfig) -> Self {
        Self::new(config)
    }
}

impl<K, V> Parser<K, V> for BsonParser<K, V>
where
    K: ParseValue + Send + Sync,
    V: for<'de> Deserialize<'de> + Send + Sync,
{
    fn parse(&self, buf: &[u8]) -> Result<(ChangeType, K, V), ParserError> {
        // 解析 BSON
        let bson_doc: Document = bson::from_slice(buf).map_err(|e| {
            ParserError::ParseFailed(format!("failed to parse BSON: {}", e))
        })?;

        // 生成 key
        let key_str = self.generate_key(&bson_doc)?;

        // 转换 key 到目标类型
        let key = K::parse_value(&key_str)
            .map_err(|e| ParserError::ParseFailed(format!("failed to parse key: {}", e)))?;

        // 反序列化 value
        let value: V = bson::from_slice(buf).map_err(|e| {
            ParserError::ParseFailed(format!("failed to deserialize value: {}", e))
        })?;

        // 确定变更类型
        let change_type = self.determine_change_type(&bson_doc);

        Ok((change_type, key, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;

    #[test]
    fn test_generate_key_single_field() {
        let config = BsonParserConfig {
            key_fields: vec!["id".to_string()],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = BsonParser::<String, Document>::new(config);

        let doc = doc! { "id": "user123", "name": "Alice" };
        let key = parser.generate_key(&doc).unwrap();
        assert_eq!(key, "user123");
    }

    #[test]
    fn test_generate_key_nested_fields() {
        let config = BsonParserConfig {
            key_fields: vec!["user.id".to_string(), "post.id".to_string()],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = BsonParser::<String, Document>::new(config);

        let doc = doc! {
            "user": { "id": "u1" },
            "post": { "id": "p1" }
        };
        let key = parser.generate_key(&doc).unwrap();
        assert_eq!(key, "u1_p1");
    }

    #[test]
    fn test_generate_key_missing_field() {
        let config = BsonParserConfig {
            key_fields: vec!["id".to_string()],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = BsonParser::<String, Document>::new(config);

        let doc = doc! { "name": "Alice" };
        let result = parser.generate_key(&doc);
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate_condition() {
        let config = BsonParserConfig {
            key_fields: vec![],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = BsonParser::<String, Document>::new(config);

        let doc = doc! { "status": "active", "count": 42 };

        let condition = Condition {
            field: "status".to_string(),
            value: json!("active"),
        };
        assert!(parser.evaluate_condition(&doc, &condition));

        let condition = Condition {
            field: "status".to_string(),
            value: json!("inactive"),
        };
        assert!(!parser.evaluate_condition(&doc, &condition));
    }

    #[test]
    fn test_evaluate_rule_and() {
        let config = BsonParserConfig {
            key_fields: vec![],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = BsonParser::<String, Document>::new(config);

        let doc = doc! { "status": "active", "count": 42 };

        let rule = ChangeTypeRule {
            conditions: vec![
                Condition {
                    field: "status".to_string(),
                    value: json!("active"),
                },
                Condition {
                    field: "count".to_string(),
                    value: json!(42),
                },
            ],
            logic: "AND".to_string(),
            r#type: ChangeType::Update,
        };

        assert!(parser.evaluate_rule(&doc, &rule));
    }

    #[test]
    fn test_evaluate_rule_or() {
        let config = BsonParserConfig {
            key_fields: vec![],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = BsonParser::<String, Document>::new(config);

        let doc = doc! { "status": "active", "count": 42 };

        let rule = ChangeTypeRule {
            conditions: vec![
                Condition {
                    field: "status".to_string(),
                    value: json!("inactive"),
                },
                Condition {
                    field: "count".to_string(),
                    value: json!(42),
                },
            ],
            logic: "OR".to_string(),
            r#type: ChangeType::Update,
        };

        assert!(parser.evaluate_rule(&doc, &rule));
    }

    #[test]
    fn test_determine_change_type() {
        let config = BsonParserConfig {
            key_fields: vec![],
            key_separator: "_".to_string(),
            change_type_rules: vec![ChangeTypeRule {
                conditions: vec![Condition {
                    field: "status".to_string(),
                    value: json!("deleted"),
                }],
                logic: "AND".to_string(),
                r#type: ChangeType::Delete,
            }],
        };
        let parser = BsonParser::<String, Document>::new(config);

        let doc = doc! { "status": "deleted", "count": 42 };
        let ct = parser.determine_change_type(&doc);
        assert_eq!(ct, ChangeType::Delete);
    }

    #[test]
    fn test_parse_basic() {
        let config = BsonParserConfig {
            key_fields: vec!["id".to_string()],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = BsonParser::<String, Document>::new(config);

        let doc = doc! { "id": "user123", "name": "Alice" };
        let bytes = bson::to_vec(&doc).unwrap();
        let (ct, key, value) = parser.parse(&bytes).unwrap();

        assert_eq!(ct, ChangeType::Add);
        assert_eq!(key, "user123");
        assert_eq!(value.get_str("name").unwrap(), "Alice");
    }

    #[test]
    fn test_parse_with_change_type_rule() {
        let config = BsonParserConfig {
            key_fields: vec!["id".to_string()],
            key_separator: "_".to_string(),
            change_type_rules: vec![ChangeTypeRule {
                conditions: vec![Condition {
                    field: "operation".to_string(),
                    value: json!("delete"),
                }],
                logic: "AND".to_string(),
                r#type: ChangeType::Delete,
            }],
        };
        let parser = BsonParser::<String, Document>::new(config);

        let doc = doc! { "id": "user123", "operation": "delete" };
        let bytes = bson::to_vec(&doc).unwrap();
        let (ct, key, _) = parser.parse(&bytes).unwrap();

        assert_eq!(ct, ChangeType::Delete);
        assert_eq!(key, "user123");
    }

    #[test]
    fn test_bson_parser_config_default() {
        let json = r#"{}"#;
        let config: BsonParserConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.key_fields, vec!["id"]);
        assert_eq!(config.key_separator, "_");
        assert!(config.change_type_rules.is_empty());
    }
}
