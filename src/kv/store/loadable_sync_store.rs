use arc_swap::ArcSwap;
use garde::Validate;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use crate::cfg::{create_trait_from_type_options, TypeOptions};
use crate::kv::loader::core::{
    Listener, Loader, LoaderError, Stream, LOAD_STRATEGY_INPLACE, LOAD_STRATEGY_REPLACE,
};
use crate::kv::parser::ChangeType;

use super::core::{IsSyncStore, KvError, SetOptions, Store, AsyncStore, SyncStore};

/// LoadableSyncStore 配置
#[derive(Debug, Clone, Serialize, Deserialize, SmartDefault, Validate)]
#[serde(default)]
pub struct LoadableSyncStoreConfig {
    /// 底层 SyncStore 配置
    #[garde(skip)]
    pub store: TypeOptions,

    /// Loader 配置
    #[garde(skip)]
    pub loader: TypeOptions,

    /// 加载策略: "inplace" 或 "replace"
    #[default = "inplace"]
    #[garde(pattern("inplace|replace"))]
    pub load_strategy: String,
}

/// 可从外部数据源加载数据的 SyncStore 装饰器
///
/// 通过 Loader 监听数据变更，支持两种加载策略：
/// - InPlace: 增量更新，直接在当前 store 上 set/del
/// - Replace: 全量替换，创建新 store 加载完数据后原子替换旧 store
pub struct LoadableSyncStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    store: Arc<ArcSwap<Box<dyn SyncStore<K, V>>>>,
    loader: Mutex<Box<dyn Loader<K, V>>>,
}

impl<K, V> LoadableSyncStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    pub fn new(config: LoadableSyncStoreConfig) -> Result<Self, anyhow::Error> {
        // 使用 garde 验证配置
        if let Err(errors) = config.validate() {
            return Err(anyhow::anyhow!(
                "configuration validation failed: {}",
                errors
            ));
        }

        let store: Box<dyn SyncStore<K, V>> = create_trait_from_type_options(&config.store)?;
        let store = Arc::new(ArcSwap::from_pointee(store));

        let mut loader: Box<dyn Loader<K, V>> = create_trait_from_type_options(&config.loader)?;

        let load_strategy = config.load_strategy.clone();
        let store_config = config.store.clone();
        let store_clone = Arc::clone(&store);

        let listener: Listener<K, V> =
            Arc::new(
                move |stream: Arc<dyn Stream<K, V>>| match load_strategy.as_str() {
                    LOAD_STRATEGY_INPLACE => handle_inplace_load(&store_clone, &stream),
                    LOAD_STRATEGY_REPLACE => {
                        handle_replace_load(&store_clone, &store_config, &stream)
                    }
                    _ => Err(LoaderError::LoadFailed(format!(
                        "unknown load strategy: {}",
                        load_strategy
                    ))),
                },
            );

        loader.on_change(listener)?;

        Ok(Self {
            store,
            loader: Mutex::new(loader),
        })
    }
}

/// InPlace 策略：增量更新当前 store
fn handle_inplace_load<K, V>(
    store: &Arc<ArcSwap<Box<dyn SyncStore<K, V>>>>,
    stream: &Arc<dyn Stream<K, V>>,
) -> Result<(), LoaderError>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    let current = store.load();
    stream.each(&|change_type, key, val| match change_type {
        ChangeType::Add | ChangeType::Update | ChangeType::Unknown => current
            .set_sync(&key, &val, &SetOptions::new())
            .map_err(|e| LoaderError::LoadFailed(format!("set failed: {}", e))),
        ChangeType::Delete => current
            .del_sync(&key)
            .map_err(|e| LoaderError::LoadFailed(format!("del failed: {}", e))),
    })
}

