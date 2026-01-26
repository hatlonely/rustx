use crate::aop::{Aop, AopConfig};
use crate::cfg::ConfigReloader;
use anyhow::Result;
use serde::Deserialize;
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Aop Manager 配置
///
/// 用于统一管理多个 Aop 实例
#[derive(Debug, Clone, Deserialize, SmartDefault)]
#[serde(default)]
pub struct AopManagerConfig {
    /// 全局默认配置（如果未配置则使用默认值）
    pub default: AopConfig,

    /// 命名 aop 配置映射
    pub aops: HashMap<String, AopConfig>,
}

/// AOP 管理器
///
/// 全局单例，负责统一维护所有 Aop 实例
pub struct AopManager {
    config: Arc<RwLock<AopManagerConfig>>,
    pub(crate) aops: Arc<RwLock<HashMap<String, Arc<Aop>>>>,
    default: Arc<RwLock<Arc<Aop>>>,
}

impl AopManager {
    /// 从配置创建 AopManager
    pub fn new(config: AopManagerConfig) -> Result<Self> {
        let mut aops_map = HashMap::new();

        // 第一步：创建所有 Create 模式的 aop
        let mut reference_configs: Vec<(String, String)> = Vec::new();

        for (key, aop_config) in &config.aops {
            match aop_config {
                AopConfig::Reference { instance } => {
                    // 记录引用关系，稍后处理
                    reference_configs.push((key.clone(), instance.clone()));
                }
                AopConfig::Create(create_config) => {
                    // 直接创建新的 aop
                    let aop = Arc::new(Aop::new(create_config.clone())?);
                    aops_map.insert(key.clone(), aop);
                }
            }
        }

        // 第二步：处理所有 Reference 模式的配置
        for (key, instance) in reference_configs {
            let aop = Self::resolve_aop_config_by_name(&instance, &aops_map)?;
            aops_map.insert(key, aop);
        }

        // 创建默认 aop（始终存在）
        let default_aop = match &config.default {
            AopConfig::Reference { instance } => {
                Self::resolve_aop_config_by_name(instance, &aops_map)?
            }
            AopConfig::Create(create_config) => Arc::new(Aop::new(create_config.clone())?),
        };

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            aops: Arc::new(RwLock::new(aops_map)),
            default: Arc::new(RwLock::new(default_aop)),
        })
    }

    /// 根据名称解析 Aop 实例
    ///
    /// 先从已创建的 aops 中查找，再从全局管理器中查找
    fn resolve_aop_config_by_name(
        instance: &str,
        created_aops: &HashMap<String, Arc<Aop>>,
    ) -> Result<Arc<Aop>> {
        // 先从当前已创建的 aops 中查找
        if let Some(aop) = created_aops.get(instance) {
            return Ok(Arc::clone(aop));
        }

        // 再从全局管理器中查找
        if let Some(aop) = crate::aop::get(instance) {
            return Ok(aop);
        }

        // 都找不到，返回错误
        Err(anyhow::anyhow!(
            "Aop instance '{}' not found (neither in current config nor in global manager)",
            instance
        ))
    }

    /// 获取指定 key 的 aop
    ///
    /// 如果 key 不存在，返回 None
    pub fn get(&self, key: &str) -> Option<Arc<Aop>> {
        let aops = self.aops.read().unwrap();
        aops.get(key).cloned()
    }

    /// 获取指定 key 的 aop，如果不存在则返回默认 aop
    pub fn get_or_default(&self, key: &str) -> Arc<Aop> {
        self.get(key).unwrap_or_else(|| self.get_default())
    }

    /// 获取默认 aop
    pub fn get_default(&self) -> Arc<Aop> {
        let default = self.default.read().unwrap();
        Arc::clone(&default)
    }

    /// 设置默认 aop
    pub fn set_default(&self, aop: Arc<Aop>) {
        let mut default = self.default.write().unwrap();
        *default = aop;
    }

    /// 动态添加 aop
    pub fn add(&self, key: String, aop: Aop) {
        let mut aops = self.aops.write().unwrap();
        aops.insert(key, Arc::new(aop));
    }

    /// 检查指定 key 的 aop 是否存在
    pub fn contains(&self, key: &str) -> bool {
        let aops = self.aops.read().unwrap();
        aops.contains_key(key)
    }

    /// 获取所有 aop 的 key 列表
    pub fn keys(&self) -> Vec<String> {
        let aops = self.aops.read().unwrap();
        aops.keys().cloned().collect()
    }

    /// 移除指定 key 的 aop
    pub fn remove(&self, key: &str) -> Option<Arc<Aop>> {
        let mut aops = self.aops.write().unwrap();
        aops.remove(key)
    }
}

