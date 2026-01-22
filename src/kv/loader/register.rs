use anyhow::Result;
use std::hash::Hash;

use crate::cfg::register_trait;

use super::{Loader, KvFileLoader, KvFileLoaderConfig, FileTrigger, FileTriggerConfig};

/// 注册所有基础 Loader 实现
///
/// 为指定的 K, V 类型组合注册所有可用的 Loader 实现。
/// 由于 `Loader<K, V>` 是泛型 trait，不同的 K, V 组合会产生不同的 TypeId，
/// 因此可以使用相同的类型名称注册，不会冲突。
///
/// # 类型参数
/// - `K`: 键类型，需要满足 `Clone + Send + Sync + Eq + Hash + 'static`
/// - `V`: 值类型，需要满足 `Clone + Send + Sync + 'static`
///
/// # 注册的类型
/// - `KvFileLoader` - KV 文件加载器
/// - `FileTrigger` - 文件触发器
///
/// # 示例
/// ```ignore
/// use rustx::kv::loader::{register_loaders, Loader};
/// use rustx::cfg::{TypeOptions, create_trait_from_type_options};
///
/// // 注册 String -> String 类型的 Loader
/// register_loaders::<String, String>()?;
///
/// // 通过配置创建实例
/// let opts = TypeOptions::from_json(r#"{
///     "type": "KvFileLoader",
///     "options": {
///         "file_path": "/tmp/test.txt",
///         "parser": {
///             "type": "LineParser",
///             "options": {
///                 "separator": "\t"
///             }
///         }
///     }
/// }"#)?;
///
/// let loader: Box<dyn Loader<String, String>> = create_trait_from_type_options(&opts)?;
/// ```
pub fn register_loaders<K, V>() -> Result<()>
where
    K: Clone + Send + Sync + Eq + Hash + 'static,
    V: Clone + Send + Sync + 'static,
{
    register_trait::<KvFileLoader<K, V>, dyn Loader<K, V>, KvFileLoaderConfig>("KvFileLoader")?;
    register_trait::<FileTrigger<K, V>, dyn Loader<K, V>, FileTriggerConfig>("FileTrigger")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{create_trait_from_type_options, TypeOptions};
    use crate::kv::loader::Listener;
    use std::sync::{Arc, Mutex};
    use std::io::Write;
    use std::thread;
    use std::time::Duration;
    use tempfile::NamedTempFile;

    #[test]
    fn test_register_loaders_kv_file_loader() -> Result<()> {
        // 注册 Parser
        crate::kv::parser::register_parsers::<String, String>()?;
        // 注册 Loader
        register_loaders::<String, String>()?;

        // 创建临时文件
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "key1\tvalue1").unwrap();
        writeln!(temp_file, "key2\tvalue2").unwrap();

        // 创建 loader 配置
        let opts = TypeOptions::from_json(
            format!(r#"{{
                "type": "KvFileLoader",
                "options": {{
                    "file_path": "{}",
                    "parser": {{
                        "type": "LineParser",
                        "options": {{
                            "separator": "\t"
                        }}
                    }}
                }}
            }}"#, temp_file.path().to_string_lossy()).as_str()
        ).unwrap();

        let mut loader: Box<dyn Loader<String, String>> = create_trait_from_type_options(&opts)?;

        // 测试 loader 可以正常使用
        let call_count = Arc::new(Mutex::new(0));
        let results = Arc::new(Mutex::new(Vec::new()));

        let call_count_clone = call_count.clone();
        let results_clone = results.clone();

        let listener: Listener<String, String> = Arc::new(move |stream: Arc<dyn crate::kv::loader::Stream<_, _>>| {
            let mut count = call_count_clone.lock().unwrap();
            *count += 1;
            drop(count);

            stream.each(&|_change_type, key, value| {
                let mut results = results_clone.lock().unwrap();
                results.push(format!("{}:{}", key, value));
                Ok(())
            })
        });

        let _ = loader.on_change(listener);
        thread::sleep(Duration::from_millis(100));

        // 验证数据加载
        let count = call_count.lock().unwrap();
        assert!(*count >= 1);

        let results = results.lock().unwrap();
        assert!(results.contains(&"key1:value1".to_string()));
        assert!(results.contains(&"key2:value2".to_string()));

        // 清理
        let _ = loader.close();

        Ok(())
    }

    #[test]
    fn test_register_loaders_file_trigger() -> Result<()> {
        register_loaders::<String, String>()?;

        let opts = TypeOptions::from_json(
            r#"{
                "type": "FileTrigger",
                "options": {
                    "file_path": "/tmp/test.txt"
                }
            }"#
        ).unwrap();

        let mut trigger: Box<dyn Loader<String, String>> = create_trait_from_type_options(&opts)?;

        // 测试 trigger 可以正常使用
        let call_count = Arc::new(Mutex::new(0));

        let call_count_clone = call_count.clone();

        let listener: Listener<String, String> = Arc::new(move |stream: Arc<dyn crate::kv::loader::Stream<_, _>>| {
            let mut count = call_count_clone.lock().unwrap();
            *count += 1;
            drop(count);

            // FileTrigger 传递的是空流
            stream.each(&|_change_type, _key, _value| {
                Ok(())
            })
        });

        let _ = trigger.on_change(listener);
        thread::sleep(Duration::from_millis(100));

        // 验证触发
        let count = call_count.lock().unwrap();
        assert!(*count >= 1);

        // 清理
        let _ = trigger.close();

        Ok(())
    }

    #[test]
    fn test_register_multiple_type_combinations() -> Result<()> {
        // 注册多种类型组合
        register_loaders::<String, String>()?;
        register_loaders::<String, i64>()?;
        register_loaders::<i32, String>()?;

        // 验证各类型组合都能正常工作
        let opts_str_str = TypeOptions::from_json(
            r#"{"type": "FileTrigger", "options": {"file_path": "/tmp/test.txt"}}"#
        )?;
        let opts_str_i64 = TypeOptions::from_json(
            r#"{"type": "FileTrigger", "options": {"file_path": "/tmp/test.txt"}}"#
        )?;

        let _trigger1: Box<dyn Loader<String, String>> = create_trait_from_type_options(&opts_str_str)?;
        let _trigger2: Box<dyn Loader<String, i64>> = create_trait_from_type_options(&opts_str_i64)?;

        Ok(())
    }
}
