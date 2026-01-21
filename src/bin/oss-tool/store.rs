// Store matching and management

use anyhow::{anyhow, Result};
use rustx::cfg::{create_trait_from_type_options, TypeOptions};
use rustx::oss::{register_object_store, ObjectStore};
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::OssToolConfig;
use crate::uri::OssUri;

/// Store manager for caching and retrieving ObjectStore instances
pub struct StoreManager {
    config: OssToolConfig,
    cache: HashMap<String, Arc<dyn ObjectStore>>,
}

impl StoreManager {
    /// Create a new StoreManager with the given configuration
    pub fn new(config: OssToolConfig) -> Self {
        // Register ObjectStore implementations
        register_object_store();

        Self {
            config,
            cache: HashMap::new(),
        }
    }

    /// Get or create an ObjectStore for the given URI
    pub fn get_store(&mut self, uri: &OssUri) -> Result<Arc<dyn ObjectStore>> {
        // Check cache first
        if let Some(store) = self.cache.get(&uri.bucket) {
            return Ok(Arc::clone(store));
        }

        // Find matching store configuration
        let store_config = self
            .config
            .find_store_config(&uri.bucket)
            .ok_or_else(|| anyhow!("No store configured for bucket: {}", uri.bucket))?;

        // Create store instance
        let store = create_store_from_config(store_config)?;
        let store = Arc::from(store);

        // Cache the store
        self.cache.insert(uri.bucket.clone(), Arc::clone(&store));

        Ok(store)
    }

    /// Get default options from config
    pub fn defaults(&self) -> &crate::config::DefaultOptions {
        &self.config.defaults
    }
}

/// Create an ObjectStore from TypeOptions configuration
fn create_store_from_config(config: &TypeOptions) -> Result<Box<dyn ObjectStore>> {
    create_trait_from_type_options::<dyn ObjectStore>(config)
        .map_err(|e| anyhow!("Failed to create ObjectStore: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_config() -> OssToolConfig {
        OssToolConfig {
            stores: vec![TypeOptions {
                type_name: "AwsS3ObjectStore".to_string(),
                options: json!({
                    "bucket": "test-bucket",
                    "region": "us-east-1"
                }),
            }],
            defaults: Default::default(),
        }
    }

    #[test]
    fn test_store_manager_creation() {
        let config = create_test_config();
        let _manager = StoreManager::new(config);
    }
}
