use crate::cfg::serde_duration::{serde_as, HumanDur};
use anyhow::Result;
use garde::Validate;
use opentelemetry::global;
use opentelemetry_otlp::{WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::trace::{BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use serde::Deserialize;
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::Level;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::prelude::*;

/// gRPC tracing layer for tonic
pub use tonic_tracing_opentelemetry::middleware::server::OtelGrpcLayer;

/// HTTP tracing layer for tower/axum
pub type HttpTraceLayer = tower_http::trace::TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>;

/// 全局 Tracing 配置
#[derive(Debug, Clone, Deserialize, SmartDefault, Validate, PartialEq)]
#[serde(default)]
pub struct GlobalTracingConfig {
    /// 是否启用 tracing
    #[default = false]
    #[garde(skip)]
    pub enabled: bool,

    /// 服务名称
    #[garde(length(min = 1))]
    #[default = "rustx-service"]
    pub service_name: String,

    /// 采样率（0.0 - 1.0）
    #[default = 1.0]
    #[garde(range(min = 0.0, max = 1.0))]
    pub sample_rate: f32,

    /// Exporter 配置
    #[serde(default)]
    #[garde(skip)]
    pub exporter: ExporterConfig,

    /// BatchSpanProcessor 配置
    #[serde(default)]
    #[garde(skip)]
    pub batch_processor: Option<BatchProcessorConfig>,

    /// tracing_subscriber 配置
    #[serde(default)]
    #[garde(skip)]
    pub subscriber: SubscriberConfig,
}

/// Exporter 配置
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ExporterConfig {
    /// OTLP Exporter
    Otlp(OtlpExporterConfig),
    /// Stdout Exporter（用于调试）
    Stdout {},
    /// 不导出
    None {},
}

impl ExporterConfig {
    pub fn is_none(&self) -> bool {
        matches!(self, ExporterConfig::None {})
    }
}

impl Default for ExporterConfig {
    fn default() -> Self {
        ExporterConfig::None {}
    }
}

/// OTLP Exporter 配置
#[serde_as]
#[derive(Debug, Clone, Deserialize, SmartDefault, Validate, PartialEq)]
#[serde(default)]
pub struct OtlpExporterConfig {
    /// OTLP endpoint
    #[default = "http://localhost:4317"]
    #[garde(length(min = 1))]
    pub endpoint: String,

    /// 请求头（用于认证等）
    #[garde(skip)]
    pub headers: Option<HashMap<String, String>>,

    /// 超时时间
    #[serde_as(as = "HumanDur")]
    #[default(Duration::from_secs(10))]
    #[garde(skip)]
    pub timeout: Duration,
}

/// BatchSpanProcessor 配置
#[serde_as]
#[derive(Debug, Clone, Deserialize, SmartDefault, PartialEq)]
#[serde(default)]
pub struct BatchProcessorConfig {
    /// 导出间隔
    #[serde_as(as = "HumanDur")]
    #[default(Duration::from_secs(5))]
    pub scheduled_delay: Duration,

    /// 最大队列大小
    #[default = 2048]
    pub max_queue_size: usize,

    /// 最大导出批次大小
    #[default = 512]
    pub max_export_batch_size: usize,

    /// 最大并发导出数量
    #[default = 1]
    pub max_concurrent_exports: usize,
}

/// tracing_subscriber 配置
#[derive(Debug, Clone, Deserialize, SmartDefault, PartialEq)]
#[serde(default)]
pub struct SubscriberConfig {
    /// 日志级别: "trace", "debug", "info", "warn", "error"
    #[default = "info"]
    pub log_level: String,

    /// 是否包含 fmt layer（输出可读日志到控制台）
    #[default = false]
    pub with_fmt_layer: bool,
}

impl SubscriberConfig {
    /// 获取日志级别对应的 Level
    pub fn level(&self) -> Level {
        Level::from_str(self.log_level.to_lowercase().as_str()).unwrap_or(Level::INFO)
    }
}

/// 保证 init_tracer 只被调用一次
static INIT_ONCE: OnceLock<Result<()>> = OnceLock::new();

/// 初始化全局 tracer provider 和 tracing_subscriber
///
/// 根据配置创建 tracer provider 并设置为全局 provider
/// 同时自动初始化 tracing_subscriber
/// 多次调用此函数只会初始化一次，后续调用会返回第一次初始化的结果
pub fn init_tracer(tracer_config: &GlobalTracingConfig) -> Result<()> {
    INIT_ONCE
        .get_or_init(|| init_tracer_inner(tracer_config))
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{}", e))
        .copied()
}

/// 内部初始化函数，实际初始化 tracer provider 和 subscriber
fn init_tracer_inner(tracer_config: &GlobalTracingConfig) -> Result<()> {
    // 如果未启用，直接返回
    if !tracer_config.enabled {
        return Ok(());
    }

    // 创建 Resource
    let resource = Resource::builder()
        .with_service_name(tracer_config.service_name.clone())
        .build();

    // 创建 sampler
    let sampler = if tracer_config.sample_rate >= 1.0 {
        Sampler::AlwaysOn
    } else if tracer_config.sample_rate <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(tracer_config.sample_rate as f64)
    };

    // 创建 exporter
    match &tracer_config.exporter {
        ExporterConfig::Otlp(otlp_config) => {
            let mut builder = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(otlp_config.endpoint.clone())
                .with_timeout(otlp_config.timeout);

            // 设置 headers
            if let Some(ref headers) = otlp_config.headers {
                use tonic::metadata::{MetadataKey, MetadataMap};
                let mut metadata_map = MetadataMap::new();
                for (key, value) in headers {
                    let metadata_key = MetadataKey::from_bytes(key.as_bytes())
                        .map_err(|e| anyhow::anyhow!("Invalid header key '{}': {}", key, e))?;
                    metadata_map.insert(metadata_key, value.parse().map_err(|e| {
                        anyhow::anyhow!("Invalid header value for '{}': {}", key, e)
                    })?);
                }
                builder = builder.with_metadata(metadata_map);
            }

            let exporter = builder.build()?;

            // 创建 BatchSpanProcessor
            let batch_config = if let Some(ref config) = tracer_config.batch_processor {
                BatchConfigBuilder::default()
                    .with_scheduled_delay(config.scheduled_delay)
                    .with_max_queue_size(config.max_queue_size)
                    .with_max_export_batch_size(config.max_export_batch_size)
                    .with_max_concurrent_exports(config.max_concurrent_exports)
                    .build()
            } else {
                BatchConfigBuilder::default().build()
            };

            let batch_processor = BatchSpanProcessor::builder(exporter)
                .with_batch_config(batch_config)
                .build();

            // 创建 tracer provider
            let provider = SdkTracerProvider::builder()
                .with_span_processor(batch_processor)
                .with_resource(resource)
                .with_sampler(sampler)
                .build();

            // 设置为全局 provider
            global::set_tracer_provider(provider);
        }
        ExporterConfig::Stdout {} => {
            // 创建 stdout exporter 用于调试
            let exporter = opentelemetry_stdout::SpanExporter::default();

            // 创建 tracer provider，使用 with_simple_exporter 方法
            let provider = SdkTracerProvider::builder()
                .with_simple_exporter(exporter)
                .with_resource(resource)
                .with_sampler(sampler)
                .build();

            // 设置为全局 provider
            global::set_tracer_provider(provider);
        }
        ExporterConfig::None {} => {
            // 不导出，不设置 tracer provider
        }
    }

    // 初始化 tracing_subscriber
    init_subscriber(tracer_config);

    Ok(())
}

/// 获取 gRPC tracing layer
///
/// 返回用于 tonic gRPC 服务端的 OpenTelemetry tracing layer
/// 自动与全局 tracer provider 集成
///
/// # 使用示例
///
/// ```rust,ignore
/// use rustx::aop::grpc_tracing_layer;
///
/// let tracing_layer = grpc_tracing_layer();
/// Server::builder()
///     .layer(tracing_layer)
///     .add_service(service)
///     .serve(addr)
///     .await?;
/// ```
pub fn grpc_tracing_layer() -> OtelGrpcLayer {
    OtelGrpcLayer::default()
}

/// 获取 HTTP tracing layer
///
/// 返回用于 tower/axum HTTP 服务端的 tracing layer
/// 自动记录 HTTP 请求和响应的详细信息
///
/// # 使用示例
///
/// ```rust,ignore
/// use rustx::aop::http_tracing_layer;
///
/// let app = Router::new()
///     .route("/", get(handler))
///     .layer(http_tracing_layer());
/// ```
pub fn http_tracing_layer() -> HttpTraceLayer {
    tower_http::trace::TraceLayer::new_for_http()
}

/// 初始化 tracing_subscriber
///
/// 创建 OTEL layer 和 fmt layer 并注册到全局 dispatcher
fn init_subscriber(config: &GlobalTracingConfig) {
    // 创建 OTEL layer，将 tracing events 转换为 OTEL spans
    let service_name = config.service_name.clone();
    let tracer = global::tracer(service_name);
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // 构建 subscriber
    let registry = tracing_subscriber::registry().with(otel_layer);

    // 如果启用 fmt layer，添加可读日志输出
    if config.subscriber.with_fmt_layer {
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_level(true)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .with_filter(LevelFilter::from_level(config.subscriber.level()));

        registry.with(fmt_layer).init();
    } else {
        registry.with(LevelFilter::from_level(config.subscriber.level())).init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracer_config_default() {
        let config = GlobalTracingConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.service_name, "rustx-service");
        assert_eq!(config.sample_rate, 1.0);
        assert!(matches!(config.exporter, ExporterConfig::None {}));
        assert!(config.batch_processor.is_none());
        assert_eq!(config.subscriber.log_level, "info");
        assert!(!config.subscriber.with_fmt_layer);
    }

    #[test]
    fn test_tracer_config_deserialize() {
        let config_json = r#"{
            enabled: true,
            service_name: "my-service",
            sample_rate: 0.5,
            exporter: {
                type: "otlp",
                endpoint: "http://localhost:4317",
                timeout: "30s"
            }
        }"#;

        let config: GlobalTracingConfig = json5::from_str(config_json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.service_name, "my-service");
        assert_eq!(config.sample_rate, 0.5);
        assert!(matches!(config.exporter, ExporterConfig::Otlp(_)));
    }

    #[test]
    fn test_subscriber_config_default() {
        let config = SubscriberConfig::default();
        assert_eq!(config.log_level, "info");
        assert!(!config.with_fmt_layer);
        assert_eq!(config.level(), Level::INFO);
    }

    #[test]
    fn test_subscriber_config_log_levels() {
        let cases = vec![
            ("trace", Level::TRACE),
            ("debug", Level::DEBUG),
            ("info", Level::INFO),
            ("warn", Level::WARN),
            ("error", Level::ERROR),
            ("INFO", Level::INFO),  // 测试大小写不敏感
            ("DEBUG", Level::DEBUG),
        ];

        for (log_level, expected) in cases {
            let config = SubscriberConfig {
                log_level: log_level.to_string(),
                ..Default::default()
            };
            assert_eq!(config.level(), expected, "Failed for log_level: {}", log_level);
        }
    }

    #[test]
    fn test_subscriber_config_invalid_level() {
        let config = SubscriberConfig {
            log_level: "invalid".to_string(),
            ..Default::default()
        };
        // 无效级别应该回退到 INFO
        assert_eq!(config.level(), Level::INFO);
    }

    #[test]
    fn test_tracer_config_with_subscriber() {
        let config_json = r#"{
            enabled: true,
            service_name: "my-service",
            subscriber: {
                log_level: "debug",
                with_fmt_layer: false
            }
        }"#;

        let config: GlobalTracingConfig = json5::from_str(config_json).unwrap();
        assert_eq!(config.subscriber.log_level, "debug");
        assert!(!config.subscriber.with_fmt_layer);
        assert_eq!(config.subscriber.level(), Level::DEBUG);
    }

    #[test]
    fn test_tracer_config_validation() {
        // 有效配置
        let config = GlobalTracingConfig {
            enabled: true,
            service_name: "test".to_string(),
            sample_rate: 0.8,
            ..Default::default()
        };
        assert!(config.validate().is_ok());

        // 采样率超出范围
        let config = GlobalTracingConfig {
            sample_rate: 1.5,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = GlobalTracingConfig {
            sample_rate: -0.1,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // service_name 为空
        let config = GlobalTracingConfig {
            service_name: "".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_otlp_exporter_config_default() {
        let config = OtlpExporterConfig::default();
        assert_eq!(config.endpoint, "http://localhost:4317");
        assert_eq!(config.timeout, Duration::from_secs(10));
        assert!(config.headers.is_none());
    }

    #[test]
    fn test_otlp_exporter_config_with_headers() {
        let config_json = r#"{
            endpoint: "http://otel-collector:4317",
            headers: {
                "Authorization": "Bearer token123"
            },
            timeout: "20s"
        }"#;

        let config: OtlpExporterConfig = json5::from_str(config_json).unwrap();
        assert_eq!(config.endpoint, "http://otel-collector:4317");
        assert!(config.headers.is_some());
        let headers = config.headers.unwrap();
        assert_eq!(headers.get("Authorization"), Some(&"Bearer token123".to_string()));
        assert_eq!(config.timeout, Duration::from_secs(20));
    }

    #[test]
    fn test_batch_processor_config_default() {
        let config = BatchProcessorConfig::default();
        assert_eq!(config.scheduled_delay, Duration::from_secs(5));
        assert_eq!(config.max_queue_size, 2048);
        assert_eq!(config.max_export_batch_size, 512);
        assert_eq!(config.max_concurrent_exports, 1);
    }

    #[test]
    fn test_batch_processor_config_deserialize() {
        let config_json = r#"{
            scheduled_delay: "10s",
            max_queue_size: 4096,
            max_export_batch_size: 1024,
            max_concurrent_exports: 2
        }"#;

        let config: BatchProcessorConfig = json5::from_str(config_json).unwrap();
        assert_eq!(config.scheduled_delay, Duration::from_secs(10));
        assert_eq!(config.max_queue_size, 4096);
        assert_eq!(config.max_export_batch_size, 1024);
        assert_eq!(config.max_concurrent_exports, 2);
    }

    #[test]
    fn test_exporter_config_default() {
        let config = ExporterConfig::default();
        assert!(matches!(config, ExporterConfig::None {}));
    }

    #[test]
    fn test_exporter_config_is_none() {
        let config = ExporterConfig::None {};
        assert!(config.is_none());

        let config = ExporterConfig::Stdout {};
        assert!(!config.is_none());

        let otlp_config = OtlpExporterConfig::default();
        let config = ExporterConfig::Otlp(otlp_config);
        assert!(!config.is_none());
    }

    #[test]
    fn test_exporter_config_deserialize_stdout() {
        // 测试 stdout 配置
        let config: ExporterConfig = json5::from_str(r#"{ type: "stdout" }"#).unwrap();
        assert!(matches!(config, ExporterConfig::Stdout {}));
    }

    #[test]
    fn test_exporter_config_deserialize_none() {
        // 测试 none 配置
        let config: ExporterConfig = json5::from_str(r#"{ type: "none" }"#).unwrap();
        assert!(matches!(config, ExporterConfig::None {}));
    }

    #[test]
    fn test_exporter_config_deserialize_otlp() {
        // 测试 otlp 配置（保持向后兼容）
        let config: ExporterConfig =
            json5::from_str(r#"{ type: "otlp", endpoint: "http://localhost:4317" }"#).unwrap();
        assert!(matches!(config, ExporterConfig::Otlp(_)));
        if let ExporterConfig::Otlp(otlp) = config {
            assert_eq!(otlp.endpoint, "http://localhost:4317");
        }
    }
}

#[cfg(test)]
mod tracing_tests {
    use super::*;

    /// 测试辅助函数：创建一个简单的 tracing subscriber
    fn setup_test_subscriber() {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_test_writer()
                .with_max_level(tracing::Level::TRACE)
                .finish(),
        );
    }

    #[test]
    fn test_sync_function_with_instrument() {
        setup_test_subscriber();

        // 测试同步函数的 instrument 宏
        #[tracing::instrument]
        fn add_numbers(x: i32, y: i32) -> i32 {
            tracing::info!("calculating sum");
            x + y
        }

        let result = add_numbers(10, 20);
        assert_eq!(result, 30);
    }

    #[test]
    fn test_nested_sync_functions() {
        setup_test_subscriber();

        #[tracing::instrument]
        fn level_three() -> i32 {
            tracing::info!("at level three");
            3
        }

        #[tracing::instrument]
        fn level_two() -> i32 {
            tracing::info!("at level two");
            level_three()
        }

        #[tracing::instrument]
        fn level_one() -> i32 {
            tracing::info!("at level one");
            level_two()
        }

        let result = level_one();
        assert_eq!(result, 3);
    }

    #[test]
    fn test_sync_function_with_fields() {
        setup_test_subscriber();

        #[tracing::instrument(fields(user_id = 123, action = "test"))]
        fn process_request() {
            tracing::info!("processing request");
        }

        process_request();
    }

    #[tokio::test]
    async fn test_async_function_with_instrument() {
        setup_test_subscriber();

        #[tracing::instrument]
        async fn async_add(x: i32, y: i32) -> i32 {
            tracing::info!("async calculating");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            x + y
        }

        let result = async_add(5, 15).await;
        assert_eq!(result, 20);
    }

    #[tokio::test]
    async fn test_nested_async_functions() {
        setup_test_subscriber();

        #[tracing::instrument]
        async fn async_child(value: i32) -> i32 {
            tracing::info!("child processing");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            value * 2
        }

        #[tracing::instrument]
        async fn async_parent(x: i32) -> i32 {
            tracing::info!("parent processing");
            async_child(x).await
        }

        let result = async_parent(10).await;
        assert_eq!(result, 20);
    }

    #[tokio::test]
    async fn test_async_concurrent_tasks() {
        setup_test_subscriber();

        #[tracing::instrument]
        async fn task(id: i32) -> i32 {
            tracing::info!("task {} starting", id);
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            tracing::info!("task {} completing", id);
            id
        }

        let results = tokio::join!(task(1), task(2), task(3));
        assert_eq!(results, (1, 2, 3));
    }

    #[test]
    fn test_manual_span_creation() {
        setup_test_subscriber();

        fn manual_function() {
            let span = tracing::span!(tracing::Level::INFO, "manual_span", param1 = 42);
            let _enter = span.enter();

            tracing::info!("inside manual span");

            // span 在 _enter drop 时自动退出
        }

        manual_function();
    }

    #[test]
    fn test_span_with_events() {
        setup_test_subscriber();

        #[tracing::instrument]
        fn function_with_events() {
            tracing::event!(tracing::Level::WARN, message = "warning event");
            tracing::warn!("this is a warning");
            tracing::info!("this is info");
        }

        function_with_events();
    }

    #[test]
    fn test_error_handling() {
        setup_test_subscriber();

        #[derive(Debug)]
        struct CustomError;

        impl std::fmt::Display for CustomError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "Custom error occurred")
            }
        }

        impl std::error::Error for CustomError {}

        #[tracing::instrument(err)]
        fn function_with_error() -> Result<(), CustomError> {
            tracing::info!("attempting operation");
            Err(CustomError)
        }

        let result = function_with_error();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_async_error_handling() {
        setup_test_subscriber();

        #[derive(Debug)]
        struct AsyncError;

        impl std::fmt::Display for AsyncError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "Async error")
            }
        }

        impl std::error::Error for AsyncError {}

        #[tracing::instrument(err)]
        async fn async_failing_function() -> Result<i32, AsyncError> {
            tracing::info!("about to fail");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Err(AsyncError)
        }

        let result = async_failing_function().await;
        assert!(result.is_err());
    }

    #[test]
    fn test_span_skip_parameters() {
        setup_test_subscriber();

        #[derive(Debug)]
        struct SensitiveData(#[allow(dead_code)] String);

        #[tracing::instrument(skip(_data))]
        fn process_sensitive(_data: SensitiveData, value: i32) -> i32 {
            tracing::info!("processing");
            value
        }

        let data = SensitiveData("secret".to_string());
        let result = process_sensitive(data, 42);
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_mixed_sync_async_calls() {
        setup_test_subscriber();

        #[tracing::instrument]
        fn sync_part(x: i32) -> i32 {
            tracing::info!("sync computation");
            x * 2
        }

        #[tracing::instrument]
        async fn async_part(x: i32) -> i32 {
            tracing::info!("async computation");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            sync_part(x) + 1
        }

        #[tracing::instrument]
        async fn coordinator(x: i32) -> i32 {
            tracing::info!("coordinating");
            let sync_result = sync_part(x);
            let async_result = async_part(sync_result).await;
            async_result
        }

        let result = coordinator(5).await;
        assert_eq!(result, 21); // 5 * 2 = 10, 10 * 2 = 20, 20 + 1 = 21
    }

    // 测试：手动 span 包装未加 instrument 的 async 函数
    #[tokio::test]
    async fn test_manual_span_for_async_function() {
        setup_test_subscriber();

        // 没有 #[instrument] 的 async 函数
        async fn external_async_task(x: i32) -> i32 {
            tracing::info!("inside external task");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            x * 2
        }

        // 方法1：使用 span! 和 enter() 手动包装
        async fn call_with_manual_span(x: i32) -> i32 {
            let span = tracing::span!(tracing::Level::INFO, "manual_span_wrapper", task_id = x);
            let _enter = span.enter();

            tracing::info!("before calling external task");
            let result = external_async_task(x).await;
            tracing::info!("after calling external task");

            result
        }

        let result = call_with_manual_span(42).await;
        assert_eq!(result, 84);
    }

    // 测试：使用 in_future 包装未加 instrument 的 async 函数（更安全的方式）
    #[tokio::test]
    async fn test_span_in_future_for_async_function() {
        setup_test_subscriber();

        // 没有 #[instrument] 的 async 函数
        async fn uninstrumented_async(value: i32) -> i32 {
            tracing::info!("uninstrumented async function");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            value + 10
        }

        // 使用 in_future 创建跨 await 的 span
        async fn call_with_span_in_future(value: i32) -> i32 {
            use tracing::Instrument;

            let span = tracing::info_span!("span_in_future", input = value);
            uninstrumented_async(value).instrument(span).await
        }

        let result = call_with_span_in_future(5).await;
        assert_eq!(result, 15);
    }

    // 测试：多层嵌套的未加 instrument 的 async 函数
    #[tokio::test]
    async fn test_nested_uninstrumented_async_calls() {
        setup_test_subscriber();

        async fn level_three_uninstrumented() -> i32 {
            tracing::info!("level three uninstrumented");
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            3
        }

        async fn level_two_uninstrumented() -> i32 {
            tracing::info!("level two uninstrumented");
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            level_three_uninstrumented().await
        }

        #[tracing::instrument]
        async fn level_one_instrumented() -> i32 {
            tracing::info!("level one instrumented");

            // 手动包装未加 instrument 的调用
            use tracing::Instrument;
            let span = tracing::info_span!("manual_level_two");
            level_two_uninstrumented().instrument(span).await
        }

        let result = level_one_instrumented().await;
        assert_eq!(result, 3);
    }

    // 测试：为多个未加 instrument 的并发任务分别创建 span
    #[tokio::test]
    async fn test_multiple_uninstrumented_async_tasks() {
        setup_test_subscriber();

        async fn uninstrumented_worker(id: i32, duration: u64) -> i32 {
            tracing::info!("worker {} starting", id);
            tokio::time::sleep(std::time::Duration::from_millis(duration)).await;
            tracing::info!("worker {} completing", id);
            id
        }

        async fn run_workers() -> (i32, i32, i32) {
            use tracing::Instrument;

            // 为每个任务创建独立的 span
            let task1 = uninstrumented_worker(1, 10)
                .instrument(tracing::info_span!("worker_1"));
            let task2 = uninstrumented_worker(2, 15)
                .instrument(tracing::info_span!("worker_2"));
            let task3 = uninstrumented_worker(3, 20)
                .instrument(tracing::info_span!("worker_3"));

            tokio::join!(task1, task2, task3)
        }

        #[tracing::instrument]
        async fn coordinator() -> (i32, i32, i32) {
            tracing::info!("coordinating workers");
            run_workers().await
        }

        let results = coordinator().await;
        assert_eq!(results, (1, 2, 3));
    }

    // 测试：在 instrumented 函数中混合调用 instrumented 和 uninstrumented 函数
    #[tokio::test]
    async fn test_mixed_instrumented_uninstrumented_calls() {
        setup_test_subscriber();

        // uninstrumented async 函数
        async fn external_api_call() -> i32 {
            tracing::info!("external API call");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            100
        }

        // instrumented async 函数
        #[tracing::instrument]
        async fn internal_calculation(x: i32) -> i32 {
            tracing::info!("internal calculation");
            x * 2
        }

        // 混合调用两者
        #[tracing::instrument]
        async fn business_logic(value: i32) -> i32 {
            tracing::info!("starting business logic");

            // 调用 instrumented 函数（自动追踪）
            let calc_result = internal_calculation(value).await;

            // 调用 uninstrumented 函数（手动包装）
            use tracing::Instrument;
            let api_span = tracing::info_span!("external_api_wrapper");
            let api_result = external_api_call().instrument(api_span).await;

            calc_result + api_result
        }

        let result = business_logic(50).await;
        assert_eq!(result, 200); // 50 * 2 + 100 = 200
    }

    // 测试：使用 span scope 包装同步的非 instrumented 函数
    #[test]
    fn test_sync_uninstrumented_function_with_span() {
        setup_test_subscriber();

        // uninstrumented 同步函数
        fn sync_uninstrumented(x: i32) -> i32 {
            tracing::info!("sync uninstrumented");
            x + 1
        }

        #[tracing::instrument]
        fn call_uninstrumented_sync(x: i32) -> i32 {
            tracing::info!("before uninstrumented call");

            // 方法1：直接调用（会继承当前 span）
            let result1 = sync_uninstrumented(x);

            // 方法2：手动创建新的 child span
            let span = tracing::info_span!("child_uninstrumented", input = x);
            let _guard = span.enter();
            let result2 = sync_uninstrumented(result1);

            tracing::info!("after uninstrumented call");
            result2
        }

        let result = call_uninstrumented_sync(10);
        assert_eq!(result, 12); // 10 + 1 + 1 = 12
    }
}
