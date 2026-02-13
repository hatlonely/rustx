//! KV 存储抽象模块
//!
//! 提供与 Golang 版本功能对等的 KV 存储抽象接口

pub mod loader;
pub mod parser;
pub mod serializer;
pub mod store;

// 重新导出核心接口
pub use loader::{
    Stream, Listener, Loader, LoaderError, LOAD_STRATEGY_INPLACE, LOAD_STRATEGY_REPLACE,
};
pub use parser::{ChangeType, Parser, ParserError};
pub use serializer::{Serializer, SerializerError};
pub use store::{KvError, SetOptions, Store, AsyncStore};
