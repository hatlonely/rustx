use garde::Validate;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::hash::Hash;

use super::core::{IsSyncStore, KvError, SetOptions, Store, SyncStore};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, SmartDefault, Validate)]
#[serde(default)]
pub struct UnsafeHashMapStoreConfig {
    #[garde(skip)]
    pub initial_capacity: Option<usize>,
}

pub struct UnsafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    map: UnsafeCell<HashMap<K, V>>,
}

unsafe impl<K, V> Send for UnsafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
}

unsafe impl<K, V> Sync for UnsafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
}

impl<K, V> UnsafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    pub fn new(config: UnsafeHashMapStoreConfig) -> Self {
        let initial_map = match config.initial_capacity {
            Some(capacity) => HashMap::with_capacity(capacity),
            None => HashMap::new(),
        };

        Self {
            map: UnsafeCell::new(initial_map),
        }
    }

    unsafe fn get_map(&self) -> &HashMap<K, V> {
        &*self.map.get()
    }

    #[allow(clippy::mut_from_ref)]
    unsafe fn get_map_mut(&self) -> &mut HashMap<K, V> {
        &mut *self.map.get()
    }
}

impl<K, V> Default for UnsafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    fn default() -> Self {
        Self::new(UnsafeHashMapStoreConfig::default())
    }
}

// 标记为同步存储，自动获得 Store 异步接口
impl<K, V> IsSyncStore for UnsafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
}

impl<K, V> SyncStore<K, V> for UnsafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    fn set_sync(&self, key: &K, value: &V, options: &SetOptions) -> Result<(), KvError> {
        unsafe {
            let map = self.get_map_mut();

            if options.if_not_exist && map.contains_key(key) {
                return Err(KvError::ConditionFailed);
            }

            map.insert(key.clone(), value.clone());
            Ok(())
        }
    }

    fn get_sync(&self, key: &K) -> Result<V, KvError> {
        unsafe {
            let map = self.get_map();
            match map.get(key) {
                Some(value) => Ok(value.clone()),
                None => Err(KvError::KeyNotFound),
            }
        }
    }

    fn del_sync(&self, key: &K) -> Result<(), KvError> {
        unsafe {
            let map = self.get_map_mut();
            map.remove(key);
            Ok(())
        }
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

        unsafe {
            let map = self.get_map_mut();
            let mut results = Vec::with_capacity(keys.len());

            for (key, value) in keys.iter().zip(vals.iter()) {
                if options.if_not_exist && map.contains_key(key) {
                    results.push(Err(KvError::ConditionFailed));
                    continue;
                }

                map.insert(key.clone(), value.clone());
                results.push(Ok(()));
            }

            Ok(results)
        }
    }

    fn batch_get_sync(
        &self,
        keys: &[K],
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError> {
        unsafe {
            let map = self.get_map();
            let mut values = Vec::with_capacity(keys.len());
            let mut errors = Vec::with_capacity(keys.len());

            for key in keys {
                match map.get(key) {
                    Some(value) => {
                        values.push(Some(value.clone()));
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
    }

    fn batch_del_sync(&self, keys: &[K]) -> Result<Vec<Result<(), KvError>>, KvError> {
        unsafe {
            let map = self.get_map_mut();
            let mut results = Vec::with_capacity(keys.len());

            for key in keys {
                map.remove(key);
                results.push(Ok(()));
            }

            Ok(results)
        }
    }

    fn close_sync(&self) -> Result<(), KvError> {
        // 不清空数据，只做资源清理
        Ok(())
    }
}

impl<K, V> From<UnsafeHashMapStoreConfig> for UnsafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(config: UnsafeHashMapStoreConfig) -> Self {
        UnsafeHashMapStore::new(config)
    }
}

impl<K, V> From<Box<UnsafeHashMapStore<K, V>>> for Box<dyn Store<K, V>>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<UnsafeHashMapStore<K, V>>) -> Self {
        source as Box<dyn Store<K, V>>
    }
}

impl<K, V> From<Box<UnsafeHashMapStore<K, V>>> for Box<dyn SyncStore<K, V>>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<UnsafeHashMapStore<K, V>>) -> Self {
        source as Box<dyn SyncStore<K, V>>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::store::common_tests::*;

