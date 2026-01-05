use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, Ordering};

use super::core::{KvError, SetOptions, Store};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HashMapStoreConfig {
    #[serde(default)]
    pub initial_capacity: Option<usize>,
}

impl Default for HashMapStoreConfig {
    fn default() -> Self {
        Self {
            initial_capacity: None,
        }
    }
}

pub struct HashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    map: UnsafeCell<HashMap<K, V>>,
    _closed: AtomicBool,
}

unsafe impl<K, V> Send for HashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{}

unsafe impl<K, V> Sync for HashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{}

impl<K, V> HashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    pub fn new() -> Self {
        Self {
            map: UnsafeCell::new(HashMap::new()),
            _closed: AtomicBool::new(false),
        }
    }

    pub fn with_config(config: HashMapStoreConfig) -> Self {
        let initial_map = match config.initial_capacity {
            Some(capacity) => HashMap::with_capacity(capacity),
            None => HashMap::new(),
        };

        Self {
            map: UnsafeCell::new(initial_map),
            _closed: AtomicBool::new(false),
        }
    }

    unsafe fn get_map(&self) -> &HashMap<K, V> {
        &*self.map.get()
    }

    unsafe fn get_map_mut(&self) -> &mut HashMap<K, V> {
        &mut *self.map.get()
    }
}

