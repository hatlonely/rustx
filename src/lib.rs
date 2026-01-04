//! RustX - Rust 版本的工具库集合
//! 
//! 提供与 Golang 版本功能对等的抽象接口，享受 Rust 的类型安全和性能优势。
//! 
//! ## 模块
//! 
//! - **cfg**: 配置管理模块（对应 Golang cfg 包）
//! - **kv**: 键值存储抽象模块（对应 Golang kv 包）
//! 
//! ## 设计理念
//! 
//! - 🔄 **功能对等**: 与 Golang 版本保持接口兼容
//! - 🚀 **零成本抽象**: 编译时优化，无运行时开销  
//! - 🔒 **类型安全**: 编译时类型检查
//! - 🛡️ **内存安全**: Rust 所有权系统保证
//! - ⚡ **高性能**: 异步操作支持

pub mod cfg;
pub mod kv;

// 重新导出主要的公共 API
pub use cfg::{Configurable, TypeOptions, register, register_type, create_from_type_options};

pub use kv::{
    Store, KvError, SetOptions,
    Serializer, SerializerError,
    Parser, ChangeType, ParserError,
    Loader, KvStream, LoaderError, Listener,
    LOAD_STRATEGY_REPLACE, LOAD_STRATEGY_INPLACE,
};