    // ========== 公共测试 ==========

    #[tokio::test]
    async fn test_store_set() {
        let store = UnsafeHashMapStore::<String, String>::new(UnsafeHashMapStoreConfig::default());
        test_set(store).await;
    }

    #[tokio::test]
    async fn test_store_get() {
        let store = UnsafeHashMapStore::<String, String>::new(UnsafeHashMapStoreConfig::default());
        test_get(store).await;
    }

    #[tokio::test]
    async fn test_store_del() {
        let store = UnsafeHashMapStore::<String, String>::new(UnsafeHashMapStoreConfig::default());
        test_del(store).await;
    }

    #[tokio::test]
    async fn test_store_batch_set() {
        let store = UnsafeHashMapStore::<String, i32>::new(UnsafeHashMapStoreConfig::default());
        test_batch_set(store).await;
    }

    #[tokio::test]
    async fn test_store_batch_get() {
        let store = UnsafeHashMapStore::<String, i32>::new(UnsafeHashMapStoreConfig::default());
        test_batch_get(store).await;
    }

    #[tokio::test]
    async fn test_store_batch_del() {
        let store = UnsafeHashMapStore::<String, i32>::new(UnsafeHashMapStoreConfig::default());
        test_batch_del(store).await;
    }

    #[tokio::test]
    async fn test_store_close() {
        let store = UnsafeHashMapStore::<String, i32>::new(UnsafeHashMapStoreConfig::default());
        test_close(store).await;
    }

    #[test]
    fn test_store_set_sync() {
        let store = UnsafeHashMapStore::<String, String>::new(UnsafeHashMapStoreConfig::default());
        test_set_sync(store);
    }

    #[test]
    fn test_store_get_sync() {
        let store = UnsafeHashMapStore::<String, String>::new(UnsafeHashMapStoreConfig::default());
        test_get_sync(store);
    }

    #[test]
    fn test_store_del_sync() {
        let store = UnsafeHashMapStore::<String, String>::new(UnsafeHashMapStoreConfig::default());
        test_del_sync(store);
    }

    #[test]
    fn test_store_batch_set_sync() {
        let store = UnsafeHashMapStore::<String, i32>::new(UnsafeHashMapStoreConfig::default());
        test_batch_set_sync(store);
    }

    #[test]
    fn test_store_batch_get_sync() {
        let store = UnsafeHashMapStore::<String, i32>::new(UnsafeHashMapStoreConfig::default());
        test_batch_get_sync(store);
    }

    #[test]
    fn test_store_batch_del_sync() {
        let store = UnsafeHashMapStore::<String, i32>::new(UnsafeHashMapStoreConfig::default());
        test_batch_del_sync(store);
    }

    #[test]
    fn test_store_close_sync() {
        let store = UnsafeHashMapStore::<String, i32>::new(UnsafeHashMapStoreConfig::default());
        test_close_sync(store);
    }

    // ========== 场景测试 ==========

    #[tokio::test]
    async fn test_store_from_json5_config() {
        // 测试从 json5 字符串创建配置
        let config_json5 = r#"{
            initial_capacity: 100,
        }"#;

        let config: UnsafeHashMapStoreConfig = json5::from_str(config_json5).unwrap();
        assert_eq!(config.initial_capacity, Some(100));

        let store = UnsafeHashMapStore::<String, i32>::new(config);

        // 验证 store 能正常工作
        store
            .set(&"key1".to_string(), &123, &SetOptions::new())
            .await
            .unwrap();
        let value = store.get(&"key1".to_string()).await.unwrap();
        assert_eq!(value, 123);

        // 测试空配置（使用默认值）
        let empty_config_json5 = r#"{}"#;
        let empty_config: UnsafeHashMapStoreConfig = json5::from_str(empty_config_json5).unwrap();
        assert_eq!(empty_config.initial_capacity, None);

