pub mod core;
pub mod safe_hash_map_store;
pub mod hash_map_store;
pub mod redis_store;
pub mod register;

// 重新导出核心类型和 trait
pub use core::{KvError, SetOptions, Store};
// 重新导出具体实现
pub use safe_hash_map_store::{SafeHashMapStore, SafeHashMapStoreConfig};
pub use hash_map_store::{HashMapStore, HashMapStoreConfig};
pub use redis_store::{RedisError, RedisStore, RedisStoreConfig};
// 重新导出注册函数
pub use register::{register_hash_stores, register_redis_stores};
