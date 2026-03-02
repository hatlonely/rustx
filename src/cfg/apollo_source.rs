//! Apollo 配置中心源
//!
//! 支持从 Apollo 配置中心加载配置，支持长轮询监听配置变化

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::source::{ConfigChange, ConfigSource, ConfigValue};
use crate::{impl_from, impl_box_from};

/// Apollo 配置中心源的配置
#[derive(Debug, Clone, Deserialize, Serialize, SmartDefault)]
#[serde(default)]
pub struct ApolloSourceConfig {
    /// Apollo 服务器地址，如 "http://localhost:8080"
    pub server_url: String,
    /// 应用 ID
    pub app_id: String,
    /// 命名空间，默认为 "application"
    #[default = "application"]
    pub namespace: String,
    /// 集群名称，默认为 "default"
    #[default = "default"]
    pub cluster: String,
}

/// Apollo 配置中心响应
#[derive(Debug, Deserialize)]
struct ApolloResponse {
    #[serde(rename = "releaseKey")]
    #[allow(dead_code)]
    release_key: String,
    configurations: serde_json::Value,
}

/// Apollo 通知响应
#[derive(Debug, Deserialize)]
struct ApolloNotification {
    #[serde(rename = "namespaceName")]
    #[allow(dead_code)]
    namespace_name: String,
    #[serde(rename = "notificationId")]
    notification_id: i64,
}

/// Apollo 配置中心源
///
/// 支持从 Apollo 配置中心加载配置并监听变化
///
/// # 监听行为说明
/// - `watch` 方法仅监听配置**变化**，不会在启动时立即触发回调
/// - 如需获取初始配置，应先调用 `load` 方法，再调用 `watch` 监听后续变化
/// - 使用 Apollo 长轮询机制实现配置变更通知
/// - 所有 watch 调用共享一个长轮询线程，自动检测配置变化
///
/// # 示例
/// ```no_run
/// use rustx::cfg::{ConfigSource, ApolloSource, ApolloSourceConfig};
///
/// // 创建 Apollo 配置源
/// let source = ApolloSource::new(ApolloSourceConfig {
///     server_url: "http://localhost:8080".to_string(),
///     app_id: "my-app".to_string(),
///     namespace: "application".to_string(),
///     cluster: "default".to_string(),
/// }).unwrap();
///
/// // 加载初始配置
/// let config = source.load("database").unwrap();
///
/// // 监听后续变化
/// source.watch("database", |change| {
///     // 仅在配置发生变化时才会触发
/// }).unwrap();
/// ```
pub struct ApolloSource {
    /// Apollo 服务器地址
    server_url: String,
    /// 应用 ID
    app_id: String,
    /// 命名空间，默认为 "application"
    namespace: String,
    /// 集群名称，默认为 "default"
    cluster: String,
    /// HTTP 客户端
    client: reqwest::blocking::Client,
    /// 存储 key -> handlers 的映射
    handlers: Arc<Mutex<HashMap<String, Vec<Box<dyn Fn(ConfigChange) + Send + Sync>>>>>,
    /// 存储每个 key 当前配置值，用于对比变化
    current_values: Arc<Mutex<HashMap<String, serde_json::Value>>>,
}

impl ApolloSource {
    /// 创建 Apollo 配置源
    ///
    /// # 参数
    /// - `config`: Apollo 配置源配置
    pub fn new(config: ApolloSourceConfig) -> Result<Self> {
        let server_url = config.server_url.trim_end_matches('/').to_string();
        let app_id = config.app_id.clone();
        let cluster = config.cluster.clone();
        let namespace = config.namespace.clone();

        let source = Self {
            server_url,
            app_id,
            namespace,
            cluster,
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(90))
                .build()?,
            handlers: Arc::new(Mutex::new(HashMap::new())),
            current_values: Arc::new(Mutex::new(HashMap::new())),
        };

        // 启动长轮询线程
        source.start_watch_thread();

