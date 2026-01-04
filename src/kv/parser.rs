use std::future::Future;
use thiserror::Error;

/// 数据变更类型（对应 Golang ChangeType）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Unknown = 0,
    Add = 1,
    Update = 2,
    Delete = 3,
}

impl Default for ChangeType {
    fn default() -> Self {
        ChangeType::Unknown
    }
}

/// 解析相关错误
#[derive(Error, Debug)]
pub enum ParserError {
    #[error("Parse failed: {0}")]
    ParseFailed(String),
}

/// 核心解析器 trait（对应 Golang Parser[K, V] interface）
/// 
/// K: 键类型
/// V: 值类型
pub trait Parser<K, V>: Send + Sync {
    /// 解析单条记录（对应 Golang Parse 方法）
    /// 
    /// # 参数
    /// - buf: 待解析的字节数据
    /// 
    /// # 返回
    /// - Ok((ChangeType, K, V)): 解析结果
    /// - Err(ParserError): 解析失败
    fn parse(&self, buf: &[u8]) -> impl Future<Output = Result<(ChangeType, K, V), ParserError>> + Send;
}

