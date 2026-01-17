use serde::Deserialize;
use std::str::FromStr;

use super::ParserError;

/// 从字符串解析值的 trait
///
/// 提供从字符串到目标类型的转换能力。
/// 对于基本类型使用 FromStr trait，对于复杂类型使用 JSON 反序列化。
pub trait ParseValue: Sized {
    /// 从字符串解析值
    ///
    /// # 参数
    /// - s: 字符串形式的值
    ///
    /// # 返回
    /// - Ok(T): 解析成功
    /// - Err(ParserError): 解析失败
    fn parse_value(s: &str) -> Result<Self, ParserError>;
}

// 为 String 实现
impl ParseValue for String {
    fn parse_value(s: &str) -> Result<Self, ParserError> {
        Ok(s.to_string())
    }
}

// 为数字类型实现的宏
macro_rules! impl_parse_value_numeric {
    ($($ty:ty),*) => {
        $(
            impl ParseValue for $ty {
                fn parse_value(s: &str) -> Result<Self, ParserError> {
                    s.parse().map_err(|e| ParserError::ParseFailed(
                        format!("failed to parse {} from '{}': {}", stringify!($ty), s, e)
                    ))
                }
            }
        )*
    };
}

// 为所有基本数字类型实现
impl_parse_value_numeric!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, f32, f64);

// 为 bool 实现
impl ParseValue for bool {
    fn parse_value(s: &str) -> Result<Self, ParserError> {
        s.parse().map_err(|e| ParserError::ParseFailed(
            format!("failed to parse bool from '{}': {}", s, e)
        ))
    }
}

// 为 serde_json::Value 实现（总是使用 JSON 解析）
impl ParseValue for serde_json::Value {
    fn parse_value(s: &str) -> Result<Self, ParserError> {
        serde_json::from_str(s).map_err(|e| ParserError::ParseFailed(
            format!("failed to parse JSON from '{}': {}", s, e)
        ))
    }
}

/// 通用解析函数：先尝试 FromStr，失败后尝试 JSON 解析
///
/// 这个函数提供了更灵活的解析策略：
/// 1. 首先尝试使用 FromStr trait 解析（适用于基本类型）
/// 2. 如果失败，尝试使用 JSON 反序列化（适用于复杂类型）
///
/// # 参数
/// - s: 字符串形式的值
///
/// # 返回
/// - Ok(T): 解析成功
/// - Err(ParserError): 两种解析方式都失败
pub fn parse_value_with_fallback<T: for<'de> Deserialize<'de> + FromStr>(s: &str) -> Result<T, ParserError>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    // 首先尝试 FromStr
    if let Ok(v) = s.parse::<T>() {
        return Ok(v);
    }

    // FromStr 失败，尝试 JSON 解析
    serde_json::from_str(s).map_err(|e| ParserError::ParseFailed(
        format!("failed to parse value (tried FromStr and JSON): {}", e)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_string() {
        let result: String = ParseValue::parse_value("hello").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_parse_i32() {
        let result: i32 = ParseValue::parse_value("42").unwrap();
        assert_eq!(result, 42);

        let result: Result<i32, _> = ParseValue::parse_value("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_f64() {
        let result: f64 = ParseValue::parse_value("3.14").unwrap();
        assert_eq!(result, 3.14);

        let result: Result<f64, _> = ParseValue::parse_value("nan");
        assert!(result.is_ok()); // f64::parse 支持 "nan"
    }

    #[test]
    fn test_parse_bool() {
        let result: bool = ParseValue::parse_value("true").unwrap();
        assert_eq!(result, true);

        let result: bool = ParseValue::parse_value("false").unwrap();
        assert_eq!(result, false);
    }

    #[test]
    fn test_parse_json() {
        use serde_json::json;

        // JSON 对象
        let result: serde_json::Value = ParseValue::parse_value(r#"{"key": "value"}"#).unwrap();
        assert_eq!(result, json!({"key": "value"}));

        // JSON 数组
        let result: serde_json::Value = ParseValue::parse_value(r#"[1, 2, 3]"#).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn test_parse_value_with_fallback() {
        // 测试基本类型（FromStr 有效）
        let result: i32 = parse_value_with_fallback("42").unwrap();
        assert_eq!(result, 42);

        // 测试 JSON 字符串（FromStr 失败，使用 JSON 解析）
        let result: serde_json::Value = parse_value_with_fallback(r#"{"name":"test","value":123}"#).unwrap();
        assert_eq!(result["name"], "test");
        assert_eq!(result["value"], 123);
    }
}
