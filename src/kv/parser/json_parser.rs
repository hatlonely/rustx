use serde::Deserialize;
use serde::de::Deserializer;
use std::marker::PhantomData;

use super::{Parser, ParserError, ChangeType, ParseValue};

/// 条件表达式（对应 Golang Condition）
///
/// 用于定义匹配规则，指定字段路径和期望值。
#[derive(Debug, Clone, Deserialize)]
pub struct Condition {
    /// 字段路径，支持嵌套，如 "user.status" 或 "metadata.timestamp"
    pub field: String,
    /// 期望值（可以是字符串、数字、布尔值等）
    pub value: serde_json::Value,
}

/// 变更类型规则（对应 Golang ChangeTypeRule）
///
/// 定义一组条件，当条件满足时返回指定的变更类型。
#[derive(Debug, Clone, Deserialize)]
pub struct ChangeTypeRule {
    /// 条件列表
    pub conditions: Vec<Condition>,
    /// 逻辑关系：AND 或 OR（默认：AND）
    #[serde(default = "default_logic")]
    pub logic: String,
    /// 满足条件时的变更类型
    #[serde(deserialize_with = "deserialize_change_type")]
    pub r#type: ChangeType,
}

fn default_logic() -> String {
    "AND".to_string()
}

pub(crate) fn deserialize_change_type<'de, D>(deserializer: D) -> Result<ChangeType, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;

    match value {
        // 数字：1=Add, 2=Update, 3=Delete
        serde_json::Value::Number(n) => {
            if let Some(n) = n.as_i64() {
                Ok(match n {
                    1 => ChangeType::Add,
                    2 => ChangeType::Update,
                    3 => ChangeType::Delete,
                    _ => ChangeType::Unknown,
                })
            } else {
                Ok(ChangeType::Unknown)
            }
        }
        // 字符串：add/update/delete
        serde_json::Value::String(s) => Ok(match s.to_lowercase().as_str() {
            "add" => ChangeType::Add,
            "update" => ChangeType::Update,
            "delete" => ChangeType::Delete,
            _ => ChangeType::Unknown,
        }),
        _ => Ok(ChangeType::Unknown),
    }
}

/// JsonParser 配置（遵循 cfg/README.md 最佳实践）
#[derive(Debug, Clone, Deserialize)]
pub struct JsonParserConfig {
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

/// JSON 解析器（对应 Golang JsonParser[K, V]）
///
/// 从 JSON 数据中解析键值对，支持：
/// - 多字段组合生成 key
/// - 条件规则匹配变更类型
/// - 嵌套字段访问（如 "user.id"）
///
/// # 示例
/// ```ignore
/// use rustx::kv::parser::{JsonParser, JsonParserConfig, Parser};
///
/// let config = JsonParserConfig {
///     key_fields: vec!["user.id".to_string(), "post.id".to_string()],
///     key_separator: "_".to_string(),
///     change_type_rules: vec![],
/// };
/// let parser = JsonParser::<String, serde_json::Value>::new(config);
///
/// let json = r#"{"user":{"id":"u1"}, "post":{"id":"p1"}, "title":"hello"}"#;
/// let (ct, key, value) = parser.parse(json.as_bytes()).unwrap();
/// assert_eq!(key, "u1_p1");
/// ```
pub struct JsonParser<K, V> {
    key_fields: Vec<String>,
    key_separator: String,
    change_type_rules: Vec<ChangeTypeRule>,
    _phantom: PhantomData<(K, V)>,
}

impl<K, V> JsonParser<K, V> {
    /// 唯一的构造方法（遵循 cfg/README.md 最佳实践）
    pub fn new(config: JsonParserConfig) -> Self {
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

    /// 从 JSON 对象中提取指定路径的字段值
    ///
    /// 支持嵌套路径，如 "user.id" 或 "metadata.timestamp"
    fn get_field_value<'a>(data: &'a serde_json::Value, field_path: &str) -> Option<&'a serde_json::Value> {
        if field_path.is_empty() {
            return None;
        }

        let parts: Vec<&str> = field_path.split('.').collect();
        let mut current = data;

        for (i, part) in parts.iter().enumerate() {
            match current.get(part) {
                Some(value) => {
                    if i == parts.len() - 1 {
                        return Some(value);
                    }
                    current = value;
                }
                None => return None,
            }
        }

        None
    }