/// 为 AopManager 实现 ConfigReloader trait
///
/// 支持配置热更新，采用增量更新策略：
/// - 配置未变化的 Aop 实例会被保留
/// - 配置变化的 Aop 实例会被重新创建
/// - 新配置中不存在的 key 会被删除
impl ConfigReloader<AopManagerConfig> for AopManager {
    fn reload_config(&mut self, new_config: AopManagerConfig) -> Result<()> {
        // 锁定旧配置和实例
        let old_config = self.config.read().unwrap();
        let old_aops = self.aops.read().unwrap();

        // 创建新的 aops map
        let mut new_aops = HashMap::new();
        let mut reference_configs: Vec<(String, String)> = Vec::new();

        // 第一步：处理所有 Create 模式的 aop
        for (key, new_aop_config) in &new_config.aops {
            match new_aop_config {
                AopConfig::Reference { instance } => {
                    // 记录引用关系，稍后处理
                    reference_configs.push((key.clone(), instance.clone()));
                }
                AopConfig::Create(new_create_config) => {
                    // 检查旧配置是否存在且相同
                    let should_reuse = match old_config.aops.get(key) {
                        Some(AopConfig::Create(old_create_config)) => {
                            old_create_config == new_create_config
                        }
                        _ => false,
                    };

                    if should_reuse {
                        // 配置未变化，复用旧实例
                        if let Some(old_aop) = old_aops.get(key) {
                            new_aops.insert(key.clone(), Arc::clone(old_aop));
                            continue;
                        }
                    }

                    // 配置变化或不存在，创建新实例
                    let aop = Arc::new(Aop::new(new_create_config.clone())?);
                    new_aops.insert(key.clone(), aop);
                }
            }
        }

        // 第二步：处理所有 Reference 模式的配置
        for (key, instance) in reference_configs {
            let aop = Self::resolve_aop_config_by_name(&instance, &new_aops)?;
            new_aops.insert(key, aop);
        }

        // 第三步：处理默认 aop
        let new_default_aop = match &new_config.default {
            AopConfig::Reference { instance } => {
                Self::resolve_aop_config_by_name(instance, &new_aops)?
            }
            AopConfig::Create(new_create_config) => {
                // 检查旧 default 配置是否存在且相同
                let should_reuse = match &old_config.default {
                    AopConfig::Create(old_create_config) => {
                        old_create_config == new_create_config
                    }
                    _ => false,
                };

                if should_reuse {
                    // 配置未变化，复用旧实例
                    let default_aop = self.default.read().unwrap();
                    let old_default = (*default_aop).clone();
                    drop(default_aop);
                    old_default
                } else {
                    // 配置变化，创建新实例
                    Arc::new(Aop::new(new_create_config.clone())?)
                }
            }
        };

        // 释放旧锁
        drop(old_config);
        drop(old_aops);

        // 第四步：更新内部状态
        {
            let mut config_write = self.config.write().unwrap();
            *config_write = new_config;
        }

        {
            let mut aops_write = self.aops.write().unwrap();
            *aops_write = new_aops;
        }

        {
            let mut default_write = self.default.write().unwrap();
            *default_write = new_default_aop;
        }

        Ok(())
    }
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
    async fn test_manager_new() -> Result<()> {
        let mut aops = HashMap::new();
        aops.insert("main".to_string(), AopConfig::Create(create_test_aop_config()));
        aops.insert("db".to_string(), AopConfig::Create(create_test_aop_config()));

        let config = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops,
        };

        let manager = AopManager::new(config)?;

        // 测试获取 aop
        assert!(manager.contains("main"));
        assert!(manager.contains("db"));
        assert!(!manager.contains("nonexistent"));

        // 测试默认 aop
        let _default = manager.get_default();

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_get() -> Result<()> {
        let mut aops = HashMap::new();
        aops.insert("main".to_string(), AopConfig::Create(create_test_aop_config()));

        let config = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops,
        };

        let manager = AopManager::new(config)?;

        // 测试获取存在的 aop
        let aop = manager.get("main");
        assert!(aop.is_some());

