//! KV 存储抽象模块
//! 
//! 提供与 Golang 版本功能对等的 KV 存储抽象接口

pub mod store;
pub mod serializer;
pub mod parser;
pub mod loader;

// 重新导出核心接口
pub use store::{Store, KvError, SetOptions};
pub use serializer::{Serializer, SerializerError};
pub use parser::{Parser, ChangeType, ParserError};
pub use loader::{
    Loader, KvStream, LoaderError, Listener,
    LOAD_STRATEGY_REPLACE, LOAD_STRATEGY_INPLACE,
};