use crate::aop::aop_manager::AopManager;
use crate::aop::aop_manager::AopManagerConfig;
use crate::aop::Aop;
use anyhow::Result;
use std::sync::Arc;

/// 全局 AopManager 单例
///
/// 默认包含一个空的 Aop 实例
static GLOBAL_AOP_MANAGER: once_cell::sync::Lazy<Arc<AopManager>> =
    once_cell::sync::Lazy::new(|| {
        // 使用默认配置
        let default_config = AopManagerConfig::default();

        Arc::new(AopManager::new(default_config).expect("Failed to create global AopManager"))
    });

/// 初始化全局 AopManager
///
/// # 示例
///
/// ```ignore
/// fn example() -> anyhow::Result<()> {
///     let config = AopManagerConfig {
///         default: default_aop_config,
///         aops: aop_map,
///     };
///     ::rustx::aop::init(config)?;
///     Ok(())
/// }
/// ```
pub fn init(config: AopManagerConfig) -> Result<()> {
    let manager = AopManager::new(config)?;

    // 合并 aops 到全局单例
    let global_aops = manager.aops.read().unwrap();
    let mut global = GLOBAL_AOP_MANAGER.aops.write().unwrap();

    // 复制所有 aop
    for (key, aop) in global_aops.iter() {
        global.insert(key.clone(), aop.clone());
    }

    // 设置默认 aop
    GLOBAL_AOP_MANAGER.set_default(manager.get_default());

    Ok(())
}

/// 获取全局 AopManager
pub fn global_aop_manager() -> Arc<AopManager> {
    Arc::clone(&GLOBAL_AOP_MANAGER)
}

/// 获取指定 key 的 aop（全局）
pub fn get(key: &str) -> Option<Arc<Aop>> {
    global_aop_manager().get(key)
}

/// 获取指定 key 的 aop，如果不存在则返回默认 aop（全局）
pub fn get_or_default(key: &str) -> Arc<Aop> {
    global_aop_manager().get_or_default(key)
}

/// 获取默认 aop（全局）
pub fn get_default() -> Arc<Aop> {
    global_aop_manager().get_default()
}

/// 设置默认 aop（全局）
pub fn set_default(aop: Arc<Aop>) {
    global_aop_manager().set_default(aop);
}

/// 动态添加 aop（全局）
pub fn add(key: String, aop: Aop) {
    global_aop_manager().add(key, aop);
}

/// 检查指定 key 的 aop 是否存在（全局）
pub fn contains(key: &str) -> bool {
    global_aop_manager().contains(key)
}

/// 获取所有 aop 的 key 列表（全局）
pub fn keys() -> Vec<String> {
    global_aop_manager().keys()
}

/// 移除指定 key 的 aop（全局）
pub fn remove(key: &str) -> Option<Arc<Aop>> {
    global_aop_manager().remove(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aop::aop::AopCreateConfig;

    /// 辅助函数：创建测试用的 AopCreateConfig
    fn create_test_aop_config() -> AopCreateConfig {
        let config_json = r#"{
            retry: {
                max_times: 3,
                strategy: "constant",
                delay: "100ms"
            }
        }"#;

        json5::from_str(config_json).expect("Failed to parse AopCreateConfig")
    }

    #[tokio::test]
    async fn test_global_aop_manager() -> Result<()> {
        // 测试获取全局单例
        let manager1 = global_aop_manager();
        let manager2 = global_aop_manager();

        // 验证是同一个实例
        assert!(Arc::ptr_eq(&manager1, &manager2));

        // 测试全局函数
        let aop = Aop::new(create_test_aop_config())?;
        add("test_global".to_string(), aop);

        assert!(get("test_global").is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_init() -> Result<()> {
        let mut aops = std::collections::HashMap::new();
        aops.insert(
            "main".to_string(),
            crate::aop::AopConfig::Create(create_test_aop_config()),
        );
        aops.insert(
            "db".to_string(),
            crate::aop::AopConfig::Create(create_test_aop_config()),
        );

        let config = AopManagerConfig {
            default: crate::aop::AopConfig::Create(create_test_aop_config()),
            aops,
            tracer: None,
            metric: None,
        };

        init(config)?;

        // 验证 aop 已添加到全局
        assert!(get("main").is_some());
        assert!(get("db").is_some());
        let _default = get_default();

        Ok(())
    }

    #[tokio::test]
    async fn test_default_aop_available() {
        // 创建一个新的 AopManager 进行测试，避免受全局单例影响
        let config = AopManagerConfig {
            default: crate::aop::AopConfig::Create(create_test_aop_config()),
            aops: std::collections::HashMap::new(),
            tracer: None,
            metric: None,
        };

        let manager = AopManager::new(config).unwrap();
        let default_aop = manager.get_default();

        // 验证默认 aop 存在
        assert!(Arc::ptr_eq(&default_aop, &manager.get_default()));

        // 测试可以正常使用（这个 aop 有 retry 配置，所以 build_backoff 应该返回 Some）
        assert!(default_aop.build_backoff().is_some());
    }

    #[tokio::test]
    async fn test_convenience_functions() -> Result<()> {
        // 测试全局便捷函数
        let aop = Aop::new(create_test_aop_config())?;
        add("test".to_string(), aop);

        assert!(contains("test"));
        assert!(!contains("nonexistent"));

        let keys = keys();
        assert!(keys.contains(&"test".to_string()));

        let removed = remove("test");
        assert!(removed.is_some());
        assert!(!contains("test"));

        let removed_none = remove("nonexistent");
        assert!(removed_none.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_or_default_aop() -> Result<()> {
        // 创建独立的 AopManager 进行测试，避免受全局单例影响
        let config = AopManagerConfig {
            default: crate::aop::AopConfig::Create(create_test_aop_config()),
            aops: {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    "existing".to_string(),
                    crate::aop::AopConfig::Create(create_test_aop_config()),
                );
                map
            },
            tracer: None,
            metric: None,
        };

        let manager = AopManager::new(config)?;

        // 存在的 key
        let result = manager.get_or_default("existing");
        assert!(result.retry_config.is_some());

        // 不存在的 key 返回默认
        let result = manager.get_or_default("nonexistent");
        assert!(result.retry_config.is_some()); // 默认配置也有 retry

        Ok(())
    }

    #[tokio::test]
    async fn test_set_default_aop() -> Result<()> {
        // 创建新的 aop
        let new_aop = Arc::new(Aop::new(create_test_aop_config())?);

        // 设置为默认
        set_default(new_aop.clone());

        // 验证默认 aop 已更新
        let default = get_default();
        assert!(Arc::ptr_eq(&new_aop, &default));

        Ok(())
    }
}
