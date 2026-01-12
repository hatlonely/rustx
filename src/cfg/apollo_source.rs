//! Apollo 配置中心源
//!
//! 支持从 Apollo 配置中心加载配置，支持长轮询监听配置变化

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use super::source::{ConfigSource, ConfigChange, WatchHandle};
use super::type_options::TypeOptions;

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
/// // 加载配置
/// let config = source.load("database").unwrap();
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
    /// 内部维护所有监听句柄
    watches: Arc<Mutex<Vec<WatchHandle>>>,
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
            watches: Arc::new(Mutex::new(Vec::new())),
        })
    }
}

impl From<ApolloSourceConfig> for ApolloSource {
    fn from(config: ApolloSourceConfig) -> Self {
        ApolloSource::new(config).expect("创建 ApolloSource 失败")
    }
}

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
            return Err(anyhow!(
                "Apollo 返回错误状态: {}",
                resp.status()
            ));
        }

        resp.json::<ApolloResponse>()
            .map_err(|e| anyhow!("解析 Apollo 响应失败: {}", e))
    }
}

impl ConfigSource for ApolloSource {
    fn load(&self, key: &str) -> Result<TypeOptions> {
        let apollo_resp = self.fetch_namespace_config()?;

        // 从 Apollo 配置中提取特定 key
        let config_value = apollo_resp
            .configurations
            .get(key)
            .ok_or_else(|| anyhow!("配置 key 不存在: {}", key))?;

        // Apollo 中的值可能是 JSON 字符串，也可能直接是 JSON 对象
        // 如果是字符串，先尝试解析；如果是对象，直接使用
        if let Some(config_str) = config_value.as_str() {
            TypeOptions::from_json(config_str)
        } else {
            TypeOptions::from_json(&config_value.to_string())
        }
    }

    fn watch<F>(&self, key: &str, handler: F) -> Result<()>
    where
        F: Fn(ConfigChange) + Send + 'static,
    {
        // 创建停止信号通道
        let (stop_tx, stop_rx) = crossbeam::channel::unbounded();

        let url = format!("{}/notifications/v2", self.server_url);
        let client = self.client.clone();
        let app_id = self.app_id.clone();
        let cluster = self.cluster.clone();
        let namespace = self.namespace.clone();
        let key_owned = key.to_string();

        // 为了能在 watch 线程中调用 load，我们需要克隆必要的字段
        let server_url = self.server_url.clone();

        // 启动监听线程
        let thread_handle = thread::spawn(move || {
            let mut notification_id = -1i64;

            loop {
                // 检查停止信号（非阻塞）
                if stop_rx.try_recv().is_ok() {
                    break;
                }

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
                                                Ok(config_resp) if config_resp.status().is_success() => {
                                                    if let Ok(apollo_resp) = config_resp.json::<ApolloResponse>() {
                                                        if let Some(config_value) = apollo_resp.configurations.get(&key_owned) {
                                                            let result = if let Some(config_str) = config_value.as_str() {
                                                                TypeOptions::from_json(config_str)
                                                            } else {
                                                                TypeOptions::from_json(&config_value.to_string())
                                                            };

                                                            match result {
                                                                Ok(config) => {
                                                                    handler(ConfigChange::Updated(config));
                                                                }
                                                                Err(e) => {
                                                                    handler(ConfigChange::Error(
                                                                        format!("解析配置失败: {}", e)
                                                                    ));
                                                                }
                                                            }
                                                        } else {
                                                            handler(ConfigChange::Deleted);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    handler(ConfigChange::Error(
                                                        format!("重新加载配置失败: {}", e)
                                                    ));
                                                }
                                                _ => {}
                                            }
                                        }
                                        Err(e) => {
                                            handler(ConfigChange::Error(
                                                format!("创建 HTTP 客户端失败: {}", e)
                                            ));
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

        // 将 handle 存储到内部
        let handle = WatchHandle {
            stop_sender: Some(stop_tx),
            thread_handle: Some(thread_handle),
        };
        self.watches.lock().unwrap().push(handle);

        Ok(())
    }
}

// 注意：ApolloSource 不需要显式实现 Drop
// 当 ApolloSource drop 时，watches: Arc<Mutex<Vec<WatchHandle>>> 会自动 drop
// 进而触发每个 WatchHandle 的 Drop，自动停止所有监听线程

#[cfg(test)]
mod tests {
    use super::*;

    // 注意：这些测试需要 Apollo 服务器运行
    // 可以使用 docker-compose 快速启动 Apollo 服务
    // 实际项目中可以使用 mock server 进行测试

    #[test]
    #[ignore] // 需要 Apollo 服务器运行
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
    #[ignore] // 需要 Apollo 服务器运行
    fn test_apollo_source_load() -> Result<()> {
        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: "http://localhost:8080".to_string(),
            app_id: "test-app".to_string(),
            namespace: "application".to_string(),
            cluster: "default".to_string(),
        })?;

        // 假设 Apollo 中有 "database" 配置
        let config = source.load("database")?;
        assert_eq!(config.type_name, "DatabaseService");

        Ok(())
    }

    #[test]
    #[ignore] // 需要 Apollo 服务器运行
    fn test_apollo_source_watch() -> Result<()> {
        use std::sync::{Arc, RwLock};

        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: "http://localhost:8080".to_string(),
            app_id: "test-app".to_string(),
            namespace: "application".to_string(),
            cluster: "default".to_string(),
        })?;

        let changes = Arc::new(RwLock::new(Vec::new()));
        let changes_clone = changes.clone();

        source.watch("database", move |change| {
            changes_clone.write().unwrap().push(change);
        })?;

        // 等待一段时间观察变更
        thread::sleep(Duration::from_secs(10));

        Ok(())
    }
}
