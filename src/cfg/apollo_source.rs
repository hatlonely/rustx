//! Apollo 配置中心源
//!
//! 支持从 Apollo 配置中心加载配置，支持长轮询监听配置变化

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;

use super::source::{ConfigChange, ConfigSource, ConfigValue};
use crate::{impl_from, impl_box_from};

/// Apollo 配置中心源的配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApolloSourceConfig {
    /// Apollo 服务器地址，如 "http://localhost:8080"
    pub server_url: String,
    /// 应用 ID
    pub app_id: String,
    /// 命名空间，默认为 "application"
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// 集群名称，默认为 "default"
    #[serde(default = "default_cluster")]
    pub cluster: String,
}

fn default_namespace() -> String {
    "application".to_string()
}

fn default_cluster() -> String {
    "default".to_string()
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
}

impl ApolloSource {
    /// 创建 Apollo 配置源
    ///
    /// # 参数
    /// - `config`: Apollo 配置源配置
    pub fn new(config: ApolloSourceConfig) -> Result<Self> {
        Ok(Self {
            server_url: config.server_url.trim_end_matches('/').to_string(),
            app_id: config.app_id,
            namespace: config.namespace,
            cluster: config.cluster,
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(90))
                .build()?,
        })
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
        let url = format!("{}/notifications/v2", self.server_url);
        let client = self.client.clone();
        let app_id = self.app_id.clone();
        let cluster = self.cluster.clone();
        let namespace = self.namespace.clone();
        let key_owned = key.to_string();

        // 为了能在 watch 线程中调用 load，我们需要克隆必要的字段
        let server_url = self.server_url.clone();

        // 启动监听线程
        let _thread_handle = thread::spawn(move || {
            let mut notification_id = -1i64;
            let mut is_first_notification = true; // 标记是否是首次收到通知

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
                                        continue; // 跳过本次回调触发，继续下一轮长轮询
                                    }

                                    // 配置有更新，重新加载
                                    // 构建临时的 ApolloSource 来加载配置
                                    match reqwest::blocking::Client::builder()
                                        .timeout(Duration::from_secs(30))
                                        .build()
                                    {
                                        Ok(temp_client) => {
                                            let fetch_url = format!(
                                                "{}/configs/{}/{}/{}",
                                                server_url, app_id, cluster, namespace
                                            );

                                            match temp_client.get(&fetch_url).send() {
                                                Ok(config_resp)
                                                    if config_resp.status().is_success() =>
                                                {
                                                    if let Ok(apollo_resp) =
                                                        config_resp.json::<ApolloResponse>()
                                                    {
                                                        if let Some(config_value) = apollo_resp
                                                            .configurations
                                                            .get(&key_owned)
                                                        {
                                                            let value = if let Some(config_str) =
                                                                config_value.as_str()
                                                            {
                                                                serde_json::from_str(config_str)
                                                            } else {
                                                                Ok(config_value.clone())
                                                            };

                                                            match value {
                                                                Ok(v) => {
                                                                    handler(ConfigChange::Updated(
                                                                        ConfigValue::new(v),
                                                                    ));
                                                                }
                                                                Err(e) => {
                                                                    handler(ConfigChange::Error(
                                                                        format!(
                                                                            "解析配置失败: {}",
                                                                            e
                                                                        ),
                                                                    ));
                                                                }
                                                            }
                                                        } else {
                                                            handler(ConfigChange::Deleted);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    handler(ConfigChange::Error(format!(
                                                        "重新加载配置失败: {}",
                                                        e
                                                    )));
                                                }
                                                _ => {}
                                            }
                                        }
                                        Err(e) => {
                                            handler(ConfigChange::Error(format!(
                                                "创建 HTTP 客户端失败: {}",
                                                e
                                            )));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(resp) if resp.status().as_u16() == 304 => {
                        // 304 表示配置未变化，继续长轮询
                    }
                    Err(e) => {
                        // 网络错误或超时，等待后重试
                        handler(ConfigChange::Error(format!("请求失败: {}", e)));
                        thread::sleep(Duration::from_secs(5));
                    }
                    Ok(resp) => {
                        // 其他错误状态码
                        handler(ConfigChange::Error(format!(
                            "Apollo 返回错误状态: {}",
                            resp.status()
                        )));
                        thread::sleep(Duration::from_secs(5));
                    }
                }

                // 短暂休眠，避免紧密循环
                thread::sleep(Duration::from_millis(100));
            }
        });

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
}