impl<K, V> Default for HashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Store<K, V> for HashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    async fn set(&self, key: K, value: V, options: SetOptions) -> Result<(), KvError> {
        unsafe {
            let map = self.get_map_mut();

            if options.if_not_exist && map.contains_key(&key) {
                return Err(KvError::ConditionFailed);
            }

            map.insert(key, value);
            Ok(())
        }
    }

    async fn get(&self, key: K) -> Result<V, KvError> {
        unsafe {
            let map = self.get_map();
            match map.get(&key) {
                Some(value) => Ok(value.clone()),
                None => Err(KvError::KeyNotFound),
            }
        }
    }

    async fn del(&self, key: K) -> Result<(), KvError> {
        unsafe {
            let map = self.get_map_mut();
            map.remove(&key);
            Ok(())
        }
    }

    async fn batch_set(
        &self,
        keys: Vec<K>,
        vals: Vec<V>,
        options: SetOptions,
    ) -> Result<Vec<Result<(), KvError>>, KvError> {
        if keys.len() != vals.len() {
            return Err(KvError::Other(
                "Keys and values length mismatch".to_string(),
            ));
        }

        unsafe {
            let map = self.get_map_mut();
            let mut results = Vec::with_capacity(keys.len());

            for (key, value) in keys.into_iter().zip(vals.into_iter()) {
                if options.if_not_exist && map.contains_key(&key) {
                    results.push(Err(KvError::ConditionFailed));
                    continue;
                }

                map.insert(key, value);
                results.push(Ok(()));
            }

            Ok(results)
        }
    }

    async fn batch_get(
        &self,
        keys: Vec<K>,
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError> {
        unsafe {
            let map = self.get_map();
            let mut values = Vec::with_capacity(keys.len());
            let mut errors = Vec::with_capacity(keys.len());

            for key in keys {
                match map.get(&key) {
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

    async fn batch_del(&self, keys: Vec<K>) -> Result<Vec<Result<(), KvError>>, KvError> {
        unsafe {
            let map = self.get_map_mut();
            let mut results = Vec::with_capacity(keys.len());

            for key in keys {
                map.remove(&key);
                results.push(Ok(()));
            }

            Ok(results)
        }
    }

    async fn close(&self) -> Result<(), KvError> {
        unsafe {
            let map = self.get_map_mut();
            map.clear();
            self._closed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
}

impl<K, V> crate::cfg::config::WithConfig<HashMapStoreConfig> for HashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn with_config(config: HashMapStoreConfig) -> Self {
        HashMapStore::with_config(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_map_store_basic_operations() {
        let store = HashMapStore::<String, String>::new();

        let key = "test_key".to_string();
        let value = "test_value".to_string();

        store
            .set(key.clone(), value.clone(), SetOptions::new())
            .await
            .unwrap();
        let retrieved = store.get(key.clone()).await.unwrap();
        assert_eq!(retrieved, value);

        store.del(key.clone()).await.unwrap();
        let result = store.get(key).await;
        assert!(matches!(result, Err(KvError::KeyNotFound)));
    }

    #[tokio::test]
    async fn test_if_not_exist() {
        let store = HashMapStore::<String, String>::new();
        let key = "test_key".to_string();
        let value1 = "value1".to_string();
        let value2 = "value2".to_string();

        store
            .set(
                key.clone(),
                value1.clone(),
                SetOptions::new().with_if_not_exist(),
            )
            .await
            .unwrap();

        let result = store
            .set(key.clone(), value2, SetOptions::new().with_if_not_exist())
            .await;
        assert!(matches!(result, Err(KvError::ConditionFailed)));

        let retrieved = store.get(key).await.unwrap();
        assert_eq!(retrieved, value1);
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let store = HashMapStore::<String, i32>::new();

        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let values = vec![1, 2, 3];

        let results = store
            .batch_set(keys.clone(), values.clone(), SetOptions::new())
            .await
            .unwrap();
        assert!(results.iter().all(|r| r.is_ok()));

        let (retrieved_values, errors) = store.batch_get(keys.clone()).await.unwrap();
        assert_eq!(retrieved_values, vec![Some(1), Some(2), Some(3)]);
        assert!(errors.iter().all(|e| e.is_none()));

        let del_results = store.batch_del(keys.clone()).await.unwrap();
        assert!(del_results.iter().all(|r| r.is_ok()));

        let (empty_values, not_found_errors) = store.batch_get(keys).await.unwrap();
        assert!(empty_values.iter().all(|v| v.is_none()));
        assert!(not_found_errors
            .iter()
            .all(|e| matches!(e, Some(KvError::KeyNotFound))));
    }

    #[test]
    fn test_map_store_config_default() {
        let config = HashMapStoreConfig::default();
        assert_eq!(config.initial_capacity, None);
    }

    #[test]
    fn test_map_store_with_config() {
        let default_config = HashMapStoreConfig::default();
        let _store1 = HashMapStore::<String, i32>::with_config(default_config);

        let config_with_capacity = HashMapStoreConfig {
            initial_capacity: Some(100),
        };
        let _store2 = HashMapStore::<String, i32>::with_config(config_with_capacity);
    }

    #[test]
    fn test_map_store_config_serialization() {
        let config = HashMapStoreConfig {
            initial_capacity: Some(1000),
        };

        let serialized = serde_json::to_string(&config).unwrap();
        assert!(serialized.contains("1000"));

        let deserialized: HashMapStoreConfig = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, config);
    }

    #[test]
    fn test_with_config_trait() {
        let config = HashMapStoreConfig {
            initial_capacity: Some(50),
        };

        let store = HashMapStore::<String, String>::with_config(config.clone());
        let _store_ref = &store;
    }

    #[tokio::test]
    async fn test_write_once_read_many_concurrent() {
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::time::sleep;

        let store = Arc::new(HashMapStore::<String, i32>::new());
        
        // 阶段1：单线程初始化 - 写入所有数据
        println!("Phase 1: Single-threaded initialization");
        for i in 0..100 {
            store
                .set(format!("key_{}", i), i * 10, SetOptions::new())
                .await
                .unwrap();
        }
        
        // 批量设置一些额外数据
        let batch_keys: Vec<String> = (100..150).map(|i| format!("batch_key_{}", i)).collect();
        let batch_values: Vec<i32> = (100..150).map(|i| i * 20).collect();
        let batch_results = store
            .batch_set(batch_keys.clone(), batch_values, SetOptions::new())
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
                    
                    match store_clone.get(key).await {
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
                            panic!("Unexpected KeyNotFound for reader {}, key_id {}", reader_id, key_id);
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
                
                println!("Reader {} completed: {}/{} successful reads", 
                    reader_id, successful_reads, total_reads);
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
                    let (values, errors) = store_clone.batch_get(batch_keys.clone()).await.unwrap();
                    
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
        
        println!("All concurrent reads completed. Total successful reads: {}", total_reads);
        
        // 最终验证：确保数据完整性没有被破坏
        for i in 0..100 {
            let value = store.get(format!("key_{}", i)).await.unwrap();
            assert_eq!(value, i * 10);
        }
        
        for i in 100..150 {
            let value = store.get(format!("batch_key_{}", i)).await.unwrap();
            assert_eq!(value, i * 20);
        }
        
        println!("Final integrity check passed!");
    }
}