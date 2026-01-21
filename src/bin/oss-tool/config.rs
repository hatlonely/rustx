// Configuration loading for oss-tool

use anyhow::{Context, Result};
use rustx::cfg::TypeOptions;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// OSS tool configuration
#[derive(Debug, Deserialize, Serialize)]
pub struct OssToolConfig {
    /// ObjectStore configuration list
    pub stores: Vec<TypeOptions>,

    /// Default options
    #[serde(default)]
    pub defaults: DefaultOptions,
}

/// Default options for operations
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DefaultOptions {
    /// Concurrent operations count (default: 4)
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,

    /// Part size for multipart upload (default: 8MB)
    #[serde(default = "default_part_size")]
    pub part_size: usize,

    /// Threshold for multipart upload (default: 100MB)
    #[serde(default = "default_multipart_threshold")]
    pub multipart_threshold: u64,
}

impl Default for DefaultOptions {
    fn default() -> Self {
        Self {
            concurrency: default_concurrency(),
            part_size: default_part_size(),
            multipart_threshold: default_multipart_threshold(),
        }
    }
}

fn default_concurrency() -> usize {
    4
}

fn default_part_size() -> usize {
    8 * 1024 * 1024 // 8MB
}

fn default_multipart_threshold() -> u64 {
    100 * 1024 * 1024 // 100MB
}

impl OssToolConfig {
    /// Load configuration from file
    pub fn load(config_path: Option<&str>) -> Result<Self> {
        let path = match config_path {
            Some(p) => PathBuf::from(shellexpand::tilde(p).to_string()),
            None => Self::default_config_path()?,
        };

        if !path.exists() {
            return Err(anyhow::anyhow!(
                "Config file not found: {}. Please create it first.",
                path.display()
            ));
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        // Expand environment variables in the config content
        let expanded_content = Self::expand_env_vars(&content);

        let config: Self = serde_yaml::from_str(&expanded_content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }

    /// Get default config path (~/.oss-tool/config.yaml)
    pub fn default_config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        Ok(home.join(".oss-tool").join("config.yaml"))
    }

    /// Expand environment variables in the format ${VAR_NAME}
    fn expand_env_vars(content: &str) -> String {
        let mut result = content.to_string();
        let re = regex_lite::Regex::new(r"\$\{([^}]+)\}").unwrap();

        for cap in re.captures_iter(content) {
            let var_name = &cap[1];
            let full_match = &cap[0];
            if let Ok(value) = std::env::var(var_name) {
                result = result.replace(full_match, &value);
            }
        }

        result
    }

    /// Find store configuration by bucket name
    pub fn find_store_config(&self, bucket: &str) -> Option<&TypeOptions> {
        self.stores.iter().find(|store| {
            store
                .options
                .get("bucket")
                .and_then(|v| v.as_str())
                .map(|b| b == bucket)
                .unwrap_or(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_VAR", "test_value");
        let content = "key: ${TEST_VAR}";
        let expanded = OssToolConfig::expand_env_vars(content);
        assert_eq!(expanded, "key: test_value");
        std::env::remove_var("TEST_VAR");
    }

    #[test]
    fn test_default_options() {
        let defaults = DefaultOptions::default();
        assert_eq!(defaults.concurrency, 4);
        assert_eq!(defaults.part_size, 8 * 1024 * 1024);
        assert_eq!(defaults.multipart_threshold, 100 * 1024 * 1024);
    }
}
