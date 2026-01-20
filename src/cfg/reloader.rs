//! 配置热更新支持
//!
//! 提供配置自动重载的 trait 和相关类型

use anyhow::Result;

/// 配置重载接口
///
/// 实现此 trait 的类型可以在配置变化时自动更新内部状态
///
/// # 示例
/// ```no_run
/// use rustx::cfg::ConfigReloader;
/// use serde::Deserialize;
///
/// #[derive(Deserialize, Clone)]
/// struct DatabaseConfig {
///     host: String,
///     port: u16,
/// }
///
/// struct DatabaseService {
///     host: String,
///     port: u16,
/// }
///
/// impl ConfigReloader<DatabaseConfig> for DatabaseService {
///     fn reload_config(&mut self, config: DatabaseConfig) -> Result<()> {
///         self.host = config.host;
///         self.port = config.port;
///         println!("Database config updated: {}:{}", self.host, self.port);
///         Ok(())
///     }
/// }
/// ```
pub trait ConfigReloader<Config>: Send + Sync {
    /// 当配置变化时被调用
    ///
    /// # 参数
    /// - `config`: 新的配置值
    ///
    /// # 返回
    /// - 成功返回 Ok(())
    /// - 失败返回错误，错误会被记录但不会影响当前对象状态（保留旧配置）
    fn reload_config(&mut self, config: Config) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct TestConfig {
        value: String,
        count: usize,
    }

    struct TestService {
        config: TestConfig,
    }

    impl ConfigReloader<TestConfig> for TestService {
        fn reload_config(&mut self, config: TestConfig) -> Result<()> {
            self.config = config;
            Ok(())
        }
    }

    #[test]
    fn test_config_reloader() {
        let config1 = TestConfig {
            value: "first".to_string(),
            count: 1,
        };

        let config2 = TestConfig {
            value: "second".to_string(),
            count: 2,
        };

        let mut service = TestService {
            config: config1.clone(),
        };

        assert_eq!(service.config, config1);

        service.reload_config(config2.clone()).unwrap();
        assert_eq!(service.config, config2);
    }
}
