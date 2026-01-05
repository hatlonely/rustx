use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;

use super::core::{KvError, SetOptions, Store};

/// MapStore 配置结构体
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SafeHashMapStoreConfig {
    /// 初始容量（可选）
    #[serde(default)]
    pub initial_capacity: Option<usize>,
}

impl Default for SafeHashMapStoreConfig {
    fn default() -> Self {
        Self {
            initial_capacity: None,
        }
    }
}

/// 基于内存 HashMap 的 KV 存储实现（对应 Golang MapStore）
pub struct SafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    map: RwLock<HashMap<K, V>>,
}

impl<K, V> SafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    /// 创建新的 MapStore 实例（对应 Golang NewMapStoreWithOptions）
    pub fn new() -> Self {
        Self {
            map: RwLock::new(HashMap::new()),
        }
    }

    /// 使用配置创建新的 MapStore 实例
    pub fn with_config(config: SafeHashMapStoreConfig) -> Self {
        let initial_map = match config.initial_capacity {
            Some(capacity) => HashMap::with_capacity(capacity),
            None => HashMap::new(),
        };

        Self {
            map: RwLock::new(initial_map),
        }
    }
}

impl<K, V> Default for SafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Store<K, V> for SafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    async fn set(&self, key: K, value: V, options: SetOptions) -> Result<(), KvError> {
        let mut map = self.map.write().unwrap();

        // 检查 if_not_exist 条件
        if options.if_not_exist && map.contains_key(&key) {
            return Err(KvError::ConditionFailed);
        }

