//! 配置源扩展功能
//!
//! 提供基于配置创建对象的能力，支持自动热更新

use anyhow::Result;
use serde::de::DeserializeOwned;
use std::sync::{Arc, RwLock};

use super::source::{ConfigChange, ConfigSource};
use super::type_options::TypeOptions;

/// 配置源扩展功能
///
/// 此 trait 为所有配置源提供创建对象的能力，包括：
/// - 一次性创建对象
/// - 创建对象并自动监听配置变化
/// - 创建 trait object 并自动监听配置变化
///
/// # 示例
/// ```no_run
/// use rustx::cfg::{Configurable, FileSource, FileSourceConfig, ConfigReloader};
/// use serde::Deserialize;
///
/// #[derive(Deserialize, Clone)]
/// struct DatabaseConfig {
///     host: String,
///     port: u16,
/// }
///
/// struct DatabaseService { /* ... */ }
///
/// impl From<DatabaseConfig> for DatabaseService {
///     fn from(config: DatabaseConfig) -> Self { Self { /* ... */ } }
/// }
///
/// impl ConfigReloader<DatabaseConfig> for DatabaseService {
///     fn reload_config(&mut self, config: DatabaseConfig) -> anyhow::Result<()> {
///         // 更新配置
///         Ok(())
///     }
/// }
///
/// let source = FileSource::new(FileSourceConfig {
///     base_path: "config".to_string(),
/// });
///
/// // 一次性创建
/// let service: DatabaseService = source.create("database").unwrap();
///
/// // 创建并自动监听
/// let service = source.create_with_watch::<DatabaseService, DatabaseConfig>("database").unwrap();
/// ```
pub trait Configurable: ConfigSource {
    /// 创建对象（一次性）
    ///
    /// 从配置源加载配置并创建对象实例
    ///
    /// # 类型参数
    /// - `T`: 目标类型，必须实现 From<Config>
    /// - `Config`: 配置类型
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回
    /// - 成功返回创建的对象实例
    fn create<T, Config>(&self, key: &str) -> Result<T>
    where
        T: From<Config> + Send + Sync + 'static,
        Config: DeserializeOwned + Clone + Send + Sync + 'static,
    {
        let config_value = self.load(key)?;
        let config: Config = config_value.into_type()?;
        Ok(T::from(config))
    }

    /// 创建对象并自动监听配置变化
    ///
    /// 从配置源加载配置并创建对象，同时监听配置变化自动更新对象
    ///
    /// # 类型参数
    /// - `T`: 目标类型，必须实现 From<Config> 和 ConfigReloader<Config>
    /// - `Config`: 配置类型
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回
    /// - 成功返回包装在 Arc<RwLock<T>> 中的对象实例
    ///
    /// # 线程安全性
    /// - 返回的 Arc<RwLock<T>> 可以在线程间安全共享
    /// - 配置更新时会自动获取写锁并调用 reload_config
    ///
    /// # 错误处理
    /// - 如果 reload_config 失败，会记录错误并保留旧配置
    fn create_with_watch<T, Config>(
        &self,
        key: &str,
    ) -> Result<Arc<RwLock<T>>>
    where
        T: From<Config> + crate::cfg::ConfigReloader<Config> + Send + Sync + 'static,
        Config: DeserializeOwned + Clone + Send + Sync + 'static,
    {
        // 加载初始配置
        let config_value = self.load(key)?;
        let config: Config = config_value.into_type()?;
        let instance = T::from(config);
        let instance = Arc::new(RwLock::new(instance));
        let instance_clone = instance.clone();

        // 监听配置变化
        let key_owned = key.to_string();
        self.watch(key, Box::new(move |change| {
            if let ConfigChange::Updated(config_value) = change {
                match config_value.into_type::<Config>() {
                    Ok(new_config) => {
                        if let Ok(mut guard) = instance_clone.write() {
                            if let Err(e) = guard.reload_config(new_config) {
                                eprintln!("重载配置失败 [{}]: {}", key_owned, e);
                                // 保留旧配置
                            } else {
                                println!("配置已更新 [{}]", key_owned);
                            }
                        } else {
                            eprintln!("获取写锁失败 [{}]", key_owned);
                        }
                    }
                    Err(e) => {
                        eprintln!("解析配置失败 [{}]: {}", key_owned, e);
                    }
                }
            }
        }))?;

        Ok(instance)
    }