/// Replace 策略：创建新 store，加载完数据后原子替换
fn handle_replace_load<K, V>(
    store: &Arc<ArcSwap<Box<dyn SyncStore<K, V>>>>,
    store_config: &TypeOptions,
    stream: &Arc<dyn Stream<K, V>>,
) -> Result<(), LoaderError>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    let new_store: Box<dyn SyncStore<K, V>> = create_trait_from_type_options(store_config)
        .map_err(|e| LoaderError::LoadFailed(format!("failed to create new store: {}", e)))?;

    stream.each(&|change_type, key, val| match change_type {
        ChangeType::Add | ChangeType::Update | ChangeType::Unknown => new_store
            .set_sync(&key, &val, &SetOptions::new())
            .map_err(|e| LoaderError::LoadFailed(format!("set failed: {}", e))),
        ChangeType::Delete => Ok(()),
    })?;

    // 原子替换
    let old = store.swap(Arc::new(new_store));
    // 关闭旧 store
    let _ = old.close_sync();

    Ok(())
}

impl<K, V> IsSyncStore for LoadableSyncStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
}

impl<K, V> SyncStore<K, V> for LoadableSyncStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn set_sync(&self, key: &K, value: &V, options: &SetOptions) -> Result<(), KvError> {
        self.store.load().set_sync(key, value, options)
    }

    fn get_sync(&self, key: &K) -> Result<V, KvError> {
        self.store.load().get_sync(key)
    }

    fn del_sync(&self, key: &K) -> Result<(), KvError> {
        self.store.load().del_sync(key)
    }

    fn batch_set_sync(
        &self,
        keys: &[K],
        vals: &[V],
        options: &SetOptions,
    ) -> Result<Vec<Result<(), KvError>>, KvError> {
        self.store.load().batch_set_sync(keys, vals, options)
    }

    fn batch_get_sync(
        &self,
        keys: &[K],
    ) -> Result<(Vec<Option<V>>, Vec<Option<KvError>>), KvError> {
        self.store.load().batch_get_sync(keys)
    }

    fn batch_del_sync(&self, keys: &[K]) -> Result<Vec<Result<(), KvError>>, KvError> {
        self.store.load().batch_del_sync(keys)
    }

    fn close_sync(&self) -> Result<(), KvError> {
        self.loader
            .lock()
            .unwrap()
            .close()
            .map_err(|e| KvError::Other(e.to_string()))?;
        self.store.load().close_sync()
    }
}

impl<K, V> From<LoadableSyncStoreConfig> for LoadableSyncStore<K, V>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(config: LoadableSyncStoreConfig) -> Self {
        LoadableSyncStore::new(config).unwrap()
    }
}

impl<K, V> From<Box<LoadableSyncStore<K, V>>> for Box<dyn SyncStore<K, V>>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<LoadableSyncStore<K, V>>) -> Self {
        source as Box<dyn SyncStore<K, V>>
    }
}

impl<K, V> From<Box<LoadableSyncStore<K, V>>> for Box<dyn AsyncStore<K, V>>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<LoadableSyncStore<K, V>>) -> Self {
        source as Box<dyn AsyncStore<K, V>>
    }
}

impl<K, V> From<Box<LoadableSyncStore<K, V>>> for Box<dyn Store<K, V>>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn from(source: Box<LoadableSyncStore<K, V>>) -> Self {
        source as Box<dyn Store<K, V>>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::loader::register::register_loaders;
    use crate::kv::parser::register_parsers;
    use crate::kv::store::common_tests::*;
    use crate::kv::store::register_hash_stores;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn setup() -> Result<(), anyhow::Error> {
        let _ = register_parsers::<String, String>();
        let _ = register_loaders::<String, String>();
        let _ = register_hash_stores::<String, String>();
        Ok(())
    }

    fn setup_i32() -> Result<(), anyhow::Error> {
        let _ = register_parsers::<String, i32>();
        let _ = register_loaders::<String, i32>();
        let _ = register_hash_stores::<String, i32>();
        Ok(())
    }

