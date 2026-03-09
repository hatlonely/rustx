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

/// 命名空间状态
struct NamespaceState {
    /// 存储该 namespace 下 key -> (handler, format) 的映射
    handlers: HashMap<String, Vec<(Box<dyn Fn(ConfigChange) + Send + Sync + 'static>, Option<String>)>>,
    /// 存储该 namespace 下每个 key 当前配置值，用于对比变化
    current_values: HashMap<String, serde_json::Value>,
    /// 长轮询的 notification ID
    notification_id: i64,
}

/// Apollo 配置中心源
///
/// 支持从 Apollo 配置中心加载配置并监听变化
///
/// # Key 格式
/// - 使用 `namespace/key` 格式，例如 `application/database`、`redis.cache`
/// - namespace 和 key 都不能包含 `/` 字符
///
/// # 监听行为说明
/// - `watch` 方法仅监听配置**变化**，不会在启动时立即触发回调
/// - 如需获取初始配置，应先调用 `load` 方法，再调用 `watch` 监听后续变化
/// - 使用 Apollo 长轮询机制实现配置变更通知
/// - 每个 namespace 独立进行长轮询
///
/// # 示例
/// ```no_run
/// use rustx::cfg::{ConfigSource, ApolloSource, ApolloSourceConfig};
///
/// // 创建 Apollo 配置源
/// let source = ApolloSource::new(ApolloSourceConfig {
///     server_url: "http://localhost:8080".to_string(),
///     app_id: "my-app".to_string(),
///     cluster: "default".to_string(),
/// }).unwrap();
///
/// // 加载初始配置
/// let config = source.load("application/database").unwrap();
///
/// // 监听后续变化
/// source.watch("application/database", |change| {
///     // 仅在配置发生变化时才会触发
/// }).unwrap();
/// ```
pub struct ApolloSource {
    /// Apollo 服务器地址
    server_url: String,
    /// 应用 ID
    app_id: String,
    /// 集群名称
    cluster: String,
    /// HTTP 客户端
    client: reqwest::blocking::Client,
    /// 存储每个 namespace 的状态
    namespaces: Arc<Mutex<HashMap<String, Arc<Mutex<NamespaceState>>>>>,
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

        let source = Self {
            server_url,
            app_id,
            cluster,
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(90))
                .build()?,
            namespaces: Arc::new(Mutex::new(HashMap::new())),
        };