        Ok(source)
    }

    /// 启动长轮询监听线程
    fn start_watch_thread(&self) {
        let url = format!("{}/notifications/v2", self.server_url);
        let client = self.client.clone();
        let app_id = self.app_id.clone();
        let cluster = self.cluster.clone();
        let namespace = self.namespace.clone();
        let server_url = self.server_url.clone();
        let handlers = self.handlers.clone();
        let current_values = self.current_values.clone();

        thread::spawn(move || {
            let mut notification_id = -1i64;
            let mut is_first_notification = true;

            loop {
                // Apollo 长轮询
                let params = serde_json::json!([{
                    "namespaceName": namespace,
                    "notificationId": notification_id,
                }]);

                match client
                    .get(&url)
                    .query(&[
                        ("appId", app_id.as_str()),
                        ("cluster", cluster.as_str()),
                        ("notifications", &params.to_string()),
                    ])
                    .timeout(Duration::from_secs(90))
                    .send()
                {
                    Ok(resp) if resp.status().is_success() => {
                        // 收到配置变更通知
                        if let Ok(notifications) = resp.json::<Vec<ApolloNotification>>() {
                            if let Some(notif) = notifications.first() {
                                let new_id = notif.notification_id;
                                if new_id != notification_id {
                                    notification_id = new_id;

                                    // 如果是首次收到通知，仅更新 notification_id，不触发回调
                                    if is_first_notification {
                                        is_first_notification = false;
                                        continue;
                                    }

                                    // 配置有更新，重新加载并分发变化
                                    Self::handle_config_change(
                                        &server_url,
                                        &app_id,
                                        &cluster,
                                        &namespace,
                                        &handlers,
                                        &current_values,
                                    );
                                }
                            }
                        }
                    }
                    Ok(resp) if resp.status().as_u16() == 304 => {
                        // 304 表示配置未变化，继续长轮询
                    }
                    Err(_) | Ok(_) => {
                        // 网络错误或超时，等待后重试
                        thread::sleep(Duration::from_secs(5));
                    }
                }

                // 短暂休眠，避免紧密循环
                thread::sleep(Duration::from_millis(100));
            }
        });
    }

    /// 处理配置变更
    fn handle_config_change(
        server_url: &str,
        app_id: &str,
        cluster: &str,
        namespace: &str,
        handlers: &Arc<Mutex<HashMap<String, Vec<Box<dyn Fn(ConfigChange) + Send + Sync>>>>>,
        current_values: &Arc<Mutex<HashMap<String, serde_json::Value>>>,
    ) {
        // 重新加载 namespace 配置
        let fetch_url = format!("{}/configs/{}/{}/{}", server_url, app_id, cluster, namespace);

        match reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .and_then(|client| client.get(&fetch_url).send())
        {
            Ok(config_resp) if config_resp.status().is_success() => {
                if let Ok(apollo_resp) = config_resp.json::<ApolloResponse>() {
                    // 加锁获取 handlers 和 current_values
                    let handlers_guard = handlers.lock().unwrap();
                    let mut current_values_guard = current_values.lock().unwrap();

                    // 遍历所有已注册的 key
                    for (key, handler_list) in handlers_guard.iter() {
                        if let Some(config_value) = apollo_resp.configurations.get(key) {
                            // 解析新值
                            let new_value = if let Some(config_str) = config_value.as_str() {
                                serde_json::from_str(config_str).ok()
                            } else {
                                Some(config_value.clone())
                            };

                            if let Some(v) = new_value {
                                // 检查是否有变化
                                let has_changed = match current_values_guard.get(key) {
                                    Some(old_value) => old_value != &v,
                                    None => true,
                                };

                                if has_changed {
                                    // 更新当前值
                                    current_values_guard.insert(key.clone(), v.clone());

                                    // 触发所有 handlers
                                    for handler in handler_list {
                                        handler(ConfigChange::Updated(ConfigValue::new(v.clone())));
                                    }
                                }
                            } else {
                                // 解析失败，触发错误回调
                                for handler in handler_list {
                                    handler(ConfigChange::Error("解析配置失败".to_string()));
                                }
                            }
                        } else {
                            // 配置被删除
                            if current_values_guard.remove(key).is_some() {
                                for handler in handler_list {
                                    handler(ConfigChange::Deleted);
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {
                // 请求失败，触发所有 handlers 的错误回调
                let handlers_guard = handlers.lock().unwrap();
                for handler_list in handlers_guard.values() {
                    for handler in handler_list {
                        handler(ConfigChange::Error("重新加载配置失败".to_string()));
                    }
                }
            }
            _ => {}
        }
    }
}

impl_from!(ApolloSourceConfig => ApolloSource, expect: "创建 ApolloSource 失败");
impl_box_from!(ApolloSource => dyn ConfigSource);

impl ApolloSource {
    /// 从 Apollo 获取指定命名空间的完整配置
    fn fetch_namespace_config(&self) -> Result<ApolloResponse> {
        let url = format!(
            "{}/configs/{}/{}/{}",
            self.server_url, self.app_id, self.cluster, self.namespace
        );

        let resp = self
            .client
            .get(&url)
            .send()
            .map_err(|e| anyhow!("请求 Apollo 失败: {}", e))?;

        if !resp.status().is_success() {
            return Err(anyhow!("Apollo 返回错误状态: {}", resp.status()));
        }

        resp.json::<ApolloResponse>()
            .map_err(|e| anyhow!("解析 Apollo 响应失败: {}", e))
    }
}

impl ConfigSource for ApolloSource {
    fn load(&self, key: &str) -> Result<ConfigValue> {
        let apollo_resp = self.fetch_namespace_config()?;

        // 从 Apollo 配置中提取特定 key
        let config_value = apollo_resp
            .configurations
            .get(key)
            .ok_or_else(|| anyhow!("配置 key 不存在: {}", key))?;

        // Apollo 中的值可能是 JSON 字符串，也可能直接是 JSON 对象
        // 如果是字符串，先尝试解析；如果是对象，直接使用
        let value = if let Some(config_str) = config_value.as_str() {
            serde_json::from_str(config_str)?
        } else {
            config_value.clone()
        };
        Ok(ConfigValue::new(value))
    }

    fn watch(&self, key: &str, handler: Box<dyn Fn(ConfigChange) + Send + Sync + 'static>) -> Result<()> {
        // 加锁注册 handler
        let mut handlers = self.handlers.lock().unwrap();
        handlers
            .entry(key.to_string())
            .or_insert_with(Vec::new)
            .push(handler);

        // 记录当前配置值（用于后续对比变化）
        if let Ok(config) = self.load(key) {
            let mut current_values = self.current_values.lock().unwrap();
            current_values.insert(key.to_string(), config.as_value().clone());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apollo_source_new() -> Result<()> {
        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: "http://localhost:8080".to_string(),
            app_id: "test-app".to_string(),
            namespace: "application".to_string(),
            cluster: "default".to_string(),
        })?;
        assert_eq!(source.server_url, "http://localhost:8080");
        assert_eq!(source.app_id, "test-app");
        assert_eq!(source.namespace, "application");
        assert_eq!(source.cluster, "default");
        Ok(())
    }

    #[test]
    fn test_apollo_source_url_trim() -> Result<()> {
        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: "http://localhost:8080/".to_string(),
            app_id: "test-app".to_string(),
            namespace: "application".to_string(),
            cluster: "default".to_string(),
        })?;
        assert_eq!(source.server_url, "http://localhost:8080");
        Ok(())
    }

    #[test]
    fn test_apollo_source_config_defaults() {
        // 测试配置默认值
        let config: ApolloSourceConfig = serde_json::from_str(
            r#"{
            "server_url": "http://localhost:8080",
            "app_id": "test-app"
        }"#,
        )
        .unwrap();

        assert_eq!(config.namespace, "application");
        assert_eq!(config.cluster, "default");
    }

    #[test]
    fn test_apollo_source_load_with_mock() -> Result<()> {
        // 使用 mockito mock HTTP 响应
        let mut server = mockito::Server::new();

        let mock = server
            .mock("GET", "/configs/test-app/default/application")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "releaseKey": "20240101000000-abc123",
                "configurations": {
                    "database": "{\"host\":\"localhost\",\"port\":3306}"
                }
            }"#,
            )
            .create();

        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: server.url(),
            app_id: "test-app".to_string(),
            namespace: "application".to_string(),
            cluster: "default".to_string(),
        })?;

        let config = source.load("database")?;

        mock.assert();
        assert_eq!(
            config.as_value().get("host").and_then(|v| v.as_str()),
            Some("localhost")
        );
        assert_eq!(
            config.as_value().get("port").and_then(|v| v.as_i64()),
            Some(3306)
        );

        Ok(())
    }

    #[test]
    fn test_apollo_source_load_json_object() -> Result<()> {
        // 测试配置值是 JSON 对象而非字符串的情况
        let mut server = mockito::Server::new();

        let mock = server
            .mock("GET", "/configs/test-app/default/application")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "releaseKey": "20240101000000-abc123",
                "configurations": {
                    "redis": {"host": "127.0.0.1", "port": 6379}
                }
            }"#,
            )
            .create();

        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: server.url(),
            app_id: "test-app".to_string(),
            namespace: "application".to_string(),
            cluster: "default".to_string(),
        })?;

        let config = source.load("redis")?;

        mock.assert();
        assert_eq!(
            config.as_value().get("host").and_then(|v| v.as_str()),
            Some("127.0.0.1")
        );
        assert_eq!(
            config.as_value().get("port").and_then(|v| v.as_i64()),
            Some(6379)
        );

        Ok(())
    }

    #[test]
    fn test_apollo_source_load_key_not_found() -> Result<()> {
        let mut server = mockito::Server::new();

        let mock = server
            .mock("GET", "/configs/test-app/default/application")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "releaseKey": "20240101000000-abc123",
                "configurations": {}
            }"#,
            )
            .create();

        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: server.url(),
            app_id: "test-app".to_string(),
            namespace: "application".to_string(),
            cluster: "default".to_string(),
        })?;

        let result = source.load("nonexistent");

        mock.assert();
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("不存在"));

        Ok(())
    }

    #[test]
    fn test_apollo_source_load_server_error() -> Result<()> {
        let mut server = mockito::Server::new();

        let mock = server
            .mock("GET", "/configs/test-app/default/application")
            .with_status(500)
            .with_body("Internal Server Error")
            .create();

        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: server.url(),
            app_id: "test-app".to_string(),
            namespace: "application".to_string(),
            cluster: "default".to_string(),
        })?;

        let result = source.load("database");

        mock.assert();
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("错误状态") || error_msg.contains("500"));

        Ok(())
    }

    #[test]
    fn test_apollo_source_load_404() -> Result<()> {
        let mut server = mockito::Server::new();

        let mock = server
            .mock("GET", "/configs/test-app/default/application")
            .with_status(404)
            .with_body("Not Found")
            .create();

        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: server.url(),
            app_id: "test-app".to_string(),
            namespace: "application".to_string(),
            cluster: "default".to_string(),
        })?;

        let result = source.load("database");

        mock.assert();
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_apollo_source_from_config() {
        let config = ApolloSourceConfig {
            server_url: "http://localhost:8080".to_string(),
            app_id: "my-app".to_string(),
            namespace: "custom".to_string(),
            cluster: "prod".to_string(),
        };

        let source: ApolloSource = config.into();

        assert_eq!(source.server_url, "http://localhost:8080");
        assert_eq!(source.app_id, "my-app");
        assert_eq!(source.namespace, "custom");
        assert_eq!(source.cluster, "prod");
    }

    #[test]
    fn test_apollo_source_config_default_trait() {
        // 测试 SmartDefault 自动实现的 Default trait
        let config = ApolloSourceConfig::default();
        assert_eq!(config.namespace, "application");
        assert_eq!(config.cluster, "default");

        // 测试使用 Default 的结构体更新语法
        let config2 = ApolloSourceConfig {
            server_url: "http://localhost:8080".to_string(),
            app_id: "my-app".to_string(),
            ..Default::default()
        };
        assert_eq!(config2.server_url, "http://localhost:8080");
        assert_eq!(config2.app_id, "my-app");
        assert_eq!(config2.namespace, "application");
        assert_eq!(config2.cluster, "default");
    }
}
