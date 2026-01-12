//! cfg 模块 - 配置管理
//!
//! 提供零耦合的配置管理系统，支持多种配置来源

// 模块声明
pub mod registry;
pub mod type_options;
pub mod serde_duration;
pub mod source;
pub mod file_source;
pub mod apollo_source;

// 重新导出公共 API
pub use type_options::TypeOptions;
pub use registry::{register_with_name, register, create_from_type_options};
pub use source::{ConfigSource, ConfigChange};
pub use file_source::FileSource;
pub use apollo_source::ApolloSource;
