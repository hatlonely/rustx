pub mod core;
pub mod rwlock_hash_map_store;
pub mod unsafe_hash_map_store;
pub mod redis_store;
pub mod register;

// 重新导出核心类型和 trait
pub use core::{IsSyncStore, KvError, SetOptions, Store, SyncStore};
// 重新导出具体实现
pub use rwlock_hash_map_store::{RwLockHashMapStore, RwLockHashMapStoreConfig};
pub use unsafe_hash_map_store::{UnsafeHashMapStore, UnsafeHashMapStoreConfig};
pub use redis_store::{RedisError, RedisStore, RedisStoreConfig};
// 重新导出注册函数
pub use register::{register_hash_stores, register_redis_stores};
