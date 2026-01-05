pub mod core;
pub mod map_store;

// 重新导出核心类型和 trait
pub use core::{Store, KvError, SetOptions};
// 重新导出具体实现
pub use map_store::{MapStore, MapStoreConfig};
