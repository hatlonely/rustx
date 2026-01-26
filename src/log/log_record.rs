use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use std::fmt;
use std::str::FromStr;
use std::time::SystemTime;

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    /// 最详细的日志
    Trace = 0,
    /// 调试信息
    Debug = 1,
    /// 一般信息
    Info = 2,
    /// 警告信息
    Warn = 3,
    /// 错误信息
    Error = 4,
}

impl FromStr for LogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "trace" => Ok(LogLevel::Trace),
            "debug" => Ok(LogLevel::Debug),
            "info" => Ok(LogLevel::Info),
            "warn" => Ok(LogLevel::Warn),
            "error" => Ok(LogLevel::Error),
            _ => Err(format!("invalid log level: {}", s)),
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "TRACE"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

/// 元数据值，支持多种类型
#[derive(Debug, Clone)]
pub enum MetadataValue {
    String(String),
    I64(i64),
    U64(u64),
    F64(f64),
    Bool(bool),
    Null,
    /// 任意 JSON 兼容的数据
    Json(Value),
    /// 自定义结构体（内部序列化为 JSON）
    Struct(Value),
}

impl Serialize for MetadataValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            MetadataValue::String(s) => serializer.serialize_str(s),
            MetadataValue::I64(n) => serializer.serialize_i64(*n),
            MetadataValue::U64(n) => serializer.serialize_u64(*n),
            MetadataValue::F64(n) => serializer.serialize_f64(*n),
            MetadataValue::Bool(b) => serializer.serialize_bool(*b),
            MetadataValue::Null => serializer.serialize_none(),
            MetadataValue::Json(v) => v.serialize(serializer),
            MetadataValue::Struct(v) => v.serialize(serializer),
        }
    }
}

impl fmt::Display for MetadataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetadataValue::String(s) => write!(f, "{}", s),
            MetadataValue::I64(n) => write!(f, "{}", n),
            MetadataValue::U64(n) => write!(f, "{}", n),
            MetadataValue::F64(n) => write!(f, "{}", n),
            MetadataValue::Bool(b) => write!(f, "{}", b),
            MetadataValue::Null => write!(f, "null"),
            MetadataValue::Json(v) => write!(f, "'{}'", v),
            MetadataValue::Struct(v) => write!(f, "'{}'", v),
        }
    }
}

/// 日志记录
pub struct LogRecord {
    /// 日志级别
    pub level: LogLevel,
    /// 日志消息
    pub message: String,
    /// 模块路径
    pub module: Option<String>,
    /// 源文件路径
    pub file: Option<String>,
    /// 行号
    pub line: Option<u32>,
    /// 时间戳
    pub timestamp: SystemTime,
    /// 线程 ID（已缓存的字符串表示）
    pub thread_id: String,
    /// 自定义元数据（使用 Vec 以获得更好的迭代性能）
    pub metadata: Vec<(String, MetadataValue)>,
}

impl LogRecord {
    /// 创建新的日志记录
    pub fn new(level: LogLevel, message: String) -> Self {
        Self {
            level,
            message,
            module: None,
            file: None,
            line: None,
            timestamp: SystemTime::now(),
            // 缓存 thread_id 的字符串表示，避免每次格式化时转换
            thread_id: format!("{:?}", std::thread::current().id()),
            metadata: Vec::new(),
        }
    }

    /// 添加元数据
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<MetadataValue>,
    ) -> Self {
        self.metadata.push((key.into(), value.into()));
        self
    }

    /// 设置位置信息（文件和行号）
    pub fn with_location(mut self, file: String, line: u32) -> Self {
        self.file = Some(file);
        self.line = Some(line);
        self
    }

    /// 设置模块路径
    pub fn with_module(mut self, module: String) -> Self {
        self.module = Some(module);
        self
    }
}

// 为各种类型实现 From<MetadataValue> 以方便使用
impl From<String> for MetadataValue {
    fn from(s: String) -> Self {
        MetadataValue::String(s)
    }
}

impl From<&str> for MetadataValue {
    fn from(s: &str) -> Self {
        MetadataValue::String(s.to_string())
    }
}

impl From<i64> for MetadataValue {
    fn from(n: i64) -> Self {
        MetadataValue::I64(n)
    }
}

impl From<i32> for MetadataValue {
    fn from(n: i32) -> Self {
        MetadataValue::I64(n as i64)
    }
}

impl From<u64> for MetadataValue {
    fn from(n: u64) -> Self {
        MetadataValue::U64(n)
    }
}

impl From<u32> for MetadataValue {
    fn from(n: u32) -> Self {
        MetadataValue::U64(n as u64)
    }
}