        // 测试获取不存在的 aop
        assert!(manager.get("nonexistent").is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_get_or_default() -> Result<()> {
        let mut aops = HashMap::new();
        aops.insert("main".to_string(), AopConfig::Create(create_test_aop_config()));

        let config = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops,
        };

        let manager = AopManager::new(config)?;

        // 测试获取存在的 aop
        let aop = manager.get_or_default("main");
        assert!(aop.retry_config.is_some());

        // 测试获取不存在的 aop 返回默认
        let aop = manager.get_or_default("nonexistent");
        assert!(aop.retry_config.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_add() -> Result<()> {
        let config = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops: HashMap::new(),
        };

        let manager = AopManager::new(config)?;

        // 动态添加 aop
        let aop = Aop::new(create_test_aop_config())?;
        manager.add("dynamic".to_string(), aop);

        // 验证添加成功
        assert!(manager.contains("dynamic"));
        assert!(manager.get("dynamic").is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_keys() -> Result<()> {
        let mut aops = HashMap::new();
        aops.insert("a".to_string(), AopConfig::Create(create_test_aop_config()));
        aops.insert("b".to_string(), AopConfig::Create(create_test_aop_config()));
        aops.insert("c".to_string(), AopConfig::Create(create_test_aop_config()));

        let config = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops,
        };

        let manager = AopManager::new(config)?;

        // 测试获取所有 keys
        let keys = manager.keys();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"a".to_string()));
        assert!(keys.contains(&"b".to_string()));
        assert!(keys.contains(&"c".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_remove() -> Result<()> {
        let mut aops = HashMap::new();
        aops.insert("main".to_string(), AopConfig::Create(create_test_aop_config()));

        let config = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops,
        };

        let manager = AopManager::new(config)?;

        // 测试移除存在的 aop
        let removed = manager.remove("main");
        assert!(removed.is_some());
        assert!(!manager.contains("main"));

        // 测试移除不存在的 aop
        assert!(manager.remove("nonexistent").is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_manager_reference_instance() -> Result<()> {
        let mut aops = HashMap::new();

        // 创建一个完整的 aop
        aops.insert("main".to_string(), AopConfig::Create(create_test_aop_config()));

        // 引用 main aop
        aops.insert(
            "api".to_string(),
            AopConfig::Reference {
                instance: "main".to_string(),
            },
        );

        // 也引用 main aop
        aops.insert(
            "service".to_string(),
            AopConfig::Reference {
                instance: "main".to_string(),
            },
        );

        let config = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops,
        };

        let manager = AopManager::new(config)?;

        // 验证所有 aop 都存在
        assert!(manager.contains("main"));
        assert!(manager.contains("api"));
        assert!(manager.contains("service"));

        // 验证 api 和 service 都指向同一个 aop 实例
        let main_aop = manager.get("main").unwrap();
        let api_aop = manager.get("api").unwrap();
        let service_aop = manager.get("service").unwrap();

        // 使用 Arc::ptr_eq 检查是否是同一个实例
        assert!(Arc::ptr_eq(&main_aop, &api_aop));
        assert!(Arc::ptr_eq(&main_aop, &service_aop));
        assert!(Arc::ptr_eq(&api_aop, &service_aop));

        Ok(())
    }

    #[tokio::test]
    async fn test_config_reloader_keep_unchanged() -> Result<()> {
        // 创建初始配置
        let mut aops = HashMap::new();
        aops.insert("main".to_string(), AopConfig::Create(create_test_aop_config()));
        aops.insert("db".to_string(), AopConfig::Create(create_test_aop_config()));

        let config1 = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops: aops.clone(),
        };

        let mut manager = AopManager::new(config1)?;

        // 保存旧实例引用
        let old_main = manager.get("main").unwrap();
        let old_db = manager.get("db").unwrap();
        let old_default = manager.get_default();

        // 重载相同配置
        let config2 = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops,
        };

        manager.reload_config(config2)?;

        // 验证实例未被替换（配置未变化）
        let new_main = manager.get("main").unwrap();
        let new_db = manager.get("db").unwrap();
        let new_default = manager.get_default();

        assert!(Arc::ptr_eq(&old_main, &new_main));
        assert!(Arc::ptr_eq(&old_db, &new_db));
        assert!(Arc::ptr_eq(&old_default, &new_default));

        Ok(())
    }

    #[tokio::test]
    async fn test_config_reloader_change_config() -> Result<()> {
        // 创建初始配置
        let mut aops = HashMap::new();
        aops.insert("main".to_string(), AopConfig::Create(create_test_aop_config()));
        aops.insert("db".to_string(), AopConfig::Create(create_test_aop_config()));

        let config1 = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops,
        };

        let mut manager = AopManager::new(config1)?;

        // 保存旧实例引用
        let old_main = manager.get("main").unwrap();
        let old_db = manager.get("db").unwrap();

        // 创建不同的配置（修改重试次数）
        let config_json = r#"{
            retry: {
                max_times: 5,
                strategy: "constant",
                delay: "100ms"
            }
        }"#;
        let different_config = json5::from_str(config_json).unwrap();

