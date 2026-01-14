//! Apollo 配置中心集成测试
//!
//! 这些测试需要运行实际的 Apollo 服务：
//! ```bash
//! cd tests/deploy/apollo && docker compose up -d
//! ```
//!
//! 运行测试：
//! ```bash
//! cargo test --test test_cfg apollo_integration -- --ignored
//! ```

use anyhow::Result;
use rustx::cfg::{ApolloSource, ApolloSourceConfig, ConfigChange, ConfigSource};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

/// Apollo 服务配置
const APOLLO_SERVER_URL: &str = "http://localhost:8080";
const APOLLO_PORTAL_URL: &str = "http://localhost:8070";
const APOLLO_APP_ID: &str = "test-app";
const APOLLO_CLUSTER: &str = "default";
const APOLLO_NAMESPACE: &str = "application";
const APOLLO_TOKEN: &str = "rustx-test-token-20240101";

/// Apollo OpenAPI 客户端，用于测试时动态修改配置
struct ApolloOpenApiClient {
    portal_url: String,
    token: String,
    client: reqwest::blocking::Client,
}

impl ApolloOpenApiClient {
    fn new(portal_url: &str, token: &str) -> Self {
        Self {
            portal_url: portal_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            client: reqwest::blocking::Client::new(),
        }
    }

    /// 更新配置项
    fn update_item(
        &self,
        app_id: &str,
        env: &str,
        cluster: &str,
        namespace: &str,
        key: &str,
        value: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/openapi/v1/envs/{}/apps/{}/clusters/{}/namespaces/{}/items/{}",
            self.portal_url, env, app_id, cluster, namespace, key
        );

        let resp = self
            .client
            .put(&url)
            .header("Authorization", &self.token)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "key": key,
                "value": value,
                "dataChangeLastModifiedBy": "apollo"
            }))
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("更新配置失败: {} - {}", status, body);
        }

        Ok(())
    }

    /// 创建配置项
    #[allow(dead_code)]
    fn create_item(
        &self,
        app_id: &str,
        env: &str,
        cluster: &str,
        namespace: &str,
        key: &str,
        value: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/openapi/v1/envs/{}/apps/{}/clusters/{}/namespaces/{}/items",
            self.portal_url, env, app_id, cluster, namespace
        );

        let resp = self
            .client
            .post(&url)
            .header("Authorization", &self.token)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "key": key,
                "value": value,
                "dataChangeCreatedBy": "apollo"
            }))
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("创建配置失败: {} - {}", status, body);
        }

        Ok(())
    }

    /// 发布配置
    fn release(
        &self,
        app_id: &str,
        env: &str,
        cluster: &str,
        namespace: &str,
        release_title: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/openapi/v1/envs/{}/apps/{}/clusters/{}/namespaces/{}/releases",
            self.portal_url, env, app_id, cluster, namespace
        );

        let resp = self
            .client
            .post(&url)
            .header("Authorization", &self.token)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "releaseTitle": release_title,
                "releasedBy": "apollo"
            }))
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("发布配置失败: {} - {}", status, body);
        }

        Ok(())
    }
}

/// 创建 Apollo 配置源
fn create_apollo_source() -> Result<ApolloSource> {
    ApolloSource::new(ApolloSourceConfig {
        server_url: APOLLO_SERVER_URL.to_string(),
        app_id: APOLLO_APP_ID.to_string(),
        namespace: APOLLO_NAMESPACE.to_string(),
        cluster: APOLLO_CLUSTER.to_string(),
    })
}