    fn create_temp_file(lines: &[&str]) -> NamedTempFile {
        let mut temp_file = NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(temp_file, "{}", line).unwrap();
        }
        temp_file.flush().unwrap();
        temp_file
    }

    fn make_config(store_type: &str, file_path: &str, strategy: &str) -> LoadableSyncStoreConfig {
        json5::from_str(&format!(
            r#"{{
                store: {{
                    type: "{}",
                    options: {{}}
                }},
                loader: {{
                    type: "KvFileLoader",
                    options: {{
                        file_path: "{}",
                        parser: {{
                            type: "LineParser",
                            options: {{
                                separator: "\t"
                            }}
                        }}
                    }}
                }},
                load_strategy: "{}"
            }}"#,
            store_type, file_path, strategy
        ))
        .unwrap()
    }

    // 辅助函数：创建用于测试的 store
    fn make_store_string() -> Result<LoadableSyncStore<String, String>, anyhow::Error> {
        setup()?;
        let temp_file = create_temp_file(&[]);
        let config = make_config(
            "RwLockHashMapStore",
            temp_file.path().to_str().unwrap(),
            "inplace",
        );
        LoadableSyncStore::new(config)
    }

    fn make_store_i32() -> Result<LoadableSyncStore<String, i32>, anyhow::Error> {
        setup_i32()?;
        let temp_file = create_temp_file(&[]);
        let config = make_config(
            "RwLockHashMapStore",
            temp_file.path().to_str().unwrap(),
            "inplace",
        );
        LoadableSyncStore::new(config)
    }

    // ===== 公共测试 =====

    #[tokio::test]
    async fn test_store_set() -> Result<(), anyhow::Error> {
        let store = make_store_string()?;
        test_set(store).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_store_get() -> Result<(), anyhow::Error> {
        let store = make_store_string()?;
        test_get(store).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_store_del() -> Result<(), anyhow::Error> {
        let store = make_store_string()?;
        test_del(store).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_store_batch_set() -> Result<(), anyhow::Error> {
        let store = make_store_i32()?;
        test_batch_set(store).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_store_batch_get() -> Result<(), anyhow::Error> {
        let store = make_store_i32()?;
        test_batch_get(store).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_store_batch_del() -> Result<(), anyhow::Error> {
        let store = make_store_i32()?;
        test_batch_del(store).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_store_close() -> Result<(), anyhow::Error> {
        let store = make_store_i32()?;
        test_close(store).await;
        Ok(())
    }

    #[test]
    fn test_store_set_sync() -> Result<(), anyhow::Error> {
        let store = make_store_string()?;
        test_set_sync(store);
        Ok(())
    }

    #[test]
    fn test_store_get_sync() -> Result<(), anyhow::Error> {
        let store = make_store_string()?;
        test_get_sync(store);
        Ok(())
    }

    #[test]
    fn test_store_del_sync() -> Result<(), anyhow::Error> {
        let store = make_store_string()?;
        test_del_sync(store);
        Ok(())
    }

    #[test]
    fn test_store_batch_set_sync() -> Result<(), anyhow::Error> {
        let store = make_store_i32()?;
        test_batch_set_sync(store);
        Ok(())
    }

    #[test]
    fn test_store_batch_get_sync() -> Result<(), anyhow::Error> {
        let store = make_store_i32()?;
        test_batch_get_sync(store);
        Ok(())
    }

    #[test]
    fn test_store_batch_del_sync() -> Result<(), anyhow::Error> {
        let store = make_store_i32()?;
        test_batch_del_sync(store);
        Ok(())
    }

    #[test]
    fn test_store_close_sync() -> Result<(), anyhow::Error> {
        let store = make_store_i32()?;
        test_close_sync(store);
        Ok(())
    }

    // ===== 场景测试 =====

    #[test]
    fn test_loadable_sync_store_inplace() -> Result<(), anyhow::Error> {
        setup()?;

        let temp_file = create_temp_file(&["k1\tv1", "k2\tv2"]);
        let config = make_config(
            "RwLockHashMapStore",
            temp_file.path().to_str().unwrap(),
            "inplace",
        );

        let store = LoadableSyncStore::<String, String>::new(config)?;

        assert_eq!(store.get_sync(&"k1".to_string())?, "v1");
        assert_eq!(store.get_sync(&"k2".to_string())?, "v2");
        assert!(matches!(
            store.get_sync(&"k3".to_string()),
            Err(KvError::KeyNotFound)
        ));

        Ok(())
    }

    #[test]
    fn test_loadable_sync_store_replace() -> Result<(), anyhow::Error> {
        setup()?;

        let temp_file = create_temp_file(&["k1\tv1", "k2\tv2"]);
        let config = make_config(
            "DashMapStore",
            temp_file.path().to_str().unwrap(),
            "replace",
        );

        let store = LoadableSyncStore::<String, String>::new(config)?;

        assert_eq!(store.get_sync(&"k1".to_string())?, "v1");
        assert_eq!(store.get_sync(&"k2".to_string())?, "v2");

        Ok(())
    }

    #[test]
    fn test_create_from_type_options_as_sync_store() -> Result<(), anyhow::Error> {
        use crate::cfg::{create_trait_from_type_options, TypeOptions};

        setup()?;

        let temp_file = create_temp_file(&["k1\tv1", "k2\tv2"]);

        let opts = TypeOptions::from_json(&format!(
            r#"{{
                    "type": "LoadableSyncStore",
                    "options": {{
                        "store": {{
                            "type": "RwLockHashMapStore",
                            "options": {{}}
                        }},
                        "loader": {{
                            "type": "KvFileLoader",
                            "options": {{
                                "file_path": "{}",
                                "parser": {{
                                    "type": "LineParser",
                                    "options": {{ "separator": "\t" }}
                                }}
                            }}
                        }},
                        "load_strategy": "inplace"
                    }}
                }}"#,
            temp_file.path().to_str().unwrap()
        ))?;

        let store: Box<dyn SyncStore<String, String>> = create_trait_from_type_options(&opts)?;

        assert_eq!(store.get_sync(&"k1".to_string())?, "v1");
        assert_eq!(store.get_sync(&"k2".to_string())?, "v2");

        Ok(())
    }

    #[tokio::test]
    async fn test_create_from_type_options_as_store() -> Result<(), anyhow::Error> {
        use crate::cfg::{create_trait_from_type_options, TypeOptions};
        use crate::kv::store::register_hash_stores;

        setup()?;
        let _ = register_hash_stores::<String, String>();

        let temp_file = create_temp_file(&["k1\tv1", "k2\tv2"]);

        let opts = TypeOptions::from_json(&format!(
            r#"{{
                    "type": "LoadableSyncStore",
                    "options": {{
                        "store": {{
                            "type": "DashMapStore",
                            "options": {{}}
                        }},
                        "loader": {{
                            "type": "KvFileLoader",
                            "options": {{
                                "file_path": "{}",
                                "parser": {{
                                    "type": "LineParser",
                                    "options": {{ "separator": "\t" }}
                                }}
                            }}
                        }},
                        "load_strategy": "replace"
                    }}
                }}"#,
            temp_file.path().to_str().unwrap()
        ))?;

        let store: Box<dyn AsyncStore<String, String>> = create_trait_from_type_options(&opts)?;

        assert_eq!(store.get(&"k1".to_string()).await?, "v1");
        assert_eq!(store.get(&"k2".to_string()).await?, "v2");

        Ok(())
    }

    #[test]
    fn test_invalid_load_strategy() {
        let config = json5::from_str::<LoadableSyncStoreConfig>(
            r#"{
                store: {
                    type: "RwLockHashMapStore",
                    options: {}
                },
                loader: {
                    type: "KvFileLoader",
                    options: {
                        file_path: "/tmp/test.txt",
                        parser: {
                            type: "LineParser",
                            options: {
                                separator: "\t"
                            }
                        }
                    }
                },
                load_strategy: "invalid_strategy"
            }"#,
        )
        .unwrap();

        let result = LoadableSyncStore::<String, String>::new(config);
        match result {
            Ok(_) => panic!("expected validation error but got Ok"),
            Err(e) => {
                let err_msg = format!("{}", e);
                assert!(err_msg.contains("configuration validation failed"));
            }
        }
    }
}
