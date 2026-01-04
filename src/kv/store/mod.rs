pub mod core;
pub mod memory;

// 重新导出核心类型和 trait
pub use core::{Store, KvError, SetOptions};
// 重新导出具体实现
pub use memory::{MapStore, MapStoreConfig};