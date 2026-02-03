use crate::cfg::serde_duration::{serde_as, HumanDur};
use crate::log::Logger;
use anyhow::Result;
use backon::{BackoffBuilder, ConstantBuilder, ExponentialBuilder, FibonacciBuilder};
use garde::Validate;
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{
        counter::Counter,
        family::Family,
        gauge::Gauge,
        histogram::{exponential_buckets, Histogram},
    },
};
use serde::Deserialize;
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use crate::log::LoggerConfig;

/// Metric 标签（包含 operation 和 status，以及固定的可选标签）
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct MetricLabels {
    pub operation: String,
    pub status: Option<String>,
    pub service: Option<String>,
    pub env: Option<String>,
    pub version: Option<String>,
    pub cluster: Option<String>,
    pub host_ip: Option<String>,
}

/// 仅包含 operation 的标签（用于 retry_count 和 duration）
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct OperationLabel {
    pub operation: String,
    pub service: Option<String>,
    pub env: Option<String>,
    pub version: Option<String>,
    pub cluster: Option<String>,
    pub host_ip: Option<String>,
}

/// 获取本地私网 IP 地址
///
/// 优先返回第一个 IPv4 私网地址（10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16）
/// 如果没有找到私网地址，则返回第一个非回环的 IPv4 地址
/// 如果都没有，返回 None
fn get_local_private_ip() -> Option<String> {
    use local_ip_address::local_ip;

    // 尝试获取本地 IP
    if let Ok(ip) = local_ip() {
        // 检查是否是 IPv4 地址
        if ip.is_ipv4() {
            let octets = match ip {
                IpAddr::V4(v4) => v4.octets(),
                IpAddr::V6(_) => return None,
            };

            // 10.0.0.0/8
            if octets[0] == 10 {
                return Some(ip.to_string());
            }
            // 172.16.0.0/12
            if octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31 {
                return Some(ip.to_string());
            }
            // 192.168.0.0/16
            if octets[0] == 192 && octets[1] == 168 {
                return Some(ip.to_string());
            }

            // 如果不是私网地址，检查是否是回环地址
            if !ip.is_loopback() {
                return Some(ip.to_string());
            }
        }
    }

    None
}

