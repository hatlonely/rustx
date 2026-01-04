//! cfg 模块 - 配置管理
//! 
//! 提供与 Golang 版本功能对等的配置抽象接口

// 模块声明
pub mod config;
pub mod registry;
pub mod serialization;
pub mod duration;

// 重新导出公共 API
pub use config::{Configurable, TypeOptions};
pub use registry::{register, register_type, create_from_type_options};