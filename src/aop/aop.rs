use crate::cfg::serde_duration::{serde_as, HumanDur};
use crate::log::Logger;
use anyhow::Result;
use backon::{BackoffBuilder, ConstantBuilder, ExponentialBuilder, FibonacciBuilder};
use garde::Validate;
use serde::Deserialize;
use smart_default::SmartDefault;
use std::sync::Arc;
use std::time::Duration;

use crate::log::LoggerConfig;

/// AOP 创建配置（用于创建新的 AOP 实例）
#[derive(Debug, Clone, Deserialize, SmartDefault)]
#[serde(default)]
pub struct AopCreateConfig {
    /// Logging 配置
    pub logging: Option<LoggingConfig>,

    /// Retry 配置
    pub retry: Option<RetryConfig>,
}

/// Logging 配置
#[derive(Debug, Clone, Deserialize, SmartDefault, Validate)]
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

/// Retry 配置
#[serde_as]
#[derive(Debug, Clone, Deserialize, SmartDefault, Validate)]
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

        Ok(Self {
            logger,
            retry_config: config.retry,
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
}