/// 从 HashMap 提取固定的标签字段
///
/// 优先级：
/// - service: 配置值 > 环境变量 > 二进制文件名 > None
/// - version: 配置值 > 环境变量 > 编译时 git tag > None
/// - env/cluster: 配置值 > 环境变量 > None
/// - host_ip: 配置值 > 环境变量 > 自动获取（本地私网 IP）> None
///
/// 环境变量命名规范：AOP_<字段名大写>
pub fn extract_fixed_labels(
    labels: &HashMap<String, String>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let service = labels
        .get("service")
        .cloned()
        .or_else(|| std::env::var("AOP_SERVICE").ok())
        .or_else(|| {
            // 自动获取二进制文件名作为 service
            std::env::current_exe().ok().and_then(|path| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
        });
    let env = labels
        .get("env")
        .cloned()
        .or_else(|| std::env::var("AOP_ENV").ok());
    let version = labels
        .get("version")
        .cloned()
        .or_else(|| std::env::var("AOP_VERSION").ok())
        .or_else(|| {
            // 使用编译时嵌入的 git tag
            option_env!("GIT_VERSION").map(|s| s.to_string())
        });
    let cluster = labels
        .get("cluster")
        .cloned()
        .or_else(|| std::env::var("AOP_CLUSTER").ok());
    let host_ip = labels
        .get("host_ip")
        .cloned()
        .or_else(|| std::env::var("AOP_HOST_IP").ok())
        .or_else(get_local_private_ip);
    (service, env, version, cluster, host_ip)
}

/// AOP 创建配置（用于创建新的 AOP 实例）
#[derive(Debug, Clone, Deserialize, SmartDefault, PartialEq)]
#[serde(default)]
pub struct AopCreateConfig {
    /// Retry 配置
    pub retry: Option<RetryConfig>,

    /// Logging 配置
    pub logging: Option<LoggingConfig>,

    /// Tracing 配置
    pub tracing: Option<TracingConfig>,

    /// Metrics 配置
    pub metrics: Option<MetricsConfig>,
}

/// Retry 配置
#[serde_as]
#[derive(Debug, Clone, Deserialize, SmartDefault, Validate, PartialEq)]
#[serde(default)]
pub struct RetryConfig {
    /// 最大重试次数
    #[default = 3]
    #[garde(range(min = 1, max = 100))]
    pub max_times: usize,

    /// 退避策略: "constant" / "exponential" / "fibonacci"
    #[default = "constant"]
    #[garde(pattern("constant|exponential|fibonacci"))]
    pub strategy: String,

    /// 延迟（用于 constant 策略）
    #[serde_as(as = "HumanDur")]
    #[default(Duration::from_secs(1))]
    #[garde(skip)]
    pub delay: Duration,

    /// 最小延迟（用于 exponential/fibonacci 策略）
    #[serde_as(as = "HumanDur")]
    #[default(Duration::from_secs(1))]
    #[garde(skip)]
    pub min_delay: Duration,

    /// 最大延迟
    #[serde_as(as = "HumanDur")]
    #[default(Duration::from_secs(60))]
    #[garde(skip)]
    pub max_delay: Duration,

    /// 退避因子（用于 exponential 策略）
    #[default = 2.0]
    #[garde(skip)]
    pub factor: f32,

    /// 抖动（jitter）：是否在延迟基础上添加随机抖动
    #[default = false]
    #[garde(skip)]
    pub jitter: bool,
}

/// Logging 配置
#[derive(Debug, Clone, Deserialize, SmartDefault, Validate, PartialEq)]
#[serde(default)]
pub struct LoggingConfig {
    /// Logger 配置
    #[garde(skip)]
    pub logger: LoggerConfig,

    /// 成功日志的采样率（0.0 - 1.0），默认 1.0（总是记录）
    #[default = 1.0]
    #[garde(range(min = 0.0, max = 1.0))]
    pub info_sample_rate: f32,

    /// 失败日志的采样率（0.0 - 1.0），默认 1.0（总是记录）
    #[default = 1.0]
    #[garde(range(min = 0.0, max = 1.0))]
    pub warn_sample_rate: f32,
}

/// Tracing 配置
#[derive(Debug, Clone, Deserialize, SmartDefault, Validate, PartialEq)]
#[serde(default)]
pub struct TracingConfig {
    /// Span 名称字段
    #[default = "aop"]
    #[garde(length(min = 1))]
    pub name: String,

    /// 是否记录参数
    #[default = false]
    #[garde(skip)]
    pub with_args: bool,
}

/// Metrics 配置（运行时，用于记录指标）
#[derive(Debug, Clone, Deserialize, SmartDefault, Validate, PartialEq)]
#[serde(default)]
pub struct MetricsConfig {
    /// Metric 名称前缀
    #[default = "aop"]
    #[garde(length(min = 1))]
    pub prefix: String,

    /// 常量 Labels（会应用到所有 metric）
    ///
    /// 支持的常量标签：
    /// - `service`: 服务名称（优先级: 配置值 > 环境变量 AOP_SERVICE > 二进制文件名）
    /// - `env`: 部署环境（如 dev/test/prod）（优先级: 配置值 > 环境变量 AOP_ENV）
    /// - `version`: 服务版本（优先级: 配置值 > 环境变量 AOP_VERSION > 编译时 git tag）
    /// - `cluster`: 集群名称（优先级: 配置值 > 环境变量 AOP_CLUSTER）
    /// - `host_ip`: 主机 IP 地址（优先级: 配置值 > 环境变量 AOP_HOST_IP > 自动获取本地私网 IP）
    #[garde(skip)]
    pub labels: Option<HashMap<String, String>>,
}

/// AOP 配置
///
/// 支持两种模式：
/// - Reference: 引用已存在的 aop 实例（通过 $instance 字段）
/// - Create: 创建新的 aop 实例
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AopConfig {
    /// 引用一个已存在的 aop 实例
    Reference {
        /// 引用的 aop 实例名称
        #[serde(rename = "$instance")]
        instance: String,
    },

    /// 创建新的 aop 实例
    Create(AopCreateConfig),
}

impl Default for AopConfig {
    fn default() -> Self {
        AopConfig::Create(AopCreateConfig::default())
    }
}

/// AOP 切面
pub struct Aop {
    /// Logger（如果启用 logging）
    pub logger: Option<Arc<Logger>>,

    /// Retry 配置（如果启用 retry）
    pub retry_config: Option<RetryConfig>,

    /// Tracing 配置（如果启用 tracing）
    pub tracing_config: Option<TracingConfig>,

    /// Metrics 配置（如果启用 metrics）
    pub metrics_config: Option<MetricsConfig>,

    /// Metric: 总调用次数（按 operation + status 分组）
    pub metric_total: Option<Family<MetricLabels, Counter<u64>>>,

    /// Metric: 重试次数（按 operation 分组）
    pub metric_retry_count: Option<Family<OperationLabel, Counter<u64>>>,

    /// Metric: 调用耗时分布（按 operation 分组）
    pub metric_duration: Option<Family<OperationLabel, Histogram, fn() -> Histogram>>,

    /// Metric: 当前正在执行的请求数（按 operation 分组）
    pub metric_in_progress: Option<Family<OperationLabel, Gauge<i64>>>,

    /// Metric: 预创建的默认 OperationLabel（operation 为空，使用时替换）
    pub metric_default_operation_label: Option<OperationLabel>,

    /// Metric: 预创建的默认 MetricLabels（operation 和 status 为空，使用时替换）
    pub metric_default_metric_labels: Option<MetricLabels>,

    /// 成功日志的采样率
    pub info_sample_rate: f32,

    /// 失败日志的采样率
    pub warn_sample_rate: f32,
}

impl Aop {
    /// 从创建配置创建 Aop
    pub fn new(config: AopCreateConfig) -> Result<Self> {
        // 解析 logger
        let (logger, info_sample_rate, warn_sample_rate) =
            if let Some(logging_config) = config.logging {
                // 验证 logging 配置
                garde::Validate::validate(&logging_config)?;
                (
                    Some(Logger::resolve(logging_config.logger)?),
                    logging_config.info_sample_rate,
                    logging_config.warn_sample_rate,
                )
            } else {
                (None, 1.0, 1.0)
            };

        // 验证 retry 配置
        if let Some(ref retry_config) = config.retry {
            garde::Validate::validate(retry_config)?;
        }

        // 验证 tracing 配置
        if let Some(ref tracing_config) = config.tracing {
            garde::Validate::validate(tracing_config)?;
        }

        // 处理 metrics 配置
        let (
            metrics_config,
            metric_total,
            metric_retry_count,
            metric_duration,
            metric_in_progress,
            metric_default_operation_label,
            metric_default_metric_labels,
        ) = if let Some(metric_cfg) = config.metrics {
            // 验证 metric 配置
            garde::Validate::validate(&metric_cfg)?;

            // 注册 metric 到全局 Registry
            let registry = crate::aop::global_registry();
            let mut registry = registry.write().unwrap();
            let prefix = &metric_cfg.prefix;

            // 注册 total counter
            let total = Family::default();
            registry.register(
                format!("{}_total", prefix),
                format!("Total number of {} calls", prefix),
                total.clone(),
            );

            // 注册 retry_count counter
            let retry_count = Family::default();
            registry.register(
                format!("{}_retry_count", prefix),
                format!("Total number of {} retries", prefix),
                retry_count.clone(),
            );

            // 注册 duration histogram
            fn new_histogram() -> Histogram {
                Histogram::new(exponential_buckets(1.0, 2.0, 12))
            }
            let duration = Family::new_with_constructor(new_histogram as fn() -> Histogram);
            registry.register(
                format!("{}_duration_ms", prefix),
                format!("Duration of {} calls in milliseconds", prefix),
                duration.clone(),
            );

            // 注册 in_progress gauge
            let in_progress = Family::default();
            registry.register(
                format!("{}_in_progress", prefix),
                format!("Number of {} calls currently in progress", prefix),
                in_progress.clone(),
            );

            // 预创建默认标签（operation 和 status 为空，使用时替换）
            use std::collections::HashMap;
            let empty_labels = HashMap::new();
            let labels_map = metric_cfg.labels.as_ref().unwrap_or(&empty_labels);
            let (service, env, version, cluster, host_ip) = extract_fixed_labels(labels_map);
            let default_operation_label = Some(OperationLabel {
                operation: String::new(),
                service: service.clone(),
                env: env.clone(),
                version: version.clone(),
                cluster: cluster.clone(),
                host_ip: host_ip.clone(),
            });
            let default_metric_labels = Some(MetricLabels {
                operation: String::new(),
                status: None,
                service,
                env,
                version,
                cluster,
                host_ip,
            });

            (
                Some(metric_cfg),
                Some(total),
                Some(retry_count),
                Some(duration),
                Some(in_progress),
                default_operation_label,
                default_metric_labels,
            )
        } else {
            (None, None, None, None, None, None, None)
        };

        Ok(Self {
            logger,
            retry_config: config.retry,
            tracing_config: config.tracing,
            metrics_config,
            metric_total,
            metric_retry_count,
            metric_duration,
            metric_in_progress,
            metric_default_operation_label,
            metric_default_metric_labels,
            info_sample_rate,
            warn_sample_rate,
        })
    }

    /// 从配置解析 Aop
    ///
    /// 如果配置是 Reference 模式，从全局管理器获取已存在的 aop
    /// 如果配置是 Create 模式，创建新的 aop
    pub fn resolve(config: AopConfig) -> Result<Arc<Self>> {
        match config {
            AopConfig::Reference { instance } => {
                // 从全局管理器获取已存在的 aop
                crate::aop::get(&instance).ok_or_else(|| {
                    anyhow::anyhow!("Aop instance '{}' not found in global manager", instance)
                })
            }

            AopConfig::Create(create_config) => {
                // 创建新的 aop
                Ok(Arc::new(Aop::new(create_config)?))
            }
        }
    }

    /// 构建 backon 的 Backoff 策略
    pub fn build_backoff(&self) -> Option<Box<dyn Iterator<Item = Duration> + Send + Sync>> {
        let retry_config = self.retry_config.as_ref()?;

        let backoff: Box<dyn Iterator<Item = Duration> + Send + Sync> =
            match retry_config.strategy.as_str() {
                "constant" => {
                    let mut builder = ConstantBuilder::default().with_delay(retry_config.delay);

                    builder = builder.with_max_times(retry_config.max_times);

                    if retry_config.jitter {
                        builder = builder.with_jitter();
                    }

                    Box::new(builder.build())
                }

                "exponential" => {
                    let mut builder =
                        ExponentialBuilder::default().with_min_delay(retry_config.min_delay);

                    builder = builder.with_max_delay(retry_config.max_delay);
                    builder = builder.with_factor(retry_config.factor);
                    builder = builder.with_max_times(retry_config.max_times);

                    if retry_config.jitter {
                        builder = builder.with_jitter();
                    }

                    Box::new(builder.build())
                }

                "fibonacci" => {
                    let mut builder =
                        FibonacciBuilder::default().with_min_delay(retry_config.min_delay);

                    builder = builder.with_max_delay(retry_config.max_delay);
                    builder = builder.with_max_times(retry_config.max_times);

                    if retry_config.jitter {
                        builder = builder.with_jitter();
                    }

                    Box::new(builder.build())
                }

                _ => {
                    // 默认使用 constant 策略
                    Box::new(
                        ConstantBuilder::default()
                            .with_delay(retry_config.delay)
                            .with_max_times(retry_config.max_times)
                            .build(),
                    )
                }
            };

        Some(backoff)
    }
}

impl From<AopCreateConfig> for Aop {
    fn from(config: AopCreateConfig) -> Self {
        Aop::new(config).expect("Failed to create Aop")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::serde_duration::HumanDur;
    use serde::Serialize;
    use serial_test::serial;

    #[test]
    fn test_retry_config_deserialize() {
        // 测试基本反序列化
        let config: RetryConfig = json5::from_str(
            r#"{
                "max_times": 5,
                "strategy": "constant",
                "delay": "100ms"
            }"#,
        )
        .unwrap();
        assert_eq!(config.max_times, 5);
        assert_eq!(config.strategy, "constant");
        assert_eq!(config.delay, Duration::from_millis(100));

        // 测试 exponential 策略
        let config: RetryConfig = serde_json::from_str(
            r#"{
                "max_times": 3,
                "strategy": "exponential",
                "min_delay": "50ms",
                "max_delay": "10s",
                "factor": 2.5
            }"#,
        )
        .unwrap();
        assert_eq!(config.strategy, "exponential");
        assert_eq!(config.min_delay, Duration::from_millis(50));
        assert_eq!(config.max_delay, Duration::from_secs(10));
        assert_eq!(config.factor, 2.5);

        // 测试 fibonacci 策略
        let config: RetryConfig = serde_json::from_str(
            r#"{
                "strategy": "fibonacci",
                "min_delay": "1s",
                "max_delay": "1m",
                "jitter": true
            }"#,
        )
        .unwrap();
        assert_eq!(config.strategy, "fibonacci");
        assert_eq!(config.min_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(60));
        assert!(config.jitter);
    }

    #[test]
    fn test_retry_config_default() {
        let config: RetryConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.max_times, 3);
        assert_eq!(config.strategy, "constant");
        assert_eq!(config.delay, Duration::from_secs(1));
        assert_eq!(config.min_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(60));
        assert_eq!(config.factor, 2.0);
        assert!(!config.jitter);
    }

    #[test]
    fn test_retry_config_validation() {
        // max_times 超出范围
        let config = RetryConfig {
            max_times: 0,
            ..Default::default()
        };
        assert!(garde::Validate::validate(&config).is_err());

        let config = RetryConfig {
            max_times: 101,
            ..Default::default()
        };
        assert!(garde::Validate::validate(&config).is_err());

        // 无效的 strategy
        let config = RetryConfig {
            strategy: "invalid".to_string(),
            ..Default::default()
        };
        assert!(garde::Validate::validate(&config).is_err());

        // 有效配置
        let config = RetryConfig {
            max_times: 5,
            strategy: "exponential".to_string(),
            ..Default::default()
        };
        assert!(garde::Validate::validate(&config).is_ok());
    }

    #[test]
    fn test_logging_config_validation() {
        // 有效的采样率
        let config = LoggingConfig {
            info_sample_rate: 0.5,
            warn_sample_rate: 1.0,
            ..Default::default()
        };
        assert!(garde::Validate::validate(&config).is_ok());

        // 无效的采样率（超出范围）
        let config = LoggingConfig {
            info_sample_rate: 1.5,
            ..Default::default()
        };
        assert!(garde::Validate::validate(&config).is_err());

        let config = LoggingConfig {
            warn_sample_rate: -0.1,
            ..Default::default()
        };
        assert!(garde::Validate::validate(&config).is_err());
    }

    #[test]
    fn test_tracing_config_default() {
        let config = TracingConfig::default();
        assert_eq!(config.name, "aop");
        assert!(!config.with_args);
    }

    #[test]
    fn test_tracing_config_deserialize() {
        let config: TracingConfig = serde_json::from_str(
            r#"{
                "name": "oss.client",
                "with_args": true
            }"#,
        )
        .unwrap();
        assert_eq!(config.name, "oss.client");
        assert!(config.with_args);
    }

    #[test]
    fn test_tracing_config_validation() {
        // 有效配置
        let config = TracingConfig {
            name: "test".to_string(),
            with_args: true,
        };
        assert!(garde::Validate::validate(&config).is_ok());

        // 无效配置：name 为空
        let config = TracingConfig {
            name: "".to_string(),
            with_args: false,
        };
        assert!(garde::Validate::validate(&config).is_err());
    }

    #[test]
    fn test_aop_config_with_tracing() {
        let config: AopCreateConfig = serde_json::from_str(
            r#"{
                "tracing": {
                    "name": "my.service",
                    "with_args": true
                }
            }"#,
        )
        .unwrap();
        assert!(config.tracing.is_some());
        let tracing = config.tracing.unwrap();
        assert_eq!(tracing.name, "my.service");
        assert!(tracing.with_args);
    }

    #[test]
    fn test_aop_new_with_tracing() {
        let config: AopCreateConfig = serde_json::from_str(
            r#"{
                "tracing": {
                    "name": "test.aop",
                    "with_args": false
                }
            }"#,
        )
        .unwrap();
        let aop = Aop::new(config).unwrap();
        assert!(aop.tracing_config.is_some());
        let tracing_config = aop.tracing_config.unwrap();
        assert_eq!(tracing_config.name, "test.aop");
        assert!(!tracing_config.with_args);
    }

    #[test]
    fn test_aop_config_deserialize() {
        let config: AopCreateConfig = serde_json::from_str(
            r#"{
                "retry": {
                    "max_times": 5,
                    "strategy": "constant",
                    "delay": "200ms"
                }
            }"#,
        )
        .unwrap();
        assert!(config.logging.is_none());
        assert!(config.retry.is_some());

        let retry = config.retry.unwrap();
        assert_eq!(retry.max_times, 5);
        assert_eq!(retry.delay, Duration::from_millis(200));
    }

    #[test]
    fn test_aop_new_without_config() {
        let config = AopCreateConfig::default();
        let aop = Aop::new(config).unwrap();
        assert!(aop.logger.is_none());
        assert!(aop.retry_config.is_none());
        assert_eq!(aop.info_sample_rate, 1.0);
        assert_eq!(aop.warn_sample_rate, 1.0);
    }

    #[test]
    fn test_aop_new_with_retry() {
        let config: AopCreateConfig = serde_json::from_str(
            r#"{
                "retry": {
                    "max_times": 3,
                    "strategy": "exponential",
                    "min_delay": "100ms",
                    "max_delay": "5s"
                }
            }"#,
        )
        .unwrap();
        let aop = Aop::new(config).unwrap();
        assert!(aop.retry_config.is_some());
    }

    #[test]
    fn test_aop_from_config() {
        let config = AopCreateConfig::default();
        let aop: Aop = config.into();
        assert!(aop.logger.is_none());
    }

    #[test]
    fn test_build_backoff_none() {
        let aop = Aop::new(AopCreateConfig::default()).unwrap();
        assert!(aop.build_backoff().is_none());
    }

    #[test]
    fn test_build_backoff_constant() {
        let config: AopCreateConfig = serde_json::from_str(
            r#"{
                "retry": {
                    "max_times": 3,
                    "strategy": "constant",
                    "delay": "100ms"
                }
            }"#,
        )
        .unwrap();
        let aop = Aop::new(config).unwrap();
        let mut backoff = aop.build_backoff().unwrap();

        // constant 策略每次延迟相同
        assert_eq!(backoff.next(), Some(Duration::from_millis(100)));
        assert_eq!(backoff.next(), Some(Duration::from_millis(100)));
        assert_eq!(backoff.next(), Some(Duration::from_millis(100)));
        assert_eq!(backoff.next(), None); // max_times = 3
    }

    #[test]
    fn test_build_backoff_exponential() {
        let config: AopCreateConfig = serde_json::from_str(
            r#"{
                "retry": {
                    "max_times": 4,
                    "strategy": "exponential",
                    "min_delay": "100ms",
                    "factor": 2.0
                }
            }"#,
        )
        .unwrap();
        let aop = Aop::new(config).unwrap();
        let mut backoff = aop.build_backoff().unwrap();

        // exponential 策略延迟指数增长
        let d1 = backoff.next().unwrap();
        let d2 = backoff.next().unwrap();
        let d3 = backoff.next().unwrap();
        let d4 = backoff.next().unwrap();

        assert_eq!(d1, Duration::from_millis(100));
        assert!(d2 >= d1);
        assert!(d3 >= d2);
        assert!(d4 >= d3);
        assert_eq!(backoff.next(), None); // max_times = 4
    }

    #[test]
    fn test_build_backoff_fibonacci() {
        let config: AopCreateConfig = serde_json::from_str(
            r#"{
                "retry": {
                    "max_times": 5,
                    "strategy": "fibonacci",
                    "min_delay": "100ms"
                }
            }"#,
        )
        .unwrap();
        let aop = Aop::new(config).unwrap();
        let mut backoff = aop.build_backoff().unwrap();

        // fibonacci 策略延迟按斐波那契数列增长
        let delays: Vec<_> = backoff.by_ref().take(5).collect();
        assert_eq!(delays.len(), 5);
        assert_eq!(delays[0], Duration::from_millis(100));
        // 验证递增趋势
        for i in 1..delays.len() {
            assert!(delays[i] >= delays[i - 1]);
        }
        assert_eq!(backoff.next(), None);
    }

    #[test]
    fn test_build_backoff_with_max_delay() {
        let config: AopCreateConfig = serde_json::from_str(
            r#"{
                "retry": {
                    "max_times": 10,
                    "strategy": "exponential",
                    "min_delay": "100ms",
                    "max_delay": "500ms",
                    "factor": 2.0
                }
            }"#,
        )
        .unwrap();
        let aop = Aop::new(config).unwrap();
        let backoff = aop.build_backoff().unwrap();

        // 验证所有延迟不超过 max_delay
        for delay in backoff {
            assert!(delay <= Duration::from_millis(500));
        }
    }

    #[test]
    fn test_build_backoff_unknown_strategy() {
        // 未知策略应该使用默认的 constant（直接构造 Aop 绕过验证）
        let aop = Aop {
            logger: None,
            retry_config: Some(RetryConfig {
                strategy: "unknown".to_string(),
                max_times: 2,
                ..Default::default()
            }),
            tracing_config: None,
            metrics_config: None,
            metric_total: None,
            metric_retry_count: None,
            metric_duration: None,
            metric_in_progress: None,
            metric_default_operation_label: None,
            metric_default_metric_labels: None,
            info_sample_rate: 1.0,
            warn_sample_rate: 1.0,
        };
        let mut backoff = aop.build_backoff().unwrap();

        // 使用默认 constant 策略
        assert_eq!(backoff.next(), Some(Duration::from_secs(1)));
        assert_eq!(backoff.next(), Some(Duration::from_secs(1)));
        assert_eq!(backoff.next(), None);
    }

    #[test]
    fn test_retry_config_with_jitter() {
        let config: AopCreateConfig = serde_json::from_str(
            r#"{
                "retry": {
                    "max_times": 3,
                    "strategy": "constant",
                    "delay": "100ms",
                    "jitter": true
                }
            }"#,
        )
        .unwrap();
        let aop = Aop::new(config).unwrap();
        let backoff = aop.build_backoff().unwrap();

        // jitter 会添加随机抖动，验证延迟存在
        let delays: Vec<_> = backoff.collect();
        assert_eq!(delays.len(), 3);
    }

    #[test]
    fn test_retry_config_serialize() {
        #[serde_as]
        #[derive(Serialize)]
        struct TestConfig {
            #[serde_as(as = "Option<HumanDur>")]
            delay: Option<Duration>,
        }

        let config = TestConfig {
            delay: Some(Duration::from_secs(30)),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"30s\""));

        let config_none = TestConfig { delay: None };
        let json_none = serde_json::to_string(&config_none).unwrap();
        assert!(json_none.contains("null"));
    }

    #[test]
    #[serial]
    fn test_extract_fixed_labels_from_config() {
        use std::collections::HashMap;

        // 确保环境变量是干净的
        std::env::remove_var("AOP_SERVICE");
        std::env::remove_var("AOP_ENV");
        std::env::remove_var("AOP_VERSION");
        std::env::remove_var("AOP_CLUSTER");

        let mut labels = HashMap::new();
        labels.insert("service".to_string(), "my-service".to_string());
        labels.insert("env".to_string(), "prod".to_string());
        labels.insert("version".to_string(), "1.0.0".to_string());
        labels.insert("cluster".to_string(), "cluster-1".to_string());

        let (service, env, version, cluster, host_ip) = extract_fixed_labels(&labels);

        assert_eq!(service, Some("my-service".to_string()));
        assert_eq!(env, Some("prod".to_string()));
        assert_eq!(version, Some("1.0.0".to_string()));
        assert_eq!(cluster, Some("cluster-1".to_string()));
        assert!(host_ip.is_some(), "host_ip should be auto-populated");
    }

    #[test]
    #[serial]
    fn test_extract_fixed_labels_from_env() {
        use std::collections::HashMap;

        // 确保环境变量是干净的
        std::env::remove_var("AOP_SERVICE");
        std::env::remove_var("AOP_ENV");
        std::env::remove_var("AOP_VERSION");
        std::env::remove_var("AOP_CLUSTER");

        // 设置环境变量
        std::env::set_var("AOP_SERVICE", "env-service");
        std::env::set_var("AOP_ENV", "test");
        std::env::set_var("AOP_VERSION", "2.0.0");
        std::env::set_var("AOP_CLUSTER", "cluster-2");

        let labels = HashMap::new();
        let (service, env, version, cluster, host_ip) = extract_fixed_labels(&labels);

        assert_eq!(service, Some("env-service".to_string()));
        assert_eq!(env, Some("test".to_string()));
        assert_eq!(version, Some("2.0.0".to_string()));
        assert_eq!(cluster, Some("cluster-2".to_string()));
        assert!(host_ip.is_some(), "host_ip should be auto-populated");

        // 清理环境变量
        std::env::remove_var("AOP_SERVICE");
        std::env::remove_var("AOP_ENV");
        std::env::remove_var("AOP_VERSION");
        std::env::remove_var("AOP_CLUSTER");
    }

    #[test]
    #[serial]
    fn test_extract_fixed_labels_priority() {
        use std::collections::HashMap;

        // 确保环境变量是干净的
        std::env::remove_var("AOP_SERVICE");
        std::env::remove_var("AOP_ENV");
        std::env::remove_var("AOP_VERSION");
        std::env::remove_var("AOP_CLUSTER");

        // 设置环境变量
        std::env::set_var("AOP_SERVICE", "env-service");
        std::env::set_var("AOP_ENV", "env-test");

        let mut labels = HashMap::new();
        labels.insert("service".to_string(), "config-service".to_string());
        // env 从配置中缺失，应该从环境变量读取
        // version 配置和环境变量都没有
        // cluster 从配置中提供

        labels.insert("cluster".to_string(), "config-cluster".to_string());

        let (service, env, version, cluster, host_ip) = extract_fixed_labels(&labels);

        // 配置值优先于环境变量
        assert_eq!(service, Some("config-service".to_string()));
        // 配置缺失则从环境变量读取
        assert_eq!(env, Some("env-test".to_string()));
        // 配置和环境变量都没有，则自动获取 git tag（如果编译时有 git 信息）
        assert!(
            version.is_some(),
            "version should be auto-populated from git tag"
        );
        // 配置值优先
        assert_eq!(cluster, Some("config-cluster".to_string()));
        // host_ip 自动获取
        assert!(host_ip.is_some(), "host_ip should be auto-populated");

        // 清理环境变量
        std::env::remove_var("AOP_SERVICE");
        std::env::remove_var("AOP_ENV");
    }

    #[test]
    #[serial]
    fn test_extract_fixed_labels_with_host_ip() {
        use std::collections::HashMap;

        // 确保环境变量是干净的
        std::env::remove_var("AOP_SERVICE");
        std::env::remove_var("AOP_HOST_IP");

        // 测试从配置中读取 host_ip
        let mut labels = HashMap::new();
        labels.insert("host_ip".to_string(), "192.168.1.100".to_string());

        let (_, _, _, _, host_ip) = extract_fixed_labels(&labels);
        assert_eq!(host_ip, Some("192.168.1.100".to_string()));

        // 清理
        std::env::remove_var("AOP_HOST_IP");
    }

    #[test]
    #[serial]
    fn test_extract_fixed_labels_host_ip_priority() {
        use std::collections::HashMap;

        // 确保环境变量是干净的
        std::env::remove_var("AOP_SERVICE");
        std::env::remove_var("AOP_HOST_IP");

        // 设置环境变量
        std::env::set_var("AOP_HOST_IP", "10.0.0.5");

        // 配置值优先于环境变量
        let mut labels = HashMap::new();
        labels.insert("host_ip".to_string(), "172.16.0.10".to_string());

        let (_, _, _, _, host_ip) = extract_fixed_labels(&labels);
        assert_eq!(host_ip, Some("172.16.0.10".to_string()));

        // 配置缺失时从环境变量读取
        let labels = HashMap::new();
        let (_, _, _, _, host_ip) = extract_fixed_labels(&labels);
        assert_eq!(host_ip, Some("10.0.0.5".to_string()));

        // 清理环境变量
        std::env::remove_var("AOP_SERVICE");
        std::env::remove_var("AOP_HOST_IP");
    }
}
