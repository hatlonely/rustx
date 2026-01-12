//! cfg 模块 - 配置管理
//!
//! 提供零耦合的配置管理系统，支持多种配置来源

// 模块声明
pub mod apollo_source;
pub mod file_source;
pub mod registry;
pub mod serde_duration;
pub mod source;
pub mod type_options;

// 重新导出公共 API
pub use apollo_source::{ApolloSource, ApolloSourceConfig};
pub use file_source::{FileSource, FileSourceConfig};
pub use registry::{create_from_type_options, register, register_with_name};
pub use source::{ConfigChange, ConfigSource, ConfigValue};
pub use type_options::TypeOptions;