        let store2 = UnsafeHashMapStore::<String, String>::new(empty_config);
        store2
            .set(
                &"test".to_string(),
                &"value".to_string(),
                &SetOptions::new(),
            )
            .await
            .unwrap();
        let value2 = store2.get(&"test".to_string()).await.unwrap();
        assert_eq!(value2, "value");
    }

    #[tokio::test]
    async fn test_write_once_read_many_concurrent() {
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::time::sleep;

        let store = Arc::new(UnsafeHashMapStore::<String, i32>::new(
            UnsafeHashMapStoreConfig::default(),
        ));

        // 阶段1：单线程初始化 - 写入所有数据
        println!("Phase 1: Single-threaded initialization");
        for i in 0..100 {
            store
                .set(&format!("key_{}", i), &(i * 10), &SetOptions::new())
                .await
                .unwrap();
        }

        // 批量设置一些额外数据
        let batch_keys: Vec<String> = (100..150).map(|i| format!("batch_key_{}", i)).collect();
        let batch_values: Vec<i32> = (100..150).map(|i| i * 20).collect();
        let batch_results = store
            .batch_set(&batch_keys, &batch_values, &SetOptions::new())
            .await
            .unwrap();
        assert!(batch_results.iter().all(|r| r.is_ok()));

        println!("Initialization completed. Starting concurrent read phase...");

        // 阶段2：多线程并发只读访问
        // 重要：从此刻起，绝对不能再有任何写入操作！
        let num_readers = 8;
        let reads_per_reader = 200;
        let mut handles = Vec::new();

        for reader_id in 0..num_readers {
            let store_clone = Arc::clone(&store);
            let handle = tokio::spawn(async move {
                let mut successful_reads = 0;
                let mut total_reads = 0;

                for i in 0..reads_per_reader {
                    // 随机读取不同的键
                    let key_id = i % 150;
                    let key = if key_id < 100 {
                        format!("key_{}", key_id)
                    } else {
                        format!("batch_key_{}", key_id)
                    };

                    match store_clone.get(&key).await {
                        Ok(value) => {
                            // 验证值的正确性
                            let expected = if key_id < 100 {
                                key_id * 10
                            } else {
                                key_id * 20
                            };
                            assert_eq!(value, expected);
                            successful_reads += 1;
                        }
                        Err(KvError::KeyNotFound) => {
                            panic!(
                                "Unexpected KeyNotFound for reader {}, key_id {}",
                                reader_id, key_id
                            );
                        }
                        Err(e) => {
                            panic!("Reader {} failed with error: {:?}", reader_id, e);
                        }
                    }
                    total_reads += 1;

                    // 添加小延迟模拟真实读取场景
                    if i % 20 == 0 {
                        sleep(Duration::from_micros(100)).await;
                    }
                }

                println!(
                    "Reader {} completed: {}/{} successful reads",
                    reader_id, successful_reads, total_reads
                );
                successful_reads
            });
            handles.push(handle);
        }

        // 在读取期间，主线程也进行一些批量读取操作
        let main_thread_handle = {
            let store_clone = Arc::clone(&store);
            tokio::spawn(async move {
                let batch_keys: Vec<String> = (0..50).map(|i| format!("key_{}", i)).collect();

                for _ in 0..10 {
                    let (values, errors) = store_clone.batch_get(&batch_keys).await.unwrap();

                    // 验证所有值都能正确读取
                    assert_eq!(values.len(), 50);
                    assert!(errors.iter().all(|e| e.is_none()));

                    for (i, value_opt) in values.iter().enumerate() {
                        if let Some(value) = value_opt {
                            assert_eq!(*value, (i as i32) * 10);
                        } else {
                            panic!("Unexpected None value at index {}", i);
                        }
                    }

                    sleep(Duration::from_millis(1)).await;
                }

                println!("Main thread batch reads completed");
                50 // 返回处理的键数量
            })
        };

        // 等待所有读取任务完成
        let mut total_reads = 0;
        for handle in handles {
            let reads = handle.await.unwrap();
            total_reads += reads;
        }

        let main_reads = main_thread_handle.await.unwrap();
        total_reads += main_reads;

        println!(
            "All concurrent reads completed. Total successful reads: {}",
            total_reads
        );

        // 最终验证：确保数据完整性没有被破坏
        for i in 0..100 {
            let value = store.get(&format!("key_{}", i)).await.unwrap();
            assert_eq!(value, i * 10);
        }

        for i in 100..150 {
            let value = store.get(&format!("batch_key_{}", i)).await.unwrap();
            assert_eq!(value, i * 20);
        }

        println!("Final integrity check passed!");
    }
}
