pub mod core;

// 重新导出核心类型和 trait
pub use core::{
    Loader, KvStream, LoaderError, Listener,
    LOAD_STRATEGY_REPLACE, LOAD_STRATEGY_INPLACE,
};