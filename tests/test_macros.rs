//! ParseValue derive macro 的集成测试

use rustx::kv::parser::ParseValue;
use rustx_macros::ParseValue;
use serde::Deserialize;

// ============================================================================
// 测试结构体定义
// ============================================================================

#[derive(Debug, Deserialize, ParseValue)]
struct User {
    name: String,
    age: i32,
}

#[derive(Debug, Deserialize, ParseValue)]
struct Config {
    host: String,
    port: u16,
    debug: bool,
}

#[derive(Debug, Deserialize, ParseValue)]
struct Point {
    x: f64,
    y: f64,
}

// ============================================================================
// 测试用例
// ============================================================================

#[test]
fn test_parse_user() {
    let json = r#"{"name":"Alice","age":30}"#;
    let result = User::parse_value(json).unwrap();
    assert_eq!(result.name, "Alice");
    assert_eq!(result.age, 30);
}

#[test]
fn test_parse_config() {
    let json = r#"{"host":"localhost","port":8080,"debug":true}"#;
    let result = Config::parse_value(json).unwrap();
    assert_eq!(result.host, "localhost");
    assert_eq!(result.port, 8080);
    assert_eq!(result.debug, true);
}

#[test]
fn test_parse_point() {
    let json = r#"{"x":1.5,"y":2.5}"#;
    let result = Point::parse_value(json).unwrap();
    assert_eq!(result.x, 1.5);
    assert_eq!(result.y, 2.5);
}

#[test]
fn test_invalid_json() {
    let invalid_json = r#"{"name":"Alice""#;  // 缺少闭合括号
    let result = User::parse_value(invalid_json);
    assert!(result.is_err());
}

#[test]
fn test_missing_field() {
    let json = r#"{"name":"Bob"}"#;  // 缺少 age 字段
    let result = User::parse_value(json);
    assert!(result.is_err());
}

#[test]
fn test_wrong_type() {
    let json = r#"{"name":"Charlie","age":"thirty"}"#;  // age 应该是数字
    let result = User::parse_value(json);
    assert!(result.is_err());
}