        Ok(source)
    }

    /// 解析 key，提取 namespace 和 key
    ///
    /// # 格式
    /// 输入格式为 `namespace/key`，例如 `application/database`
    ///
    /// # 返回
    /// 返回 `(namespace, key)`
    fn parse_key(key: &str) -> Result<(String, String)> {
        if key.is_empty() {
            return Err(anyhow!("key 不能为空"));
        }

        let parts: Vec<&str> = key.splitn(2, '/').collect();

        if parts.len() != 2 {
            return Err(anyhow!(
                "key 格式无效，应为 'namespace/key' 格式，当前为: '{}'",
                key
            ));
        }

        let namespace = parts[0];
        let key = parts[1];

        if namespace.is_empty() {
            return Err(anyhow!("namespace 不能为空"));
        }

        if key.is_empty() {
            return Err(anyhow!("key 不能为空"));
        }

        if namespace.contains('/') || key.contains('/') {
            return Err(anyhow!(
                "namespace 和 key 都不能包含 '/' 字符，当前为: '{}'",
                key
            ));
        }

        Ok((namespace.to_string(), key.to_string()))
    }

    /// 为指定的 namespace 启动长轮询监听线程
    fn start_watch_thread_for_namespace(&self, namespace: String) {
        let url = format!("{}/notifications/v2", self.server_url);
        let client = self.client.clone();
        let app_id = self.app_id.clone();
        let cluster = self.cluster.clone();
        let server_url = self.server_url.clone();
        let namespaces = self.namespaces.clone();

        thread::spawn(move || {
            let mut is_first_notification = true;

            loop {
                // 获取当前 notification_id
                let notification_id = {
                    let namespaces_guard = namespaces.lock().unwrap();
                    namespaces_guard
                        .get(&namespace)
                        .map(|state| state.lock().unwrap().notification_id)
                        .unwrap_or(-1)
                };

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
                                    // 更新 notification_id
                                    {
                                        let namespaces_guard = namespaces.lock().unwrap();
                                        if let Some(state) = namespaces_guard.get(&namespace) {
                                            state.lock().unwrap().notification_id = new_id;
                                        }
                                    }

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
                                        &namespaces,
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

    /// 确保指定 namespace 的长轮询线程已启动
    fn ensure_namespace_watched(&self, namespace: &str) {
        let mut namespaces = self.namespaces.lock().unwrap();
        if !namespaces.contains_key(namespace) {
            let state = Arc::new(Mutex::new(NamespaceState {
                handlers: HashMap::new(),
                current_values: HashMap::new(),
                notification_id: -1,
            }));
            namespaces.insert(namespace.to_string(), state.clone());
            drop(namespaces);

            // 启动长轮询线程
            self.start_watch_thread_for_namespace(namespace.to_string());
        }
    }

    /// 处理配置变更
    fn handle_config_change(
        server_url: &str,
        app_id: &str,
        cluster: &str,
        namespace: &str,
        namespaces: &Arc<Mutex<HashMap<String, Arc<Mutex<NamespaceState>>>>>,
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
                    // 在锁内完成所有操作
                    let namespaces_guard = namespaces.lock().unwrap();
                    if let Some(state) = namespaces_guard.get(namespace) {
                        let mut state_guard = state.lock().unwrap();

                        // 先收集所有需要处理的 keys，避免在迭代时修改 map
                        let keys: Vec<String> = state_guard.handlers.keys().cloned().collect();

                        for key in keys {
                            if let Some(config_value) = apollo_resp.configurations.get(&key) {
                                // 配置存在，检查是否更新
                                // 克隆 handlers 信息以避免借用冲突
                                let handlers_info: Vec<_> = state_guard.handlers.get(&key)
                                    .map(|list| list.iter().map(|(h, f)| (h as *const dyn Fn(ConfigChange), f.clone())).collect())
                                    .unwrap_or_default();

                                if !handlers_info.is_empty() {
                                    // 对每个 handler，使用其对应的 format 解析配置
                                    for (handler_ptr, format) in handlers_info {
                                        let handler = unsafe {
                                            &*handler_ptr
                                        };

                                        // 解析新值
                                        let new_value = if let Some(config_str) = config_value.as_str() {
                                            match Self::parse_config_with_format(config_str, format.as_deref()) {
                                                Ok(v) => Some(v),
                                                Err(e) => {
                                                    let format_name = Self::format_name(format.as_deref());
                                                    handler(ConfigChange::Error(format!("解析 {} 格式配置失败: {}", format_name, e)));
                                                    continue;
                                                }
                                            }
                                        } else {
                                            Some(config_value.clone())
                                        };

                                        if let Some(v) = new_value {
                                            // 检查是否有变化
                                            let has_changed = match state_guard.current_values.get(&key) {
                                                Some(old_value) => old_value != &v,
                                                None => true,
                                            };

                                            if has_changed {
                                                // 更新当前值
                                                state_guard.current_values.insert(key.clone(), v.clone());
                                                // 触发 handler
                                                handler(ConfigChange::Updated(ConfigValue::new(v)));
                                            }
                                        }
                                    }
                                }
                            } else {
                                // 配置不存在，检查是否删除
                                if state_guard.current_values.remove(&key).is_some() {
                                    if let Some(handler_list) = state_guard.handlers.get(&key) {
                                        for (handler, _) in handler_list.iter() {
                                            handler(ConfigChange::Deleted);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {
                // 请求失败，触发所有 handlers 的错误回调
                let namespaces_guard = namespaces.lock().unwrap();
                if let Some(state) = namespaces_guard.get(namespace) {
                    let state_guard = state.lock().unwrap();
                    for handler_list in state_guard.handlers.values() {
                        for (handler, _) in handler_list.iter() {
                            handler(ConfigChange::Error("重新加载配置失败".to_string()));
                        }
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
    /// 根据指定格式解析配置字符串
    fn parse_config_with_format(content: &str, format: Option<&str>) -> Result<serde_json::Value> {
        let format = format.unwrap_or("json");
        match format.to_lowercase().as_str() {
            "json" => Ok(serde_json::from_str(content)?),
            "json5" => Ok(json5::from_str(content)?),
            "yaml" => Ok(serde_yaml::from_str(content)?),
            "toml" => Ok(toml::from_str(content)?),
            _ => Err(anyhow!("不支持的配置格式: {}", format)),
        }
    }

    /// 格式名称（用于错误提示）
    fn format_name(format: Option<&str>) -> String {
        format.map(|s| s.to_uppercase()).unwrap_or_else(|| "JSON".to_string())
    }
}

impl ApolloSource {
    /// 从 Apollo 获取指定命名空间的完整配置
    fn fetch_namespace_config(&self, namespace: &str) -> Result<ApolloResponse> {
        let url = format!(
            "{}/configs/{}/{}/{}",
            self.server_url, self.app_id, self.cluster, namespace
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
    fn load(&self, key: &str, format: Option<&str>) -> Result<ConfigValue> {
        // 解析 key，提取 namespace 和真正的 key
        let (namespace, actual_key) = Self::parse_key(key)?;

        let apollo_resp = self.fetch_namespace_config(&namespace)?;

        // 从 Apollo 配置中提取特定 key
        let config_value = apollo_resp
            .configurations
            .get(&actual_key)
            .ok_or_else(|| anyhow!("配置 key 不存在: {}", key))?;

        // Apollo 中的值可能是 JSON 字符串，也可能直接是 JSON 对象
        // 如果是字符串，使用指定的格式解析；如果是对象，直接使用
        let value = if let Some(config_str) = config_value.as_str() {
            match Self::parse_config_with_format(config_str, format) {
                Ok(v) => v,
                Err(e) => {
                    let format_name = Self::format_name(format);
                    Err(anyhow!("解析 {} 格式配置失败: {}", format_name, e))?
                }
            }
        } else {
            config_value.clone()
        };
        Ok(ConfigValue::new(value))
    }

    fn watch(&self, key: &str, format: Option<&str>, handler: Box<dyn Fn(ConfigChange) + Send + Sync + 'static>) -> Result<()> {
        // 解析 key，提取 namespace 和真正的 key
        let (namespace, actual_key) = Self::parse_key(key)?;

        // 确保 namespace 已被监听
        self.ensure_namespace_watched(&namespace);

        // 注册 handler
        {
            let namespaces = self.namespaces.lock().unwrap();
            if let Some(state) = namespaces.get(&namespace) {
                let mut state_guard = state.lock().unwrap();
                state_guard
                    .handlers
                    .entry(actual_key.clone())
                    .or_insert_with(Vec::new)
                    .push((handler, format.map(|s| s.to_string())));
            }
        }

        // 记录当前配置值（用于后续对比变化）
        if let Ok(config) = self.load(key, format) {
            let namespaces = self.namespaces.lock().unwrap();
            if let Some(state) = namespaces.get(&namespace) {
                let mut state_guard = state.lock().unwrap();
                state_guard.current_values.insert(actual_key, config.as_value().clone());
            }
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
            cluster: "default".to_string(),
        })?;
        assert_eq!(source.server_url, "http://localhost:8080");
        assert_eq!(source.app_id, "test-app");
        assert_eq!(source.cluster, "default");
        Ok(())
    }

    #[test]
    fn test_apollo_source_url_trim() -> Result<()> {
        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: "http://localhost:8080/".to_string(),
            app_id: "test-app".to_string(),
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

        assert_eq!(config.cluster, "default");
    }

    #[test]
    fn test_apollo_source_parse_key() {
        // 测试正常的 key 解析
        let (namespace, key) = ApolloSource::parse_key("application/database").unwrap();
        assert_eq!(namespace, "application");
        assert_eq!(key, "database");

        let (namespace, key) = ApolloSource::parse_key("redis.cache/config").unwrap();
        assert_eq!(namespace, "redis.cache");
        assert_eq!(key, "config");

        // 测试错误的 key 格式
        assert!(ApolloSource::parse_key("database").is_err());
        assert!(ApolloSource::parse_key("/database").is_err());
        assert!(ApolloSource::parse_key("application/").is_err());
        assert!(ApolloSource::parse_key("").is_err());
    }

    #[test]
    fn test_apollo_source_parse_key_with_slash() {
        // 测试 key 中包含 / 的情况
        assert!(ApolloSource::parse_key("application/db/config").is_err());
        assert!(ApolloSource::parse_key("app/v1/config").is_err());
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
            cluster: "default".to_string(),
        })?;

        let config = source.load("application/database", None)?;

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
            cluster: "default".to_string(),
        })?;

        let config = source.load("application/redis", None)?;

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
            cluster: "default".to_string(),
        })?;

        let result = source.load("application/nonexistent", None);

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
            cluster: "default".to_string(),
        })?;

        let result = source.load("application/database", None);

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
            cluster: "default".to_string(),
        })?;

        let result = source.load("application/database", None);

        mock.assert();
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_apollo_source_from_config() {
        let config = ApolloSourceConfig {
            server_url: "http://localhost:8080".to_string(),
            app_id: "my-app".to_string(),
            cluster: "prod".to_string(),
        };

        let source: ApolloSource = config.into();

        assert_eq!(source.server_url, "http://localhost:8080");
        assert_eq!(source.app_id, "my-app");
        assert_eq!(source.cluster, "prod");
    }

    #[test]
    fn test_apollo_source_config_default_trait() {
        // 测试 SmartDefault 自动实现的 Default trait
        let config = ApolloSourceConfig::default();
        assert_eq!(config.cluster, "default");

        // 测试使用 Default 的结构体更新语法
        let config2 = ApolloSourceConfig {
            server_url: "http://localhost:8080".to_string(),
            app_id: "my-app".to_string(),
            ..Default::default()
        };
        assert_eq!(config2.server_url, "http://localhost:8080");
        assert_eq!(config2.app_id, "my-app");
        assert_eq!(config2.cluster, "default");
    }

    #[test]
    fn test_apollo_source_load_with_yaml_format() -> Result<()> {
        // 测试从 Apollo 加载 YAML 格式的配置
        let mut server = mockito::Server::new();

        // mock notifications 请求
        let _notify_mock = server
            .mock("GET", "/notifications/v2")
            .with_status(304)
            .create();

        let mock = server
            .mock("GET", "/configs/test-app/default/application")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "releaseKey": "20240101000000-xyz789",
                "configurations": {
                    "database": "host: localhost\nport: 3306"
                }
            }"#,
            )
            .create();

        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: server.url(),
            app_id: "test-app".to_string(),
            cluster: "default".to_string(),
        })?;

        // 使用 YAML 格式加载
        let config = source.load("application/database", Some("yaml"))?;

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
    fn test_apollo_source_load_with_toml_format() -> Result<()> {
        // 测试从 Apollo 加载 TOML 格式的配置
        let mut server = mockito::Server::new();

        // mock notifications 请求
        let _notify_mock = server
            .mock("GET", "/notifications/v2")
            .with_status(304)
            .create();

        let mock = server
            .mock("GET", "/configs/test-app/default/application")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "releaseKey": "20240101000000-toml123",
                "configurations": {
                    "cache": "host = \"127.0.0.1\"\nport = 6379"
                }
            }"#,
            )
            .create();

        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: server.url(),
            app_id: "test-app".to_string(),
            cluster: "default".to_string(),
        })?;

        // 使用 TOML 格式加载
        let config = source.load("application/cache", Some("toml"))?;

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
    fn test_apollo_source_load_case_insensitive_format() -> Result<()> {
        // 测试格式名称大小写不敏感
        let mut server = mockito::Server::new();

        // mock notifications 请求，避免长轮询线程干扰测试
        let _notify_mock = server
            .mock("GET", "/notifications/v2")
            .with_status(304)
            .expect_at_least(1)
            .create();

        let mock = server
            .mock("GET", "/configs/test-app/default/application")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "releaseKey": "20240101000000-case123",
                "configurations": {
                    "test": "{\"value\": 42}"
                }
            }"#,
            )
            .expect_at_least(1)
            .create();

        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: server.url(),
            app_id: "test-app".to_string(),
            cluster: "default".to_string(),
        })?;

        // 测试大小写不敏感
        let config = source.load("application/test", Some("JSON"))?;
        assert_eq!(config.as_value()["value"], 42);

        let config = source.load("application/test", Some("Json"))?;
        assert_eq!(config.as_value()["value"], 42);

        mock.assert();

        Ok(())
    }

    #[test]
    fn test_apollo_source_load_unsupported_format() {
        // 测试不支持的格式
        let mut server = mockito::Server::new();

        // mock notifications 请求
        let _notify_mock = server
            .mock("GET", "/notifications/v2")
            .with_status(304)
            .create();

        let mock = server
            .mock("GET", "/configs/test-app/default/application")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "releaseKey": "20240101000000-xml123",
                "configurations": {
                    "xml_config": "<config>value</config>"
                }
            }"#,
            )
            .create();

        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: server.url(),
            app_id: "test-app".to_string(),
            cluster: "default".to_string(),
        })
        .unwrap();

        let result = source.load("application/xml_config", Some("xml"));

        mock.assert();
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("不支持的配置格式") || error_msg.contains("xml"));
    }

    #[test]
    fn test_apollo_source_load_invalid_format_content() {
        // 测试格式内容无效
        let mut server = mockito::Server::new();

        // mock notifications 请求
        let _notify_mock = server
            .mock("GET", "/notifications/v2")
            .with_status(304)
            .create();

        let mock = server
            .mock("GET", "/configs/test-app/default/application")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "releaseKey": "20240101000000-invalid",
                "configurations": {
                    "invalid_yaml": ": invalid yaml content"
                }
            }"#,
            )
            .create();

        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: server.url(),
            app_id: "test-app".to_string(),
            cluster: "default".to_string(),
        })
        .unwrap();

        let result = source.load("application/invalid_yaml", Some("yaml"));

        mock.assert();
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("YAML 格式配置失败") || error_msg.contains("解析"));
    }
}
