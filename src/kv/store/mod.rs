pub mod core;
pub mod safe_hash_map_store;
pub mod hash_map_store;
pub mod registry;

// 重新导出核心类型和 trait
pub use core::{KvError, SetOptions, Store};
// 重新导出具体实现
pub use safe_hash_map_store::{SafeHashMapStore, SafeHashMapStoreConfig};
pub use hash_map_store::{HashMapStore, HashMapStoreConfig};
// 重新导出注册函数
pub use registry::register_hash_stores;
