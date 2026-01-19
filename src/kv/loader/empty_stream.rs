use crate::kv::loader::core::{Stream, LoaderError};
use crate::kv::parser::ChangeType;

/// 空 KV 数据流：不包含任何数据，用于 FileTrigger 等场景
///
/// 对应 Golang 的 EmptyKVStream[K, V]
pub struct EmptyStream<K, V> {
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V> EmptyStream<K, V> {
    /// 创建新的空数据流
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<K, V> Default for EmptyStream<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Stream<K, V> for EmptyStream<K, V>
where
    K: Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    fn each(&self, _callback: &dyn Fn(ChangeType, K, V) -> Result<(), LoaderError>) -> Result<(), LoaderError> {
        // 空实现，不调用 callback
        log::debug!("empty stream, no data to process");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_kv_stream() {
        let stream = EmptyStream::<String, String>::new();

        // 测试 each 不调用 callback
        let call_count = std::sync::Arc::new(std::sync::Mutex::new(0));
        let call_count_clone = call_count.clone();

        let result = stream.each(&|_change_type, _key, _value| {
            let mut count = call_count_clone.lock().unwrap();
            *count += 1;
            Ok(())
        });

        assert!(result.is_ok());
        assert_eq!(*call_count.lock().unwrap(), 0);
    }

    #[test]
    fn test_empty_kv_stream_default() {
        let stream = EmptyStream::<String, i32>::default();

        // 即使 callback 会返回错误，空流也不会调用它
        let result = stream.each(&|_change_type, _key, _value| {
            Err(LoaderError::LoadFailed("should not be called".to_string()))
        });

        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_kv_stream_cloned() {
        let stream1 = EmptyStream::<String, String>::new();
        let stream2 = stream1; // 应该可以移动

        // 测试新的实例也可以工作
        let result = stream2.each(&|_change_type, _key, _value| Ok(()));
        assert!(result.is_ok());
    }
}