    /// 将值格式化为字符串（避免科学记数法）
    fn format_value(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Number(n) => {
                if n.is_i64() {
                    format!("{}", n.as_i64().unwrap())
                } else if n.is_u64() {
                    format!("{}", n.as_u64().unwrap())
                } else {
                    format!("{}", n.as_f64().unwrap())
                }
            }
            serde_json::Value::String(s) => s.clone(),
            _ => value.to_string(),
        }
    }

    /// 根据配置的字段生成 key
    fn generate_key(&self, data: &serde_json::Value) -> Result<String, ParserError> {
        if self.key_fields.is_empty() {
            return Err(ParserError::ParseFailed("no key fields configured".to_string()));
        }

        let mut key_parts = Vec::new();
        for field in &self.key_fields {
            let value = Self::get_field_value(data, field).ok_or_else(|| {
                ParserError::ParseFailed(format!("key field '{}' not found in JSON", field))
            })?;

            key_parts.push(Self::format_value(value));
        }

        Ok(key_parts.join(&self.key_separator))
    }

    /// 比较两个值是否相等
    fn compare_values(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
        if actual == expected {
            return true;
        }

        // 尝试字符串比较
        let actual_str = Self::format_value(actual);
        let expected_str = Self::format_value(expected);
        actual_str == expected_str
    }

    /// 评估单个条件是否满足
    fn evaluate_condition(&self, data: &serde_json::Value, condition: &Condition) -> bool {
        match Self::get_field_value(data, &condition.field) {
            Some(actual_value) => Self::compare_values(actual_value, &condition.value),
            None => false,
        }
    }

    /// 评估规则是否匹配
    fn evaluate_rule(&self, data: &serde_json::Value, rule: &ChangeTypeRule) -> bool {
        if rule.conditions.is_empty() {
            return false;
        }

        match rule.logic.as_str() {
            "OR" => rule.conditions.iter().any(|c| self.evaluate_condition(data, c)),
            _ => rule.conditions.iter().all(|c| self.evaluate_condition(data, c)),
        }
    }

    /// 根据规则确定变更类型
    fn determine_change_type(&self, data: &serde_json::Value) -> ChangeType {
        for rule in &self.change_type_rules {
            if self.evaluate_rule(data, rule) {
                return rule.r#type;
            }
        }
        ChangeType::Add
    }
}

// 实现 From trait（注册系统需要）
impl<K, V> From<JsonParserConfig> for JsonParser<K, V> {
    fn from(config: JsonParserConfig) -> Self {
        Self::new(config)
    }
}

// 实现 From<Box<JsonParser>> for Box<dyn Parser>（注册系统需要）
impl<K, V> From<Box<JsonParser<K, V>>> for Box<dyn super::Parser<K, V>>
where
    K: ParseValue + Send + Sync + 'static,
    V: for<'de> serde::Deserialize<'de> + Send + Sync + 'static,
{
    fn from(source: Box<JsonParser<K, V>>) -> Self {
        source as Box<dyn super::Parser<K, V>>
    }
}

