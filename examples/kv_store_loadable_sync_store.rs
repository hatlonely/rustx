use anyhow::Result;
use rustx::cfg::*;
use rustx::kv::loader::register_loaders;
use rustx::kv::parser::register_parsers;
use rustx::kv::store::{register_hash_stores, SetOptions, SyncStore};
use std::io::Write;
use tempfile::NamedTempFile;

fn main() -> Result<()> {
    // 注册 Parser、Loader、Store
    register_parsers::<String, String>()?;
    register_loaders::<String, String>()?;
    register_hash_stores::<String, String>()?;

    // 准备数据文件（tab 分隔的 key-value）
    let mut data_file = NamedTempFile::new()?;
    writeln!(data_file, "user:1\tAlice")?;
    writeln!(data_file, "user:2\tBob")?;
    writeln!(data_file, "user:3\tCharlie")?;
    data_file.flush()?;
    let file_path = data_file.path().to_str().unwrap();

    // 通过 JSON5 配置创建 LoadableSyncStore
    // inplace 策略：增量更新，直接在当前 store 上 set/del
    let config = format!(
        r#"{{
            type: "LoadableSyncStore",
            options: {{
                store: {{
                    type: "RwLockHashMapStore",
                    options: {{}}
                }},
                loader: {{
                    type: "KvFileLoader",
                    options: {{
                        file_path: "{}",
                        parser: {{
                            type: "LineParser",
                            options: {{ separator: "\t" }}
                        }}
                    }}
                }},
                load_strategy: "inplace",
            }}
        }}"#,
        file_path
    );

    let opts = TypeOptions::from_json(&config)?;
    let store: Box<dyn SyncStore<String, String>> = create_trait_from_type_options(&opts)?;

    // 数据已从文件自动加载
    println!("get user:1: {}", store.get_sync(&"user:1".to_string())?);
    println!("get user:2: {}", store.get_sync(&"user:2".to_string())?);

    // 也支持手动读写
    store.set_sync(&"user:4".to_string(), &"David".to_string(), &SetOptions::new())?;
    println!("get user:4: {}", store.get_sync(&"user:4".to_string())?);

    // 批量操作
    let keys = vec!["user:1".to_string(), "user:2".to_string(), "user:3".to_string()];
    let (vals, errs) = store.batch_get_sync(&keys)?;
    println!("batch_get: {:?}, errors: {:?}", vals, errs);

    store.del_sync(&"user:1".to_string())?;
    println!("del user:1, get: {:?}", store.get_sync(&"user:1".to_string()).is_err());

    store.close_sync()?;

    Ok(())
}