    /// 创建 trait object 并自动监听配置变化
    ///
    /// 从配置源加载配置（配置需包含 type 字段），创建对应的 trait object 并监听配置变化
    ///
    /// # 类型参数
    /// - `Trait`: 目标 trait 类型
    /// - `Config`: 配置类型
    ///
    /// # 参数
    /// - `key`: 配置键，配置内容需包含 type 和 options 字段
    ///
    /// # 返回
    /// - 成功返回包装在 Arc<RwLock<Box<Trait>>> 中的 trait object
    ///
    /// # 配置格式
    /// 配置文件应包含 type 字段指定具体实现类型：
    /// ```json
    /// {
    ///   "type": "mysql",
    ///   "options": {
    ///     "host": "localhost",
    ///     "port": 3306
    ///   }
    /// }
    /// ```
    fn create_trait_with_watch<Trait, Config>(
        &self,
        key: &str,
    ) -> Result<Arc<RwLock<Box<Trait>>>>
    where
        Trait: ?Sized + Send + Sync + 'static,
        Config: DeserializeOwned + Clone + Send + Sync + 'static,
    {
        // 加载 TypeOptions
        let config_value = self.load(key)?;
        let type_options: TypeOptions = config_value.into_type()?;

        // 通过 registry 创建 trait object
        let trait_obj = crate::cfg::create_trait_from_type_options::<Trait>(&type_options)?;

        // 将 trait object 包装在 Arc<RwLock<>>
        let inner = Arc::new(RwLock::new(trait_obj));
        let inner_clone = inner.clone();

        // 监听配置变化
        let key_owned = key.to_string();
        self.watch(key, Box::new(move |change| {
            if let ConfigChange::Updated(config_value) = change {
                match config_value.into_type::<TypeOptions>() {
                    Ok(new_type_options) => {
                        // 通过 registry 重新创建 trait object
                        match crate::cfg::create_trait_from_type_options::<Trait>(&new_type_options) {
                            Ok(new_trait_obj) => {
                                if let Ok(mut guard) = inner_clone.write() {
                                    *guard = new_trait_obj;
                                    println!("配置已更新 [{}]", key_owned);
                                } else {
                                    eprintln!("获取写锁失败 [{}]", key_owned);
                                }
                            }
                            Err(e) => {
                                eprintln!("重载配置失败 [{}]: {}", key_owned, e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("解析配置失败 [{}]: {}", key_owned, e);
                    }
                }
            }
        }))?;

        Ok(inner)
    }
}

// 为所有实现了 ConfigSource 的类型自动实现 Configurable
impl<S: ConfigSource + ?Sized> Configurable for S {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{ConfigReloader, FileSource, FileSourceConfig, register_trait};
    use anyhow::anyhow;
    use serial_test::serial;
    use serde::Deserialize;
    use std::fs;
    use std::sync::{Arc, RwLock};
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct TestConfig {
        value: String,
    }

    #[derive(Debug, PartialEq)]
    struct TestService {
        config: TestConfig,
    }

    impl From<TestConfig> for TestService {
        fn from(config: TestConfig) -> Self {
            Self { config }
        }
    }

    impl ConfigReloader<TestConfig> for TestService {
        fn reload_config(&mut self, config: TestConfig) -> Result<()> {
            self.config = config;
            Ok(())
        }
    }

    #[test]
    fn test_configurable_create() {
        let source = Box::new(FileSource::new(FileSourceConfig {
            base_path: "config/test".to_string(),
        })) as Box<dyn ConfigSource>;

        // 虽然是 dyn ConfigSource，但可以调用 create 方法
        // 因为 Configurable 为 dyn ConfigSource 实现了
        let _result: Result<TestService> = source.create::<TestService, TestConfig>("test");
        // 注意：这里只是测试编译，实际运行需要配置文件存在
    }

    #[test]
    fn test_configurable_blanket_impl() {
        // 验证 blanket impl 正确工作
        let source = FileSource::new(FileSourceConfig {
            base_path: "config".to_string(),
        });

        // FileSource 实现了 ConfigSource，所以自动获得 Configurable 方法
        let _result: Result<TestService> = source.create::<TestService, TestConfig>("test");
    }

    // ========== 完整的 Configurable 功能测试 ==========

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct TestServiceConfig {
        name: String,
        count: usize,
    }

    #[derive(Debug, PartialEq)]
    struct TestService2 {
        config: TestServiceConfig,
    }

    impl From<TestServiceConfig> for TestService2 {
        fn from(config: TestServiceConfig) -> Self {
            Self { config }
        }
    }

    impl ConfigReloader<TestServiceConfig> for TestService2 {
        fn reload_config(&mut self, config: TestServiceConfig) -> Result<()> {
            self.config = config;
            Ok(())
        }
    }

    #[test]
    fn test_configurable_create_with_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("create_test.json");

        fs::write(
            &config_path,
            r#"{"name": "test-service", "count": 42}"#,
        )?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
        });

        let service: TestService2 = source.create::<TestService2, TestServiceConfig>("create_test")?;

        assert_eq!(service.config.name, "test-service");
        assert_eq!(service.config.count, 42);

        Ok(())
    }

    #[test]
    fn test_configurable_create_invalid_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid_test.json");

