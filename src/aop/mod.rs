//! AOP (Aspect-Oriented Programming) 模块
//!
//! 提供切面编程支持，集成常见的切面功能：
//! - Logging: 记录方法调用的开始、结束、耗时、结果
//! - Retry: 支持多种退避策略的重试机制
//! - Tracing: OpenTelemetry 分布式追踪
//! - Metric: Prometheus 指标收集
//!
//! # 使用示例
//!
//! ```ignore
//! use rustx::aop::{Aop, AopConfig};
//!
//! pub struct MyServiceConfig {
//!     pub aop: Option<AopConfig>,
//! }
//!
//! pub struct MyService {
//!     client: SomeClient,
//!     aop: Option<Aop>,
//! }
//!
//! impl MyService {
//!     pub async fn get_value(&self, key: &str) -> Result<String> {
//!         aop!(&self.aop, self.client.get(key).await)
//!     }
//! }
//! ```

pub mod aop;
pub mod aop_manager;
pub mod global_aop_manager;
pub mod macros;
pub mod metric;
pub mod tracer;

pub use aop::{
    Aop, AopConfig, AopCreateConfig, LoggingConfig, MetricConfig, RetryConfig, TracingConfig,
};
pub use aop_manager::{AopManager, AopManagerConfig};
pub use global_aop_manager::{
    add, contains, get, get_default, get_or_default, init, keys, remove, set_default,
};
pub use metric::{global_registry, init_metric, MetricServerConfig};
pub use tracer::{
    init_tracer, BatchProcessorConfig, ExporterConfig, OtlpExporterConfig, TracerConfig,
};
