use garde::Validate;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;

use super::core::{IsSyncStore, KvError, SetOptions, Store, SyncStore};

/// MapStore 配置结构体
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, SmartDefault, Validate)]
#[serde(default)]
pub struct RwLockHashMapStoreConfig {
    /// 初始容量（可选）
    #[garde(skip)]
    pub initial_capacity: Option<usize>,
}

/// 基于内存 HashMap 的 KV 存储实现（对应 Golang MapStore）
pub struct RwLockHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    map: RwLock<HashMap<K, V>>,
}

impl<K, V> RwLockHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    /// 创建新的 MapStore 实例（对应 Golang NewMapStoreWithOptions）
    pub fn new(config: RwLockHashMapStoreConfig) -> Self {
        let initial_map = match config.initial_capacity {
            Some(capacity) => HashMap::with_capacity(capacity),
            None => HashMap::new(),
        };

        Self {
            map: RwLock::new(initial_map),
        }
    }
}

impl<K, V> Default for RwLockHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    fn default() -> Self {
        Self::new(RwLockHashMapStoreConfig::default())
    }
}

// 标记为同步存储，自动获得 Store 异步接口
impl<K, V> IsSyncStore for RwLockHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
}

impl<K, V> SyncStore<K, V> for RwLockHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash,
    V: Clone + Send + Sync,
{
    fn set_sync(&self, key: &K, value: &V, options: &SetOptions) -> Result<(), KvError> {
        let mut map = self.map.write().unwrap();

        // 检查 if_not_exist 条件
        if options.if_not_exist && map.contains_key(key) {
            return Err(KvError::ConditionFailed);
        }

        // 注意：当前实现忽略了 expiration，因为基本的 HashMap 不支持 TTL
        // 在实际生产环境中，可以考虑使用支持 TTL 的数据结构
        map.insert(key.clone(), value.clone());
        Ok(())
    }

    fn get_sync(&self, key: &K) -> Result<V, KvError> {
        let map = self.map.read().unwrap();
        match map.get(key) {
            Some(value) => Ok(value.clone()),
            None => Err(KvError::KeyNotFound),
        }
    }

    fn del_sync(&self, key: &K) -> Result<(), KvError> {
        let mut map = self.map.write().unwrap();
        map.remove(key);
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

        let mut map = self.map.write().unwrap();
        let mut results = Vec::with_capacity(keys.len());

        for (key, value) in keys.iter().zip(vals.iter()) {
            // 检查 if_not_exist 条件
            if options.if_not_exist && map.contains_key(key) {
                results.push(Err(KvError::ConditionFailed));
                continue;
            }

            map.insert(key.clone(), value.clone());
            results.push(Ok(()));
        }

        Ok(results)
    }

    fn batch_get_sync(
        &self,
        keys: &[K],
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError> {
        let map = self.map.read().unwrap();
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

    fn batch_del_sync(&self, keys: &[K]) -> Result<Vec<Result<(), KvError>>, KvError> {
        let mut map = self.map.write().unwrap();
        let mut results = Vec::with_capacity(keys.len());

        for key in keys {
            map.remove(key);
            results.push(Ok(()));
        }

        Ok(results)
    }

    fn close_sync(&self) -> Result<(), KvError> {
        let mut map = self.map.write().unwrap();
        map.clear();
        Ok(())
    }
}

// 为 RwLockHashMapStore 实现 From trait - 使用标准库 trait
impl<K, V> From<RwLockHashMapStoreConfig> for RwLockHashMapStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(config: RwLockHashMapStoreConfig) -> Self {
        RwLockHashMapStore::new(config)
    }
}

impl<K, V> From<Box<RwLockHashMapStore<K, V>>> for Box<dyn Store<K, V>>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<RwLockHashMapStore<K, V>>) -> Self {
        source as Box<dyn Store<K, V>>
    }
}

impl<K, V> From<Box<RwLockHashMapStore<K, V>>> for Box<dyn SyncStore<K, V>>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<RwLockHashMapStore<K, V>>) -> Self {
        source as Box<dyn SyncStore<K, V>>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_set() {
        let store = RwLockHashMapStore::<String, String>::new(RwLockHashMapStoreConfig::default());

        // 测试基本设置
        let key = "test_key".to_string();
        let value = "test_value".to_string();
        store
            .set(&key, &value, &SetOptions::new())
            .await
            .unwrap();

        let retrieved = store.get(&key).await.unwrap();
        assert_eq!(retrieved, value);

        // 测试 if_not_exist 选项 - 第一次设置应该成功
        let key2 = "test_key2".to_string();
        let value1 = "value1".to_string();
        store
            .set(
                &key2,
                &value1,
                &SetOptions::new().with_if_not_exist(),
            )
            .await
            .unwrap();

        // 第二次设置应该失败
        let value2 = "value2".to_string();
        let result = store
            .set(&key2, &value2, &SetOptions::new().with_if_not_exist())
            .await;
        assert!(matches!(result, Err(KvError::ConditionFailed)));

        // 验证值没有被修改
        let retrieved = store.get(&key2).await.unwrap();
        assert_eq!(retrieved, value1);
    }

    #[tokio::test]
    async fn test_store_get() {
        let store = RwLockHashMapStore::<String, String>::new(RwLockHashMapStoreConfig::default());

        // 测试获取不存在的 key
        let result = store.get(&"non_existent_key".to_string()).await;
        assert!(matches!(result, Err(KvError::KeyNotFound)));

        // 测试获取存在的 key
        let key = "test_key".to_string();
        let value = "test_value".to_string();
        store
            .set(&key, &value, &SetOptions::new())
            .await
            .unwrap();

        let retrieved = store.get(&key).await.unwrap();
        assert_eq!(retrieved, value);
    }

