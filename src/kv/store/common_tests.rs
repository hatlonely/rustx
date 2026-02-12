//! Store 接口通用测试模块
//!
//! 提供针对 Store trait 各个接口方法的通用测试函数

#[cfg(test)]
use super::core::{KvError, SetOptions, Store, SyncStore};

/// 测试 `set` 方法
///
/// 测试内容:
/// - 基本设置和获取
/// - `if_not_exist` 选项（第一次成功，第二次失败）
/// - 验证值不被修改
#[cfg(test)]
pub async fn test_set<S>(store: S)
where
    S: Store<String, String>,
{
    // 测试基本设置
    let key = "test_key".to_string();
    let value = "test_value".to_string();
    store.set(&key, &value, &SetOptions::new()).await.unwrap();

    let retrieved = store.get(&key).await.unwrap();
    assert_eq!(retrieved, value);

    // 测试 if_not_exist 选项 - 第一次设置应该成功
    let key2 = "test_key2".to_string();
    let value1 = "value1".to_string();
    store
        .set(&key2, &value1, &SetOptions::new().with_if_not_exist())
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

/// 同步版本的 `set` 方法测试
#[cfg(test)]
pub fn test_set_sync<S>(store: S)
where
    S: SyncStore<String, String>,
{
    // 测试基本设置
    let key = "test_key".to_string();
    let value = "test_value".to_string();
    store.set_sync(&key, &value, &SetOptions::new()).unwrap();

    let retrieved = store.get_sync(&key).unwrap();
    assert_eq!(retrieved, value);

    // 测试 if_not_exist 选项 - 第一次设置应该成功
    let key2 = "test_key2".to_string();
    let value1 = "value1".to_string();
    store
        .set_sync(&key2, &value1, &SetOptions::new().with_if_not_exist())
        .unwrap();

    // 第二次设置应该失败
    let value2 = "value2".to_string();
    let result = store
        .set_sync(&key2, &value2, &SetOptions::new().with_if_not_exist());
    assert!(matches!(result, Err(KvError::ConditionFailed)));

    // 验证值没有被修改
    let retrieved = store.get_sync(&key2).unwrap();
    assert_eq!(retrieved, value1);
}

/// 测试 `get` 方法
///
/// 测试内容:
/// - 获取不存在的 key 返回 KeyNotFound
/// - 获取存在的 key 返回正确值
#[cfg(test)]
pub async fn test_get<S>(store: S)
where
    S: Store<String, String>,
{
    // 测试获取不存在的 key
    let result = store.get(&"non_existent_key".to_string()).await;
    assert!(matches!(result, Err(KvError::KeyNotFound)));

    // 测试获取存在的 key
    let key = "test_key".to_string();
    let value = "test_value".to_string();
    store.set(&key, &value, &SetOptions::new()).await.unwrap();

    let retrieved = store.get(&key).await.unwrap();
    assert_eq!(retrieved, value);
}

/// 同步版本的 `get` 方法测试
#[cfg(test)]
pub fn test_get_sync<S>(store: S)
where
    S: SyncStore<String, String>,
{
    // 测试获取不存在的 key
    let result = store.get_sync(&"non_existent_key".to_string());
    assert!(matches!(result, Err(KvError::KeyNotFound)));

    // 测试获取存在的 key
    let key = "test_key".to_string();
    let value = "test_value".to_string();
    store.set_sync(&key, &value, &SetOptions::new()).unwrap();

    let retrieved = store.get_sync(&key).unwrap();
    assert_eq!(retrieved, value);
}

/// 测试 `del` 方法
///
/// 测试内容:
/// - 删除存在的 key
/// - 删除不存在的 key 返回 Ok
#[cfg(test)]
pub async fn test_del<S>(store: S)
where
    S: Store<String, String>,
{
    // 测试删除存在的 key
    let key = "test_key".to_string();
    let value = "test_value".to_string();
    store.set(&key, &value, &SetOptions::new()).await.unwrap();

    store.del(&key).await.unwrap();

    let result = store.get(&key).await;
    assert!(matches!(result, Err(KvError::KeyNotFound)));

    // 测试删除不存在的 key - 应该返回 Ok
    let result = store.del(&"non_existent_key".to_string()).await;
    assert!(result.is_ok());
}

/// 同步版本的 `del` 方法测试
#[cfg(test)]
pub fn test_del_sync<S>(store: S)
where
    S: SyncStore<String, String>,
{
    // 测试删除存在的 key
    let key = "test_key".to_string();
    let value = "test_value".to_string();
    store.set_sync(&key, &value, &SetOptions::new()).unwrap();

    store.del_sync(&key).unwrap();

    let result = store.get_sync(&key);
    assert!(matches!(result, Err(KvError::KeyNotFound)));

    // 测试删除不存在的 key - 应该返回 Ok
    let result = store.del_sync(&"non_existent_key".to_string());
    assert!(result.is_ok());
}

/// 测试 `batch_set` 方法
///
/// 测试内容:
/// - 基本批量设置
/// - `if_not_exist` 选项
/// - 验证值不被修改
/// - 长度不匹配错误
#[cfg(test)]
pub async fn test_batch_set<S>(store: S)
where
    S: Store<String, i32>,
{
    // 测试基本批量设置
    let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
    let values = vec![1, 2, 3];

    let results = store.batch_set(&keys, &values, &SetOptions::new()).await.unwrap();
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
    let result = store.batch_set(&invalid_keys, &invalid_values, &SetOptions::new()).await;
    assert!(matches!(result, Err(KvError::Other(_))));
}

/// 同步版本的 `batch_set` 方法测试
#[cfg(test)]
pub fn test_batch_set_sync<S>(store: S)
where
    S: SyncStore<String, i32>,
{
    // 测试基本批量设置
    let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
    let values = vec![1, 2, 3];

    let results = store
        .batch_set_sync(&keys, &values, &SetOptions::new())
        .unwrap();
    assert!(results.iter().all(|r| r.is_ok()));
    assert_eq!(results.len(), 3);

    // 测试 if_not_exist 选项
    let new_keys = vec!["key1".to_string(), "key4".to_string()];
    let new_values = vec![10, 40];

    let results = store
        .batch_set_sync(&new_keys, &new_values, &SetOptions::new().with_if_not_exist())
        .unwrap();

    assert!(matches!(&results[0], Err(KvError::ConditionFailed))); // key1 已存在
    assert!(matches!(&results[1], Ok(()))); // key4 不存在

    // 验证 key1 的值没有被修改
    let value = store.get_sync(&"key1".to_string()).unwrap();
    assert_eq!(value, 1);

    // 测试长度不匹配
    let invalid_keys = vec!["key5".to_string()];
    let invalid_values = vec![5, 6];
    let result = store.batch_set_sync(&invalid_keys, &invalid_values, &SetOptions::new());
    assert!(matches!(result, Err(KvError::Other(_))));
}

/// 测试 `batch_get` 方法
///
/// 测试内容:
/// - 批量获取存在的 key
/// - 批量获取部分不存在的 key
#[cfg(test)]
pub async fn test_batch_get<S>(store: S)
where
    S: Store<String, i32>,
{
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

/// 同步版本的 `batch_get` 方法测试
#[cfg(test)]
pub fn test_batch_get_sync<S>(store: S)
where
    S: SyncStore<String, i32>,
{
    // 先设置一些数据
    let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
    let values = vec![1, 2, 3];
    store
        .batch_set_sync(&keys, &values, &SetOptions::new())
        .unwrap();

    // 测试批量获取
    let (retrieved_values, errors) = store.batch_get_sync(&keys).unwrap();
    assert_eq!(retrieved_values, vec![Some(1), Some(2), Some(3)]);
    assert!(errors.iter().all(|e| e.is_none()));

    // 测试获取不存在的 key
    let missing_keys = vec!["key1".to_string(), "key99".to_string()];
    let (values, errors) = store.batch_get_sync(&missing_keys).unwrap();
    assert_eq!(values, vec![Some(1), None]);
    assert!(errors[0].is_none());
    assert!(matches!(&errors[1], Some(KvError::KeyNotFound)));
}

/// 测试 `batch_del` 方法
///
/// 测试内容:
/// - 批量删除存在的 key
/// - 批量删除不存在的 key
#[cfg(test)]
pub async fn test_batch_del<S>(store: S)
where
    S: Store<String, i32>,
{
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

/// 同步版本的 `batch_del` 方法测试
#[cfg(test)]
pub fn test_batch_del_sync<S>(store: S)
where
    S: SyncStore<String, i32>,
{
    // 先设置一些数据
    let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
    let values = vec![1, 2, 3];
    store
        .batch_set_sync(&keys, &values, &SetOptions::new())
        .unwrap();

    // 测试批量删除
    let del_results = store.batch_del_sync(&keys).unwrap();
    assert!(del_results.iter().all(|r| r.is_ok()));
    assert_eq!(del_results.len(), 3);

    // 验证删除成功
    let (empty_values, not_found_errors) = store.batch_get_sync(&keys).unwrap();
    assert!(empty_values.iter().all(|v| v.is_none()));
    assert!(not_found_errors
        .iter()
        .all(|e| matches!(e, Some(KvError::KeyNotFound))));

    // 测试批量删除不存在的 key - 应该返回 Ok
    let missing_keys = vec!["key99".to_string()];
    let results = store.batch_del_sync(&missing_keys).unwrap();
    assert!(results[0].is_ok());
}

/// 测试 `close` 方法
///
/// 测试内容:
/// - 关闭后数据仍然存在（不清空数据）
#[cfg(test)]
pub async fn test_close<S>(store: S)
where
    S: Store<String, i32>,
{
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

    // 验证数据仍然存在
    let result = store.get(&"key1".to_string()).await;
    assert!(matches!(result, Ok(1)));

    let result = store.get(&"key2".to_string()).await;
    assert!(matches!(result, Ok(2)));
}

/// 同步版本的 `close` 方法测试
#[cfg(test)]
pub fn test_close_sync<S>(store: S)
where
    S: SyncStore<String, i32>,
{
    // 设置一些数据
    store
        .set_sync(&"key1".to_string(), &1, &SetOptions::new())
        .unwrap();
    store
        .set_sync(&"key2".to_string(), &2, &SetOptions::new())
        .unwrap();

    // 关闭 store
    store.close_sync().unwrap();

    // 验证数据仍然存在
    let result = store.get_sync(&"key1".to_string());
    assert!(matches!(result, Ok(1)));

    let result = store.get_sync(&"key2".to_string());
    assert!(matches!(result, Ok(2)));
}
