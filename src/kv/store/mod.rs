pub mod core;
pub mod safe_hash_map_store;

// 重新导出核心类型和 trait
pub use core::{KvError, SetOptions, Store};
// 重新导出具体实现
pub use safe_hash_map_store::{SafeHashMapStore, SafeHashMapStoreConfig};