/// 创建 OpenAPI 客户端
fn create_openapi_client() -> ApolloOpenApiClient {
    ApolloOpenApiClient::new(APOLLO_PORTAL_URL, APOLLO_TOKEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试从 Apollo 加载 database 配置
    #[test]
    fn test_load_database_config() -> Result<()> {
        let source = create_apollo_source()?;

        // 加载预置的 database 配置
        let config = source.load("database")?;

        // 验证配置内容 - 配置结构为 {type, options: {...}}
        let value = config.as_value();
        let options = value.get("options").expect("应该有 options 字段");
        assert_eq!(
            options.get("host").and_then(|v| v.as_str()),
            Some("localhost")
        );
        assert_eq!(options.get("port").and_then(|v| v.as_i64()), Some(3306));
        assert_eq!(
            options.get("username").and_then(|v| v.as_str()),
            Some("root")
        );
        assert_eq!(
            options.get("database").and_then(|v| v.as_str()),
            Some("test_db")
        );

        Ok(())
    }

    /// 测试加载 redis 配置
    #[test]
    fn test_load_redis_config() -> Result<()> {
        let source = create_apollo_source()?;

        let config = source.load("redis")?;

        // 验证配置内容 - 配置结构为 {type, options: {...}}
        let value = config.as_value();
        let options = value.get("options").expect("应该有 options 字段");
        assert_eq!(
            options.get("host").and_then(|v| v.as_str()),
            Some("localhost")
        );
        assert_eq!(options.get("port").and_then(|v| v.as_i64()), Some(6379));

        Ok(())
    }

    /// 测试加载不存在的 key
    #[test]
    fn test_load_nonexistent_key() -> Result<()> {
        let source = create_apollo_source()?;

        let result = source.load("nonexistent_key");

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("不存在") || error_msg.contains("not exist"));

        Ok(())
    }

    /// 测试监听配置变化
    ///
    /// 测试流程：
    /// 1. 创建 watch 监听器
    /// 2. 通过 OpenAPI 更新配置
    /// 3. 发布配置
    /// 4. 验证 watch 回调被触发
    #[test]
    fn test_watch_config_change() -> Result<()> {
        let source = create_apollo_source()?;
        let openapi = create_openapi_client();

        // 用于收集配置变更
        let changes: Arc<RwLock<Vec<ConfigChange>>> = Arc::new(RwLock::new(Vec::new()));
        let changes_clone = changes.clone();

        // 启动监听
        source.watch("database", Box::new(move |change| {
            println!("收到配置变更: {:?}", change);
            changes_clone.write().unwrap().push(change);
        }))?;

        // 等待 watch 线程启动
        thread::sleep(Duration::from_secs(2));

        // 修改配置 - 配置结构为 {type, options: {...}}
        let new_value = r#"{"type":"DatabaseService","options":{"host":"new-host","port":3307,"username":"root","password":"new-secret","database":"test_db","max_connections":20}}"#;
        openapi.update_item(
            APOLLO_APP_ID,
            "DEV",
            APOLLO_CLUSTER,
            APOLLO_NAMESPACE,
            "database",
            new_value,
        )?;

        // 发布配置
        openapi.release(
            APOLLO_APP_ID,
            "DEV",
            APOLLO_CLUSTER,
            APOLLO_NAMESPACE,
            "Test release for watch",
        )?;

        // 等待 watch 回调触发 (Apollo 长轮询最长 90 秒)
        let mut received = false;
        for _ in 0..30 {
            thread::sleep(Duration::from_secs(3));
            let changes_guard = changes.read().unwrap();
            if !changes_guard.is_empty() {
                received = true;
                // 验证收到的是更新事件
                if let Some(ConfigChange::Updated(config)) = changes_guard.last() {
                    let value = config.as_value();
                    let options = value.get("options").expect("应该有 options 字段");
                    assert_eq!(
                        options.get("host").and_then(|v| v.as_str()),
                        Some("new-host")
                    );
                    assert_eq!(options.get("port").and_then(|v| v.as_i64()), Some(3307));
                }
                break;
            }
        }

        // 恢复原始配置 - 配置结构为 {type, options: {...}}
        let original_value = r#"{"type":"DatabaseService","options":{"host":"localhost","port":3306,"username":"root","password":"secret","database":"test_db","max_connections":10}}"#;
        openapi.update_item(
            APOLLO_APP_ID,
            "DEV",
            APOLLO_CLUSTER,
            APOLLO_NAMESPACE,
            "database",
            original_value,
        )?;
        openapi.release(
            APOLLO_APP_ID,
            "DEV",
            APOLLO_CLUSTER,
            APOLLO_NAMESPACE,
            "Restore original config",
        )?;

        assert!(received, "应该收到配置变更通知");

        Ok(())
    }

    /// 测试 watch 连接错误处理
    #[test]
    fn test_watch_error_handling() -> Result<()> {
        // 使用错误的服务器地址
        let source = ApolloSource::new(ApolloSourceConfig {
            server_url: "http://localhost:19999".to_string(), // 不存在的端口
            app_id: APOLLO_APP_ID.to_string(),
            namespace: APOLLO_NAMESPACE.to_string(),
            cluster: APOLLO_CLUSTER.to_string(),
        })?;

        let errors: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
        let errors_clone = errors.clone();

        source.watch("database", Box::new(move |change| {
            if let ConfigChange::Error(msg) = change {
                errors_clone.write().unwrap().push(msg);
            }
        }))?;

        // 等待错误回调
        thread::sleep(Duration::from_secs(10));

        let errors_guard = errors.read().unwrap();
        assert!(!errors_guard.is_empty(), "应该收到错误回调");

        Ok(())
    }

    /// 测试 ApolloSource 配置默认值
    #[test]
    fn test_apollo_source_config_defaults() {
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
}