        // 重载配置（修改 main 的配置）
        let mut new_aops = HashMap::new();
        new_aops.insert("main".to_string(), AopConfig::Create(different_config));
        new_aops.insert("db".to_string(), AopConfig::Create(create_test_aop_config()));

        let config2 = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops: new_aops,
        };

        manager.reload_config(config2)?;

        // 验证实例是否被替换
        let new_main = manager.get("main").unwrap();
        let new_db = manager.get("db").unwrap();

        // main 的配置变了，应该被重建
        assert!(!Arc::ptr_eq(&old_main, &new_main));
        assert_eq!(new_main.retry_config.as_ref().unwrap().max_times, 5);

        // db 的配置未变，应该复用旧实例
        assert!(Arc::ptr_eq(&old_db, &new_db));

        Ok(())
    }

    #[tokio::test]
    async fn test_config_reloader_add_remove_aop() -> Result<()> {
        // 创建初始配置
        let mut aops = HashMap::new();
        aops.insert("main".to_string(), AopConfig::Create(create_test_aop_config()));
        aops.insert("db".to_string(), AopConfig::Create(create_test_aop_config()));

        let config1 = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops,
        };

        let mut manager = AopManager::new(config1)?;

        // 保存旧实例引用
        let old_main = manager.get("main").unwrap();

        // 重载配置（删除 db，添加 api）
        let mut new_aops = HashMap::new();
        new_aops.insert("main".to_string(), AopConfig::Create(create_test_aop_config()));
        new_aops.insert("api".to_string(), AopConfig::Create(create_test_aop_config()));

        let config2 = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops: new_aops,
        };

        manager.reload_config(config2)?;

        // 验证结果
        assert!(manager.contains("main"));
        assert!(!manager.contains("db")); // db 被删除
        assert!(manager.contains("api")); // api 被添加

        // main 应该复用旧实例
        let new_main = manager.get("main").unwrap();
        assert!(Arc::ptr_eq(&old_main, &new_main));

        // db 已不存在
        assert!(manager.get("db").is_none());

        // api 是新创建的
        assert!(manager.get("api").is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_config_reloader_with_reference() -> Result<()> {
        // 创建初始配置
        let mut aops = HashMap::new();
        aops.insert("main".to_string(), AopConfig::Create(create_test_aop_config()));
        aops.insert(
            "api".to_string(),
            AopConfig::Reference {
                instance: "main".to_string(),
            },
        );

        let config1 = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops,
        };

        let mut manager = AopManager::new(config1)?;

        // 验证初始引用关系
        let main1 = manager.get("main").unwrap();
        let api1 = manager.get("api").unwrap();
        assert!(Arc::ptr_eq(&main1, &api1));

        // 创建不同的配置
        let config_json = r#"{
            retry: {
                max_times: 10,
                strategy: "exponential",
                delay: "200ms"
            }
        }"#;
        let different_config = json5::from_str(config_json).unwrap();

        // 重载配置（修改 main 的配置）
        let mut new_aops = HashMap::new();
        new_aops.insert("main".to_string(), AopConfig::Create(different_config));
        new_aops.insert(
            "api".to_string(),
            AopConfig::Reference {
                instance: "main".to_string(),
            },
        );

        let config2 = AopManagerConfig {
            default: AopConfig::Create(create_test_aop_config()),
            aops: new_aops,
        };

        manager.reload_config(config2)?;

        // 验证引用关系仍然保持
        let main2 = manager.get("main").unwrap();
        let api2 = manager.get("api").unwrap();
        assert!(Arc::ptr_eq(&main2, &api2));

        // 验证 main 被重建
        assert!(!Arc::ptr_eq(&main1, &main2));
        assert_eq!(main2.retry_config.as_ref().unwrap().max_times, 10);

        Ok(())
    }
}
