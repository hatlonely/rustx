//! cfg 模块 - 配置管理
//!
//! 提供零耦合的配置管理系统

// 模块声明
pub mod config;
pub mod registry;
pub mod type_options;
pub mod serde_duration;

// 重新导出公共 API
pub use config::TypeOptions;
pub use registry::{register_with_name, register, create_from_type_options};