        // 注意：当前实现忽略了 expiration，因为基本的 HashMap 不支持 TTL
        // 在实际生产环境中，可以考虑使用支持 TTL 的数据结构
        map.insert(key, value);
        Ok(())
    }

    async fn get(&self, key: K) -> Result<V, KvError> {
        let map = self.map.read().unwrap();
        match map.get(&key) {
            Some(value) => Ok(value.clone()),
            None => Err(KvError::KeyNotFound),
        }
    }

    async fn del(&self, key: K) -> Result<(), KvError> {
        let mut map = self.map.write().unwrap();
        map.remove(&key);
        Ok(())
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

        let mut map = self.map.write().unwrap();
        let mut results = Vec::with_capacity(keys.len());

        for (key, value) in keys.into_iter().zip(vals.into_iter()) {
            // 检查 if_not_exist 条件
            if options.if_not_exist && map.contains_key(&key) {
                results.push(Err(KvError::ConditionFailed));
                continue;
            }

            map.insert(key, value);
            results.push(Ok(()));
        }

        Ok(results)
    }

    async fn batch_get(
        &self,
        keys: Vec<K>,
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError> {
        let map = self.map.read().unwrap();
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

    async fn batch_del(&self, keys: Vec<K>) -> Result<Vec<Result<(), KvError>>, KvError> {
        let mut map = self.map.write().unwrap();
        let mut results = Vec::with_capacity(keys.len());

        for key in keys {
            map.remove(&key);
            results.push(Ok(()));
        }

        Ok(results)
    }

    async fn close(&self) -> Result<(), KvError> {
        let mut map = self.map.write().unwrap();
        map.clear();
        Ok(())
    }
}

// 为 MapStore 实现 WithConfig trait - 这是唯一需要的！
impl<K, V> crate::cfg::config::WithConfig<SafeHashMapStoreConfig> for SafeHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn with_config(config: SafeHashMapStoreConfig) -> Self {
        SafeHashMapStore::with_config(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_map_store_basic_operations() {
        let store = SafeHashMapStore::<String, String>::new();

        // 测试 Set 和 Get
        let key = "test_key".to_string();
        let value = "test_value".to_string();

        store
            .set(key.clone(), value.clone(), SetOptions::new())
            .await
            .unwrap();
        let retrieved = store.get(key.clone()).await.unwrap();
        assert_eq!(retrieved, value);

        // 测试 Del
        store.del(key.clone()).await.unwrap();
        let result = store.get(key).await;
        assert!(matches!(result, Err(KvError::KeyNotFound)));
    }

    #[tokio::test]
    async fn test_if_not_exist() {
        let store = SafeHashMapStore::<String, String>::new();
        let key = "test_key".to_string();
        let value1 = "value1".to_string();
        let value2 = "value2".to_string();

        // 第一次设置应该成功
        store
            .set(
                key.clone(),
                value1.clone(),
                SetOptions::new().with_if_not_exist(),
            )
            .await
            .unwrap();

        // 第二次设置应该失败
        let result = store
            .set(key.clone(), value2, SetOptions::new().with_if_not_exist())
            .await;
        assert!(matches!(result, Err(KvError::ConditionFailed)));

        // 验证值没有被修改
        let retrieved = store.get(key).await.unwrap();
        assert_eq!(retrieved, value1);
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let store = SafeHashMapStore::<String, i32>::new();

        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let values = vec![1, 2, 3];

        // 批量设置
        let results = store
            .batch_set(keys.clone(), values.clone(), SetOptions::new())
            .await
            .unwrap();
        assert!(results.iter().all(|r| r.is_ok()));

        // 批量获取
        let (retrieved_values, errors) = store.batch_get(keys.clone()).await.unwrap();
        assert_eq!(retrieved_values, vec![Some(1), Some(2), Some(3)]);
        assert!(errors.iter().all(|e| e.is_none()));

        // 批量删除
        let del_results = store.batch_del(keys.clone()).await.unwrap();
        assert!(del_results.iter().all(|r| r.is_ok()));

        // 验证删除成功
        let (empty_values, not_found_errors) = store.batch_get(keys).await.unwrap();
        assert!(empty_values.iter().all(|v| v.is_none()));
        assert!(not_found_errors
            .iter()
            .all(|e| matches!(e, Some(KvError::KeyNotFound))));
    }

    #[test]
    fn test_map_store_config_default() {
        let config = SafeHashMapStoreConfig::default();
        assert_eq!(config.initial_capacity, None);
    }

    #[test]
    fn test_map_store_with_config() {
        // 测试默认配置
        let default_config = SafeHashMapStoreConfig::default();
        let _store1 = SafeHashMapStore::<String, i32>::with_config(default_config);

        // 测试带初始容量的配置
        let config_with_capacity = SafeHashMapStoreConfig {
            initial_capacity: Some(100),
        };
        let _store2 = SafeHashMapStore::<String, i32>::with_config(config_with_capacity);

        // 两个 store 都应该能正常工作
        // 这里只是验证能够创建，实际的容量测试在内部 HashMap 中
        // HashMap 不提供公开的容量检查方法，所以我们只测试基本功能
    }

    #[test]
    fn test_map_store_config_serialization() {
        let config = SafeHashMapStoreConfig {
            initial_capacity: Some(1000),
        };

        // 测试序列化
        let serialized = serde_json::to_string(&config).unwrap();
        assert!(serialized.contains("1000"));
        assert!(serialized.contains("true"));

        // 测试反序列化
        let deserialized: SafeHashMapStoreConfig = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, config);
    }

    #[test]
    fn test_with_config_trait() {
        let config = SafeHashMapStoreConfig {
            initial_capacity: Some(50),
        };

        // 测试 with_config - 现在只需要这个方法
        let store = SafeHashMapStore::<String, String>::with_config(config.clone());

        // 验证 store 被正确创建
        // 这里我们不直接测试 async 方法，只验证创建成功
        let _store_ref = &store;
    }

    #[tokio::test]
    async fn test_concurrent_read_write() {
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::time::sleep;

        let store = Arc::new(SafeHashMapStore::<String, i32>::new());
        let num_readers = 5;
        let num_writers = 3;
        let num_operations = 100;

        // 预先插入一些数据
        for i in 0..10 {
            store
                .set(format!("key_{}", i), i, SetOptions::new())
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
                    match store_clone.get(key).await {
                        Ok(_) => read_count += 1,
                        Err(KvError::KeyNotFound) => {} // 正常情况，key可能被删除
                        Err(e) => panic!("Reader {} failed with error: {:?}", reader_id, e),
                    }
                    // 添加小延迟增加并发度
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

                    // 交替进行设置和删除操作
                    if i % 2 == 0 {
                        store_clone
                            .set(key, value, SetOptions::new())
                            .await
                            .unwrap();
                        write_count += 1;
                    } else {
                        store_clone.del(key).await.unwrap();
                        write_count += 1;
                    }

                    // 添加小延迟增加并发度
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
            .set("final_test".to_string(), 999, SetOptions::new())
            .await
            .unwrap();
        let final_value = store.get("final_test".to_string()).await.unwrap();
        assert_eq!(final_value, 999);
    }
}