    #[tokio::test]
    async fn test_store_del() {
        let store = RwLockHashMapStore::<String, String>::new(RwLockHashMapStoreConfig::default());

        // 测试删除存在的 key
        let key = "test_key".to_string();
        let value = "test_value".to_string();
        store
            .set(&key, &value, &SetOptions::new())
            .await
            .unwrap();

        store.del(&key).await.unwrap();

        let result = store.get(&key).await;
        assert!(matches!(result, Err(KvError::KeyNotFound)));

        // 测试删除不存在的 key - 应该返回 Ok
        let result = store.del(&"non_existent_key".to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_store_batch_set() {
        let store = RwLockHashMapStore::<String, i32>::new(RwLockHashMapStoreConfig::default());

        // 测试基本批量设置
        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let values = vec![1, 2, 3];

        let results = store
            .batch_set(&keys, &values, &SetOptions::new())
            .await
            .unwrap();
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(results.len(), 3);

        // 测试 if_not_exist 选项
        let new_keys = vec!["key1".to_string(), "key4".to_string()];
        let new_values = vec![10, 40];

        let results = store
            .batch_set(&new_keys, &new_values, &SetOptions::new().with_if_not_exist())
            .await
            .unwrap();

        assert!(matches!(&results[0], Err(KvError::ConditionFailed))); // key1 已存在
        assert!(matches!(&results[1], Ok(()))); // key4 不存在

        // 验证 key1 的值没有被修改
        let value = store.get(&"key1".to_string()).await.unwrap();
        assert_eq!(value, 1);

        // 测试长度不匹配
        let invalid_keys = vec!["key5".to_string()];
        let invalid_values = vec![5, 6];
        let result = store
            .batch_set(&invalid_keys, &invalid_values, &SetOptions::new())
            .await;
        assert!(matches!(result, Err(KvError::Other(_))));
    }

    #[tokio::test]
    async fn test_store_batch_get() {
        let store = RwLockHashMapStore::<String, i32>::new(RwLockHashMapStoreConfig::default());

        // 先设置一些数据
        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let values = vec![1, 2, 3];
        store
            .batch_set(&keys, &values, &SetOptions::new())
            .await
            .unwrap();

        // 测试批量获取
        let (retrieved_values, errors) = store.batch_get(&keys).await.unwrap();
        assert_eq!(retrieved_values, vec![Some(1), Some(2), Some(3)]);
        assert!(errors.iter().all(|e| e.is_none()));

        // 测试获取不存在的 key
        let missing_keys = vec!["key1".to_string(), "key99".to_string()];
        let (values, errors) = store.batch_get(&missing_keys).await.unwrap();
        assert_eq!(values, vec![Some(1), None]);
        assert!(errors[0].is_none());
        assert!(matches!(&errors[1], Some(KvError::KeyNotFound)));
    }

    #[tokio::test]
    async fn test_store_batch_del() {
        let store = RwLockHashMapStore::<String, i32>::new(RwLockHashMapStoreConfig::default());

        // 先设置一些数据
        let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
        let values = vec![1, 2, 3];
        store
            .batch_set(&keys, &values, &SetOptions::new())
            .await
            .unwrap();

        // 测试批量删除
        let del_results = store.batch_del(&keys).await.unwrap();
        assert!(del_results.iter().all(|r| r.is_ok()));
        assert_eq!(del_results.len(), 3);

        // 验证删除成功
        let (empty_values, not_found_errors) = store.batch_get(&keys).await.unwrap();
        assert!(empty_values.iter().all(|v| v.is_none()));
        assert!(not_found_errors
            .iter()
            .all(|e| matches!(e, Some(KvError::KeyNotFound))));

        // 测试批量删除不存在的 key - 应该返回 Ok
        let missing_keys = vec!["key99".to_string()];
        let results = store.batch_del(&missing_keys).await.unwrap();
        assert!(results[0].is_ok());
    }

    #[tokio::test]
    async fn test_store_close() {
        let store = RwLockHashMapStore::<String, i32>::new(RwLockHashMapStoreConfig::default());

        // 设置一些数据
        store
            .set(&"key1".to_string(), &1, &SetOptions::new())
            .await
            .unwrap();
        store
            .set(&"key2".to_string(), &2, &SetOptions::new())
            .await
            .unwrap();

        // 关闭 store
        store.close().await.unwrap();

        // 验证数据已被清空
        let result = store.get(&"key1".to_string()).await;
        assert!(matches!(result, Err(KvError::KeyNotFound)));

        let result = store.get(&"key2".to_string()).await;
        assert!(matches!(result, Err(KvError::KeyNotFound)));
    }

    #[tokio::test]
    async fn test_store_from_json5_config() {
        // 测试从 json5 字符串创建配置
        let config_json5 = r#"{
            initial_capacity: 100,
        }"#;

        let config: RwLockHashMapStoreConfig = json5::from_str(config_json5).unwrap();
        assert_eq!(config.initial_capacity, Some(100));

        let store = RwLockHashMapStore::<String, i32>::new(config);

        // 验证 store 能正常工作
        store
            .set(&"key1".to_string(), &123, &SetOptions::new())
            .await
            .unwrap();
        let value = store.get(&"key1".to_string()).await.unwrap();
        assert_eq!(value, 123);

        // 测试空配置（使用默认值）
        let empty_config_json5 = r#"{}"#;
        let empty_config: RwLockHashMapStoreConfig = json5::from_str(empty_config_json5).unwrap();
        assert_eq!(empty_config.initial_capacity, None);

        let store2 = RwLockHashMapStore::<String, String>::new(empty_config);
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

        let store = Arc::new(RwLockHashMapStore::<String, i32>::new(
            RwLockHashMapStoreConfig::default(),
        ));
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
