use dashmap::DashMap;
use garde::Validate;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::hash::Hash;

use super::core::{IsSyncStore, KvError, SetOptions, Store, SyncStore};

/// DashMapStore 配置结构体
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, SmartDefault, Validate)]
#[serde(default)]
pub struct DashMapStoreConfig {
    /// 初始容量（可选）
    #[garde(skip)]
    pub initial_capacity: Option<usize>,
}

/// 基于 DashMap 的 KV 存储实现
pub struct DashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    map: DashMap<K, V>,
}

impl<K, V> DashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    /// 创建新的 DashMapStore 实例
    pub fn new(config: DashMapStoreConfig) -> Self {
        let initial_map = match config.initial_capacity {
            Some(capacity) => DashMap::with_capacity(capacity),
            None => DashMap::new(),
        };

        Self { map: initial_map }
    }
}

impl<K, V> Default for DashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    fn default() -> Self {
        Self::new(DashMapStoreConfig::default())
    }
}

// 标记为同步存储，自动获得 Store 异步接口
impl<K, V> IsSyncStore for DashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
}

impl<K, V> SyncStore<K, V> for DashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    fn set_sync(&self, key: &K, value: &V, options: &SetOptions) -> Result<(), KvError> {
        // 检查 if_not_exist 条件
        if options.if_not_exist && self.map.contains_key(key) {
            return Err(KvError::ConditionFailed);
        }

        self.map.insert(key.clone(), value.clone());
        Ok(())
    }

    fn get_sync(&self, key: &K) -> Result<V, KvError> {
        match self.map.get(key) {
            Some(value_ref) => Ok(value_ref.clone()),
            None => Err(KvError::KeyNotFound),
        }
    }

    fn del_sync(&self, key: &K) -> Result<(), KvError> {
        self.map.remove(key);
        Ok(())
    }

    fn batch_set_sync(
        &self,
        keys: &[K],
        vals: &[V],
        options: &SetOptions,
    ) -> Result<Vec<Result<(), KvError>>, KvError> {
        if keys.len() != vals.len() {
            return Err(KvError::Other(
                "Keys and values length mismatch".to_string(),
            ));
        }

        let mut results = Vec::with_capacity(keys.len());

        for (key, value) in keys.iter().zip(vals.iter()) {
            // 检查 if_not_exist 条件
            if options.if_not_exist && self.map.contains_key(key) {
                results.push(Err(KvError::ConditionFailed));
                continue;
            }

            self.map.insert(key.clone(), value.clone());
            results.push(Ok(()));
        }

        Ok(results)
    }

    fn batch_get_sync(
        &self,
        keys: &[K],
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError> {
        let mut values = Vec::with_capacity(keys.len());
        let mut errors = Vec::with_capacity(keys.len());

        for key in keys {
            match self.map.get(key) {
                Some(value_ref) => {
                    values.push(Some(value_ref.clone()));
                    errors.push(None);
                }
                None => {
                    values.push(None);
                    errors.push(Some(KvError::KeyNotFound));
                }
            }
        }

        Ok((values, errors))
    }

    fn batch_del_sync(&self, keys: &[K]) -> Result<Vec<Result<(), KvError>>, KvError> {
        let mut results = Vec::with_capacity(keys.len());

        for key in keys {
            self.map.remove(key);
            results.push(Ok(()));
        }

        Ok(results)
    }

    fn close_sync(&self) -> Result<(), KvError> {
        // 不清空数据，只做资源清理
        Ok(())
    }
}

// 为 DashMapStore 实现 From trait
impl<K, V> From<DashMapStoreConfig> for DashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(config: DashMapStoreConfig) -> Self {
        DashMapStore::new(config)
    }
}

impl<K, V> From<Box<DashMapStore<K, V>>> for Box<dyn Store<K, V>>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<DashMapStore<K, V>>) -> Self {
        source as Box<dyn Store<K, V>>
    }
}

impl<K, V> From<Box<DashMapStore<K, V>>> for Box<dyn SyncStore<K, V>>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<DashMapStore<K, V>>) -> Self {
        source as Box<dyn SyncStore<K, V>>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::store::common_tests::*;

    // ========== 公共测试 - 异步接口 ==========

    #[tokio::test]
    async fn test_store_set() {
        let store = DashMapStore::<String, String>::new(DashMapStoreConfig::default());
        test_set(store).await;
    }

    #[tokio::test]
    async fn test_store_get() {
        let store = DashMapStore::<String, String>::new(DashMapStoreConfig::default());
        test_get(store).await;
    }

    #[tokio::test]
    async fn test_store_del() {
        let store = DashMapStore::<String, String>::new(DashMapStoreConfig::default());
        test_del(store).await;
    }

    #[tokio::test]
    async fn test_store_batch_set() {
        let store = DashMapStore::<String, i32>::new(DashMapStoreConfig::default());
        test_batch_set(store).await;
    }

    #[tokio::test]
    async fn test_store_batch_get() {
        let store = DashMapStore::<String, i32>::new(DashMapStoreConfig::default());
        test_batch_get(store).await;
    }

    #[tokio::test]
    async fn test_store_batch_del() {
        let store = DashMapStore::<String, i32>::new(DashMapStoreConfig::default());
        test_batch_del(store).await;
    }