impl From<f64> for MetadataValue {
    fn from(n: f64) -> Self {
        MetadataValue::F64(n)
    }
}

impl From<f32> for MetadataValue {
    fn from(n: f32) -> Self {
        MetadataValue::F64(n as f64)
    }
}

impl From<bool> for MetadataValue {
    fn from(b: bool) -> Self {
        MetadataValue::Bool(b)
    }
}

impl From<Value> for MetadataValue {
    fn from(v: Value) -> Self {
        MetadataValue::Json(v)
    }
}

impl MetadataValue {
    /// 从任意实现了 Serialize 的自定义结构体创建 MetadataValue
    ///
    /// # 示例
    ///
    /// ```ignore
    /// #[derive(Serialize)]
    /// struct User {
    ///     id: i64,
    ///     name: String,
    /// }
    ///
    /// let user = User { id: 123, name: "alice".to_string() };
    /// let value = MetadataValue::from_struct(user);
    /// ```
    pub fn from_struct<T: serde::Serialize>(value: T) -> Self {
        match serde_json::to_value(value) {
            Ok(json_value) => MetadataValue::Struct(json_value),
            Err(_) => MetadataValue::Null,
        }
    }
}

impl Serialize for LogRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        use serde_json::Map;
        use std::time::UNIX_EPOCH;

        // 计算 timestamp（毫秒）
        let timestamp = self
            .timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // 将 Vec 转换为 Map 以便序列化为 JSON 对象
        let metadata_map: Map<String, Value> = self
            .metadata
            .iter()
            .map(|(k, v)| {
                let json_value = serde_json::to_value(v).unwrap_or(Value::Null);
                (k.clone(), json_value)
            })
            .collect();

        let mut map = serializer.serialize_map(Some(9))?;
        map.serialize_entry("timestamp", &timestamp)?;
        map.serialize_entry("level", &self.level.to_string())?;
        map.serialize_entry("message", &self.message)?;
        map.serialize_entry("module", &self.module)?;
        map.serialize_entry("file", &self.file)?;
        map.serialize_entry("line", &self.line)?;
        map.serialize_entry("thread_id", &self.thread_id)?;

        // 序列化 metadata
        if !self.metadata.is_empty() {
            map.serialize_entry("metadata", &metadata_map)?;
        } else {
            map.serialize_entry("metadata", &None::<&Map<String, Value>>)?;
        }

        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_from_str() {
        assert_eq!(LogLevel::from_str("trace").unwrap(), LogLevel::Trace);
        assert_eq!(LogLevel::from_str("DEBUG").unwrap(), LogLevel::Debug);
        assert_eq!(LogLevel::from_str("Info").unwrap(), LogLevel::Info);
        assert_eq!(LogLevel::from_str("WARN").unwrap(), LogLevel::Warn);
        assert_eq!(LogLevel::from_str("error").unwrap(), LogLevel::Error);
    }

    #[test]
    fn test_log_level_from_str_invalid() {
        assert!(LogLevel::from_str("invalid").is_err());
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Trace.to_string(), "TRACE");
        assert_eq!(LogLevel::Debug.to_string(), "DEBUG");
        assert_eq!(LogLevel::Info.to_string(), "INFO");
        assert_eq!(LogLevel::Warn.to_string(), "WARN");
        assert_eq!(LogLevel::Error.to_string(), "ERROR");
    }

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Error > LogLevel::Warn);
        assert!(LogLevel::Warn > LogLevel::Info);
        assert!(LogLevel::Info > LogLevel::Debug);
        assert!(LogLevel::Debug > LogLevel::Trace);
    }

    #[test]
    fn test_log_record_new() {
        let record = LogRecord::new(LogLevel::Info, "test message".to_string());

        assert_eq!(record.level, LogLevel::Info);
        assert_eq!(record.message, "test message");
        assert!(record.module.is_none());
        assert!(record.file.is_none());
        assert!(record.line.is_none());
        assert!(record.metadata.is_empty());
        // 验证 thread_id 是字符串
        assert!(!record.thread_id.is_empty());
    }

    #[test]
    fn test_log_record_with_metadata() {
        let record = LogRecord::new(LogLevel::Info, "test message".to_string())
            .with_metadata("user_id", 12345)
            .with_metadata("username", "alice")
            .with_metadata("success", true);

        assert_eq!(record.metadata.len(), 3);
        assert_eq!(record.metadata[0].0, "user_id");
        assert!(matches!(record.metadata[0].1, MetadataValue::I64(12345)));
        assert_eq!(record.metadata[1].0, "username");
        assert!(matches!(record.metadata[1].1, MetadataValue::String(_)));
        assert_eq!(record.metadata[2].0, "success");
        assert!(matches!(record.metadata[2].1, MetadataValue::Bool(true)));
    }

    #[test]
    fn test_log_record_with_location() {
        let record = LogRecord::new(LogLevel::Debug, "message".to_string())
            .with_location("file.rs".to_string(), 42);

        assert_eq!(record.file, Some("file.rs".to_string()));
        assert_eq!(record.line, Some(42));
    }

    #[test]
    fn test_log_record_with_module() {
        let record = LogRecord::new(LogLevel::Error, "error".to_string())
            .with_module("my_module".to_string());

        assert_eq!(record.module, Some("my_module".to_string()));
    }

    #[test]
    fn test_log_record_builder_pattern() {
        let record = LogRecord::new(LogLevel::Warn, "warning".to_string())
            .with_module("main".to_string())
            .with_location("main.rs".to_string(), 10);

        assert_eq!(record.level, LogLevel::Warn);
        assert_eq!(record.module, Some("main".to_string()));
        assert_eq!(record.file, Some("main.rs".to_string()));
        assert_eq!(record.line, Some(10));
    }

    #[test]
    fn test_metadata_value_display() {
        assert_eq!(
            format!("{}", MetadataValue::String("hello".to_string())),
            "hello"
        );
        assert_eq!(format!("{}", MetadataValue::I64(42)), "42");
        assert_eq!(format!("{}", MetadataValue::U64(100)), "100");
        assert_eq!(format!("{}", MetadataValue::F64(3.14)), "3.14");
        assert_eq!(format!("{}", MetadataValue::Bool(true)), "true");
        assert_eq!(format!("{}", MetadataValue::Bool(false)), "false");
        assert_eq!(format!("{}", MetadataValue::Null), "null");
    }

    #[test]
    fn test_metadata_value_serialize() {
        assert_eq!(
            serde_json::to_string(&MetadataValue::String("hello".to_string())).unwrap(),
            "\"hello\""
        );
        assert_eq!(
            serde_json::to_string(&MetadataValue::I64(42)).unwrap(),
            "42"
        );
        assert_eq!(
            serde_json::to_string(&MetadataValue::U64(100)).unwrap(),
            "100"
        );
        assert_eq!(
            serde_json::to_string(&MetadataValue::F64(3.14)).unwrap(),
            "3.14"
        );
        assert_eq!(
            serde_json::to_string(&MetadataValue::Bool(true)).unwrap(),
            "true"
        );
        assert_eq!(serde_json::to_string(&MetadataValue::Null).unwrap(), "null");
    }

    #[test]
    fn test_log_record_serialize_with_metadata() {
        let metadata = vec![
            ("user_id".to_string(), MetadataValue::I64(12345)),
            (
                "username".to_string(),
                MetadataValue::String("alice".to_string()),
            ),
        ];

        let record = LogRecord::new(LogLevel::Info, "test message".to_string());
        let record_with_metadata = LogRecord { metadata, ..record };

        let json = serde_json::to_string(&record_with_metadata).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["level"], "INFO");
        assert_eq!(value["message"], "test message");
        assert!(value["metadata"].is_object());
        assert_eq!(value["metadata"]["user_id"], 12345);
        assert_eq!(value["metadata"]["username"], "alice");
    }

    #[test]
    fn test_metadata_value_from_struct() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct User {
            id: i64,
            name: String,
            email: String,
        }

        let user = User {
            id: 12345,
            name: "alice".to_string(),
            email: "alice@example.com".to_string(),
        };

        // 测试从自定义结构体转换为 MetadataValue
        let metadata_value = MetadataValue::from_struct(user);

        // 验证序列化结果
        let json = serde_json::to_string(&metadata_value).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["id"], 12345);
        assert_eq!(value["name"], "alice");
        assert_eq!(value["email"], "alice@example.com");
    }

    #[test]
    fn test_log_record_with_struct_metadata() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct RequestInfo {
            endpoint: String,
            method: String,
            duration_ms: u64,
        }

        let request_info = RequestInfo {
            endpoint: "/api/users".to_string(),
            method: "GET".to_string(),
            duration_ms: 123,
        };

        let record = LogRecord::new(LogLevel::Info, "request completed".to_string())
            .with_metadata("request", MetadataValue::from_struct(request_info));

        // 验证 metadata 中包含了结构化的数据
        assert!(record.metadata.iter().any(|(k, _)| k == "request"));

        // 验证序列化后的 JSON
        let json = serde_json::to_string(&record).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["metadata"]["request"]["endpoint"], "/api/users");
        assert_eq!(value["metadata"]["request"]["method"], "GET");
        assert_eq!(value["metadata"]["request"]["duration_ms"], 123);
    }
}