        fs::write(&config_path, r#"{"invalid": "data"}"#).unwrap();

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
        });

        let result: Result<TestService2> = source.create::<TestService2, TestServiceConfig>("invalid_test");
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_configurable_create_with_watch() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("reload_test.json");

        // 写入初始配置
        fs::write(
            &config_path,
            r#"{"name": "initial", "count": 1}"#,
        )?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
        });

        // 创建并监听
        let service = source.create_with_watch::<TestService2, TestServiceConfig>("reload_test")?;

        // 验证初始配置
        {
            let guard = service.read().unwrap();
            assert_eq!(guard.config.name, "initial");
            assert_eq!(guard.config.count, 1);
        }

        // 等待监听器启动
        thread::sleep(Duration::from_millis(200));

        // 修改配置
        fs::write(
            &config_path,
            r#"{"name": "updated", "count": 2}"#,
        )?;

        // 等待配置更新
        thread::sleep(Duration::from_millis(500));

        // 验证配置已更新
        {
            let guard = service.read().unwrap();
            assert_eq!(guard.config.name, "updated");
            assert_eq!(guard.config.count, 2);
        }

        Ok(())
    }

    #[test]
    #[serial]
    fn test_configurable_create_with_watch_reload_error() -> Result<()> {
        // 测试重载失败时保留旧配置

        #[derive(Debug, Clone, Deserialize)]
        struct FailingConfig {
            value: String,
        }

        struct FailingService {
            config: FailingConfig,
        }

        impl From<FailingConfig> for FailingService {
            fn from(config: FailingConfig) -> Self {
                Self { config }
            }
        }

        impl ConfigReloader<FailingConfig> for FailingService {
            fn reload_config(&mut self, config: FailingConfig) -> Result<()> {
                // 只允许特定值
                if config.value == "valid" {
                    self.config = config;
                    Ok(())
                } else {
                    Err(anyhow!("Invalid config value"))
                }
            }
        }

        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("failing_test.json");

        // 写入有效配置
        fs::write(&config_path, r#"{"value": "valid"}"#)?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
        });

        let service = source.create_with_watch::<FailingService, FailingConfig>("failing_test")?;

        // 验证初始配置
        {
            let guard = service.read().unwrap();
            assert_eq!(guard.config.value, "valid");
        }

        thread::sleep(Duration::from_millis(200));

        // 写入无效配置
        fs::write(&config_path, r#"{"value": "invalid"}"#)?;

        thread::sleep(Duration::from_millis(500));

        // 验证旧配置被保留
        {
            let guard = service.read().unwrap();
            assert_eq!(guard.config.value, "valid");
        }

        Ok(())
    }

    // ========== create_trait_with_watch 方法测试 ==========

    trait TestTrait: Send + Sync {
        fn get_name(&self) -> &str;
    }

    #[derive(Debug, Clone, Deserialize)]
    struct TraitServiceConfig {
        name: String,
    }

    struct TraitServiceA {
        config: TraitServiceConfig,
    }

    impl From<TraitServiceConfig> for TraitServiceA {
        fn from(config: TraitServiceConfig) -> Self {
            Self { config }
        }
    }

    impl TestTrait for TraitServiceA {
        fn get_name(&self) -> &str {
            &self.config.name
        }
    }

    impl ConfigReloader<TraitServiceConfig> for TraitServiceA {
        fn reload_config(&mut self, config: TraitServiceConfig) -> Result<()> {
            self.config = config;
            Ok(())
        }
    }

    impl From<Box<TraitServiceA>> for Box<dyn TestTrait> {
        fn from(s: Box<TraitServiceA>) -> Self {
            s as Box<dyn TestTrait>
        }
    }

    #[test]
    #[serial]
    fn test_configurable_create_trait_with_watch() -> Result<()> {
        // 注册到 trait registry
        register_trait::<TraitServiceA, dyn TestTrait, TraitServiceConfig>("trait-a")?;

        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("trait_test.json");

        // 写入配置（包含 type 字段）
        fs::write(
            &config_path,
            r#"{"type": "trait-a", "options": {"name": "initial"}}"#,
        )?;

        let source = FileSource::new(FileSourceConfig {
            base_path: temp_dir.path().to_string_lossy().to_string(),
        });

        // 创建并监听
        let service: Arc<RwLock<Box<dyn TestTrait>>> =
            source.create_trait_with_watch::<dyn TestTrait, TraitServiceConfig>("trait_test")?;

        // 验证初始配置
        {
            let guard = service.read().unwrap();
            assert_eq!(guard.get_name(), "initial");
        }

        thread::sleep(Duration::from_millis(200));

        // 修改配置
        fs::write(
            &config_path,
            r#"{"type": "trait-a", "options": {"name": "updated"}}"#,
        )?;

        thread::sleep(Duration::from_millis(500));

        // 验证配置已更新
        {
            let guard = service.read().unwrap();
            assert_eq!(guard.get_name(), "updated");
        }

        Ok(())
    }
}