    #[tokio::test]
    async fn test_store_close() {
        let store = DashMapStore::<String, i32>::new(DashMapStoreConfig::default());
        test_close(store).await;
    }

    // ========== 公共测试 - 同步接口 ==========

    #[test]
    fn test_store_set_sync() {
        let store = DashMapStore::<String, String>::new(DashMapStoreConfig::default());
        test_set_sync(store);
    }

    #[test]
    fn test_store_get_sync() {
        let store = DashMapStore::<String, String>::new(DashMapStoreConfig::default());
        test_get_sync(store);
    }

    #[test]
    fn test_store_del_sync() {
        let store = DashMapStore::<String, String>::new(DashMapStoreConfig::default());
        test_del_sync(store);
    }

    #[test]
    fn test_store_batch_set_sync() {
        let store = DashMapStore::<String, i32>::new(DashMapStoreConfig::default());
        test_batch_set_sync(store);
    }

    #[test]
    fn test_store_batch_get_sync() {
        let store = DashMapStore::<String, i32>::new(DashMapStoreConfig::default());
        test_batch_get_sync(store);
    }

    #[test]
    fn test_store_batch_del_sync() {
        let store = DashMapStore::<String, i32>::new(DashMapStoreConfig::default());
        test_batch_del_sync(store);
    }

    #[test]
    fn test_store_close_sync() {
        let store = DashMapStore::<String, i32>::new(DashMapStoreConfig::default());
        test_close_sync(store);
    }

    // ========== 非公共测试 ==========

    #[tokio::test]
    async fn test_store_from_json5_config() {
        // 测试从 json5 字符串创建配置
        let config_json5 = r#"{
            initial_capacity: 100,
        }"#;

        let config: DashMapStoreConfig = json5::from_str(config_json5).unwrap();
        assert_eq!(config.initial_capacity, Some(100));

        let store = DashMapStore::<String, i32>::new(config);

        // 验证 store 能正常工作
        store
            .set(&"key1".to_string(), &123, &SetOptions::new())
            .await
            .unwrap();
        let value = store.get(&"key1".to_string()).await.unwrap();
        assert_eq!(value, 123);

        // 测试空配置（使用默认值）
        let empty_config_json5 = r#"{}"#;
        let empty_config: DashMapStoreConfig = json5::from_str(empty_config_json5).unwrap();
        assert_eq!(empty_config.initial_capacity, None);

        let store2 = DashMapStore::<String, String>::new(empty_config);
        store2
            .set(&"test".to_string(), &"value".to_string(), &SetOptions::new())
            .await
            .unwrap();
        let value2 = store2.get(&"test".to_string()).await.unwrap();
        assert_eq!(value2, "value");
    }

    #[tokio::test]
    async fn test_concurrent_read_write() {
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::time::sleep;

        let store = Arc::new(DashMapStore::<String, i32>::new(DashMapStoreConfig::default()));
        let num_readers = 5;
        let num_writers = 3;
        let num_operations = 100;

        // 预先插入一些数据
        for i in 0..10 {
            store
                .set(&format!("key_{}", i), &i, &SetOptions::new())
                .await
                .unwrap();
        }

        let mut handles = Vec::new();

        // 启动多个读线程
        for reader_id in 0..num_readers {
            let store_clone = Arc::clone(&store);
            let handle = tokio::spawn(async move {
                let mut read_count = 0;
                for i in 0..num_operations {
                    let key = format!("key_{}", i % 10);
                    match store_clone.get(&key).await {
                        Ok(_) => read_count += 1,
                        Err(KvError::KeyNotFound) => {}
                        Err(e) => panic!("Reader {} failed with error: {:?}", reader_id, e),
                    }
                    if i % 10 == 0 {
                        sleep(Duration::from_millis(1)).await;
                    }
                }
                println!("Reader {} completed {} reads", reader_id, read_count);
                read_count
            });
            handles.push(handle);
        }

        // 启动多个写线程
        for writer_id in 0..num_writers {
            let store_clone = Arc::clone(&store);
            let handle = tokio::spawn(async move {
                let mut write_count = 0;
                for i in 0..num_operations {
                    let key = format!("key_{}", i % 10);
                    let value = writer_id * 1000 + i;

                    if i % 2 == 0 {
                        store_clone
                            .set(&key, &value, &SetOptions::new())
                            .await
                            .unwrap();
                        write_count += 1;
                    } else {
                        store_clone.del(&key).await.unwrap();
                        write_count += 1;
                    }

                    if i % 10 == 0 {
                        sleep(Duration::from_millis(1)).await;
                    }
                }
                println!("Writer {} completed {} writes", writer_id, write_count);
                write_count
            });
            handles.push(handle);
        }

        // 等待所有任务完成
        let mut total_reads = 0;
        let mut total_writes = 0;

        for (i, handle) in handles.into_iter().enumerate() {
            let count = handle.await.unwrap();
            if i < num_readers {
                total_reads += count;
            } else {
                total_writes += count;
            }
        }

        println!(
            "Total reads: {}, Total writes: {}",
            total_reads, total_writes
        );

        // 验证store仍然可用
        store
            .set(&"final_test".to_string(), &999, &SetOptions::new())
            .await
            .unwrap();
        let final_value = store.get(&"final_test".to_string()).await.unwrap();
        assert_eq!(final_value, 999);
    }
}
