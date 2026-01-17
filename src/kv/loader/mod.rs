pub mod core;
pub mod kv_file_stream;
pub mod kv_file_loader;
pub mod empty_kv_stream;
pub mod file_trigger;
pub mod registry;

// 重新导出核心类型和 trait
pub use core::{
    KvStream, Listener, Loader, LoaderError, LOAD_STRATEGY_INPLACE, LOAD_STRATEGY_REPLACE,
};

// 重新导出实现类
pub use kv_file_loader::{KvFileLoader, KvFileLoaderConfig};
pub use kv_file_stream::KvFileStream;
pub use empty_kv_stream::EmptyKvStream;
pub use file_trigger::{FileTrigger, FileTriggerConfig};

// 重新导出注册函数
pub use registry::register_loaders;