impl<K, V> Parser<K, V> for JsonParser<K, V>
where
    K: ParseValue + Send + Sync,
    V: for<'de> Deserialize<'de> + Send + Sync,
{
    fn parse(&self, buf: &[u8]) -> Result<(ChangeType, K, V), ParserError> {
        // 解析 JSON
        let json_data: serde_json::Value = serde_json::from_slice(buf).map_err(|e| {
            ParserError::ParseFailed(format!("failed to parse JSON: {}", e))
        })?;

        // 生成 key
        let key_str = self.generate_key(&json_data)?;

        // 转换 key 到目标类型
        let key = K::parse_value(&key_str)
            .map_err(|e| ParserError::ParseFailed(format!("failed to parse key: {}", e)))?;

        // 反序列化 value
        let value: V = serde_json::from_slice(buf).map_err(|e| {
            ParserError::ParseFailed(format!("failed to deserialize value: {}", e))
        })?;

        // 确定变更类型
        let change_type = self.determine_change_type(&json_data);

        Ok((change_type, key, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_generate_key_single_field() {
        let config = JsonParserConfig {
            key_fields: vec!["id".to_string()],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = JsonParser::<String, serde_json::Value>::new(config);

        let data = json!({"id": "user123", "name": "Alice"});
        let key = parser.generate_key(&data).unwrap();
        assert_eq!(key, "user123");
    }

    #[test]
    fn test_generate_key_multiple_fields() {
        let config = JsonParserConfig {
            key_fields: vec!["user.id".to_string(), "post.id".to_string()],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = JsonParser::<String, serde_json::Value>::new(config);

        let data = json!({
            "user": {"id": "u1"},
            "post": {"id": "p1"}
        });
        let key = parser.generate_key(&data).unwrap();
        assert_eq!(key, "u1_p1");
    }

    #[test]
    fn test_generate_key_missing_field() {
        let config = JsonParserConfig {
            key_fields: vec!["id".to_string()],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = JsonParser::<String, serde_json::Value>::new(config);

        let data = json!({"name": "Alice"});
        let result = parser.generate_key(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate_condition() {
        let config = JsonParserConfig {
            key_fields: vec![],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = JsonParser::<String, serde_json::Value>::new(config);

        let data = json!({"status": "active", "count": 42});

        let condition = Condition {
            field: "status".to_string(),
            value: json!("active"),
        };
        assert!(parser.evaluate_condition(&data, &condition));

        let condition = Condition {
            field: "status".to_string(),
            value: json!("inactive"),
        };
        assert!(!parser.evaluate_condition(&data, &condition));
    }

    #[test]
    fn test_evaluate_rule_and() {
        let config = JsonParserConfig {
            key_fields: vec![],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = JsonParser::<String, serde_json::Value>::new(config);

        let data = json!({"status": "active", "count": 42});

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

        assert!(parser.evaluate_rule(&data, &rule));
    }

    #[test]
    fn test_evaluate_rule_or() {
        let config = JsonParserConfig {
            key_fields: vec![],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = JsonParser::<String, serde_json::Value>::new(config);

        let data = json!({"status": "active", "count": 42});

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

        assert!(parser.evaluate_rule(&data, &rule));
    }

    #[test]
    fn test_determine_change_type() {
        let config = JsonParserConfig {
            key_fields: vec![],
            key_separator: "_".to_string(),
            change_type_rules: vec![
                ChangeTypeRule {
                    conditions: vec![Condition {
                        field: "status".to_string(),
                        value: json!("deleted"),
                    }],
                    logic: "AND".to_string(),
                    r#type: ChangeType::Delete,
                },
            ],
        };
        let parser = JsonParser::<String, serde_json::Value>::new(config);

        let data = json!({"status": "deleted", "count": 42});
        let ct = parser.determine_change_type(&data);
        assert_eq!(ct, ChangeType::Delete);
    }

    #[test]
    fn test_parse_basic() {
        let config = JsonParserConfig {
            key_fields: vec!["id".to_string()],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = JsonParser::<String, serde_json::Value>::new(config);

        let json = r#"{"id":"user123","name":"Alice"}"#;
        let (ct, key, value) = parser.parse(json.as_bytes()).unwrap();

        assert_eq!(ct, ChangeType::Add);
        assert_eq!(key, "user123");
        assert_eq!(value, json!({"id":"user123","name":"Alice"}));
    }

    #[test]
    fn test_parse_with_change_type_rule() {
        let config = JsonParserConfig {
            key_fields: vec!["id".to_string()],
            key_separator: "_".to_string(),
            change_type_rules: vec![
                ChangeTypeRule {
                    conditions: vec![Condition {
                        field: "operation".to_string(),
                        value: json!("delete"),
                    }],
                    logic: "AND".to_string(),
                    r#type: ChangeType::Delete,
                },
            ],
        };
        let parser = JsonParser::<String, serde_json::Value>::new(config);

        let json = r#"{"id":"user123","operation":"delete"}"#;
        let (ct, key, _) = parser.parse(json.as_bytes()).unwrap();

        assert_eq!(ct, ChangeType::Delete);
        assert_eq!(key, "user123");
    }

    #[test]
    fn test_json_parser_config_default() {
        let json = r#"{}"#;
        let config: JsonParserConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.key_fields, vec!["id"]);
        assert_eq!(config.key_separator, "_");
        assert!(config.change_type_rules.is_empty());
    }

    #[test]
    fn test_json_parser_config_with_rules() {
        let json = r#"{
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
        }"#;
        let config: JsonParserConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.key_fields, vec!["id"]);
        assert_eq!(config.change_type_rules.len(), 1);
        assert_eq!(config.change_type_rules[0].conditions.len(), 1);
        assert_eq!(config.change_type_rules[0].conditions[0].field, "operation");
        assert_eq!(config.change_type_rules[0].conditions[0].value, "delete");
        assert_eq!(config.change_type_rules[0].r#type, ChangeType::Delete);
    }

    #[test]
    fn test_parse_with_struct_value() {
        use serde::Deserialize;

        // 定义一个结构体作为 Value 类型
        #[derive(Debug, Deserialize, PartialEq)]
        struct User {
            id: String,
            name: String,
            age: i32,
        }

        let config = JsonParserConfig {
            key_fields: vec!["id".to_string()],
            key_separator: "_".to_string(),
            change_type_rules: vec![],
        };
        let parser = JsonParser::<String, User>::new(config);

        let json = r#"{"id":"user123","name":"Alice","age":30}"#;
        let (ct, key, value) = parser.parse(json.as_bytes()).unwrap();

        assert_eq!(ct, ChangeType::Add);
        assert_eq!(key, "user123");
        assert_eq!(value, User {
            id: "user123".to_string(),
            name: "Alice".to_string(),
            age: 30,
        });
    }
}
