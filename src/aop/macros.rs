/// AOP 宏 - 为方法调用添加切面功能（logging 和 retry）
///
/// # 使用方式
///
/// ```ignore
/// // 基本用法（参数不会被 clone）
/// aop!(&self.aop, self.client.get(key).await)
///
/// // 带 clone 的用法（指定的参数会在每次重试时 clone）
/// aop!(&self.aop, clone(value, options), self.client.put(key, value, options).await)
/// ```
///
/// # 参数
///
/// - `$aop`: `Option<Aop>` 的引用
/// - `clone(...)`: 可选，指定需要在重试时 clone 的参数列表
/// - `expression`: 要执行的表达式（必须返回 `Result<T, E>`）
///   操作名称会自动从表达式中提取（如 `self.client.get(key).await` -> "self.client.get"）
///
/// # 执行顺序
///
/// 1. 记录开始日志（如果启用 logging）
/// 2. 执行带重试的操作（如果启用 retry）
/// 3. 记录结束日志、耗时、结果（如果启用 logging）
///
/// # 示例
///
/// ```ignore
/// impl MyStruct {
///     // 简单方法，参数是引用，不需要 clone
///     pub async fn get_value(&self, key: &str) -> Result<String> {
///         aop!(&self.aop, self.client.get(key).await)
///     }
///
///     // 复杂方法，参数会被移动，需要 clone 以支持重试
///     pub async fn put_value(&self, key: &str, value: Bytes, options: Options) -> Result<()> {
///         aop!(&self.aop, clone(value, options), self.client.put(key, value, options).await)
///     }
/// }
/// ```
#[macro_export]
macro_rules! aop {
    // 带 clone 参数的版本
    ($aop:expr, clone($($clone_args:ident),+ $(,)?), $($tokens:tt)+) => {{
        $crate::__aop_extract_path_with_clone!($aop, [$($clone_args),+], [], $($tokens)+)
    }};

    // 不带 clone 参数的版本（原有行为）
    ($aop:expr, $($tokens:tt)+) => {{
        $crate::__aop_extract_path!($aop, [], $($tokens)+)
    }};
}

/// 内部宏 - 提取表达式路径（不带 clone）
#[doc(hidden)]
#[macro_export]
macro_rules! __aop_extract_path {
    // 匹配到函数调用 + await
    ($aop:expr, [$($path:tt)+], ( $($args:tt)* ) .await) => {
        $crate::__aop_execute!($aop, stringify!($($path)+), ($($args)*), $($path)+($($args)*).await)
    };

    // 匹配到函数调用 + await，后面还有其他链式调用
    ($aop:expr, [$($path:tt)+], ( $($args:tt)* ) .await $($rest:tt)+) => {
        $crate::__aop_extract_path!($aop, [$($path)+ ( $($args)* ) .await], $($rest)+)
    };

    // 匹配到函数调用（无 await）
    ($aop:expr, [$($path:tt)+], ( $($args:tt)* )) => {
        $crate::__aop_execute!($aop, stringify!($($path)+), ($($args)*), $($path)+($($args)*))
    };

    // 匹配到函数调用（无 await），后面还有其他调用
    ($aop:expr, [$($path:tt)+], ( $($args:tt)* ) . $($rest:tt)+) => {
        $crate::__aop_extract_path!($aop, [$($path)+ ( $($args)* ) .], $($rest)+)
    };

    // 继续累积路径
    ($aop:expr, [$($path:tt)*], $tt:tt $($rest:tt)*) => {
        $crate::__aop_extract_path!($aop, [$($path)* $tt], $($rest)*)
    };
}

/// 内部宏 - 提取表达式路径（带 clone）
#[doc(hidden)]
#[macro_export]
macro_rules! __aop_extract_path_with_clone {
    // 匹配到函数调用 + await
    ($aop:expr, [$($clone_args:ident),+], [$($path:tt)+], ( $($args:tt)* ) .await) => {
        $crate::__aop_execute_with_clone!($aop, [$($clone_args),+], stringify!($($path)+), ($($args)*), $($path)+($($args)*).await)
    };

    // 匹配到函数调用 + await，后面还有其他链式调用
    ($aop:expr, [$($clone_args:ident),+], [$($path:tt)+], ( $($args:tt)* ) .await $($rest:tt)+) => {
        $crate::__aop_extract_path_with_clone!($aop, [$($clone_args),+], [$($path)+ ( $($args)* ) .await], $($rest)+)
    };

    // 匹配到函数调用（无 await）
    ($aop:expr, [$($clone_args:ident),+], [$($path:tt)+], ( $($args:tt)* )) => {
        $crate::__aop_execute_with_clone!($aop, [$($clone_args),+], stringify!($($path)+), ($($args)*), $($path)+($($args)*))
    };

    // 匹配到函数调用（无 await），后面还有其他调用
    ($aop:expr, [$($clone_args:ident),+], [$($path:tt)+], ( $($args:tt)* ) . $($rest:tt)+) => {
        $crate::__aop_extract_path_with_clone!($aop, [$($clone_args),+], [$($path)+ ( $($args)* ) .], $($rest)+)
    };

    // 继续累积路径
    ($aop:expr, [$($clone_args:ident),+], [$($path:tt)*], $tt:tt $($rest:tt)*) => {
        $crate::__aop_extract_path_with_clone!($aop, [$($clone_args),+], [$($path)* $tt], $($rest)*)
    };
}

/// 内部宏 - 执行 AOP 逻辑（不带 clone）
#[doc(hidden)]
#[macro_export]
macro_rules! __aop_execute {
    ($aop:expr, $op_name:expr, ($($args:expr),*), $expr:expr) => {{
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;
        use tracing::Instrument;

        // 只在需要时创建 start_time
        let start_time = $aop.as_ref()
            .and_then(|aop| aop.logger.as_ref())
            .map(|_| Instant::now());

        // 获取 tracing 配置
        let tracing_config = $aop.as_ref().and_then(|aop| aop.tracing_config.as_ref());
        let tracing_name = tracing_config.map(|cfg| cfg.name.clone());
        let with_args = tracing_config.map(|cfg| cfg.with_args).unwrap_or(false);

        // Metric: 增加 in_progress
        'metric_inc: {
            let Some(ref aop) = $aop else { break 'metric_inc };
            let Some(ref in_progress) = aop.metric_in_progress else { break 'metric_inc };
            let Some(ref default_label) = aop.metric_default_operation_label else { break 'metric_inc };

            let labels = $crate::aop::aop::OperationLabel {
                operation: $op_name.to_string(),
                ..default_label.clone()
            };
            in_progress.get_or_create(&labels).inc();
        }

        // 定义执行闭包，每次执行创建 span
        let execute = || {
            // clone tracing_name 以支持多次调用（重试）
            let tracing_name = tracing_name.clone();
            async move {
                // 每次执行（包括重试）创建 span
                let span = tracing_name.as_ref().map(|name| {
                    tracing::info_span!(
                        "aop",
                        name = %name,
                        operation = %$op_name,
                        args = tracing::field::Empty,
                        result = tracing::field::Empty,
                        error = tracing::field::Empty,
                    )
                }).unwrap_or_else(tracing::Span::none);

                // 如果配置了 with_args，记录 args
                if with_args {
                    let args_debug = format!("{:?}", ($($args,)*));
                    span.record("args", args_debug);
                }

                async {
                    let result = $expr;
                    // 记录本次执行的结果
                    match &result {
                        Ok(v) => tracing::Span::current().record("result", format!("{:?}", v)),
                        Err(e) => tracing::Span::current().record("error", format!("{:?}", e)),
                    };
                    result
                }.instrument(span).await
            }
        };

        // 执行操作（带 retry 和 tracing）
        let (result, retry_count) = 'exec: {
            let Some(ref aop) = $aop else { break 'exec (execute().await, None) };
            let Some(backoff) = aop.build_backoff() else { break 'exec (execute().await, None) };

            use backon::Retryable;
            let retry_count = AtomicUsize::new(0);
            let result = execute.retry(backoff)
                .notify(|_err, _dur| {
                    retry_count.fetch_add(1, Ordering::SeqCst);
                })
                .await;
            (result, Some(retry_count.load(Ordering::SeqCst) as i64))
        };

        // Metric: 减少 in_progress 并记录结果指标
        'metric_record: {
            let Some(ref aop) = $aop else { break 'metric_record };
            let Some(ref default_op_label) = aop.metric_default_operation_label else { break 'metric_record };
            let Some(ref default_metric_labels) = aop.metric_default_metric_labels else { break 'metric_record };

            // 预创建 OperationLabel，复用避免多次 clone
            let op_label = $crate::aop::aop::OperationLabel {
                operation: $op_name.to_string(),
                ..default_op_label.clone()
            };

            // 减少 in_progress
            if let Some(ref in_progress) = aop.metric_in_progress {
                in_progress.get_or_create(&op_label).dec();
            }

            // 记录 duration
            if let Some(ref duration_metric) = aop.metric_duration {
                let duration = start_time.unwrap().elapsed();
                duration_metric.get_or_create(&op_label).observe(duration.as_millis() as f64);
            }

            // 记录 retry_count
            if let Some(count) = retry_count {
                if let Some(ref retry_count_metric) = aop.metric_retry_count {
                    retry_count_metric.get_or_create(&op_label).inc_by(count as u64);
                }
            }

            // 记录 total
            if let Some(ref total_metric) = aop.metric_total {
                let status = match &result {
                    Ok(_) => "success",
                    Err(_) => "error",
                };
                let labels = $crate::aop::aop::MetricLabels {
                    operation: $op_name.to_string(),
                    status: Some(status.to_string()),
                    ..default_metric_labels.clone()
                };
                total_metric.get_or_create(&labels).inc();
            }
        }

        // 记录结果日志
        'log: {
            let Some(ref aop) = $aop else { break 'log };
            let Some(ref logger) = aop.logger else { break 'log };

            let args_debug = format!("{:?}", ($($args,)*));
            let duration = start_time.unwrap().elapsed();

            use $crate::log::LogLevel;

            match &result {
                Ok(v) if ::rand::random::<f32>() < aop.info_sample_rate => {
                    let mut record = $crate::log::LogRecord::new(LogLevel::Info, format!("[AOP] {} completed", $op_name))
                        .with_location(file!().to_string(), line!())
                        .with_module(module_path!().to_string())
                        .with_metadata("operation", $op_name)
                        .with_metadata("args", args_debug)
                        .with_metadata("result", format!("{:?}", v))
                        .with_metadata("status", "success")
                        .with_metadata("duration_ms", duration.as_millis() as i64);
                    if let Some(count) = retry_count {
                        record = record.with_metadata("retry_count", count);
                    }
                    let _ = logger.log(record).await;
                }
                Err(e) if ::rand::random::<f32>() < aop.warn_sample_rate => {
                    let mut record = $crate::log::LogRecord::new(LogLevel::Warn, format!("[AOP] {} failed", $op_name))
                        .with_location(file!().to_string(), line!())
                        .with_module(module_path!().to_string())
                        .with_metadata("operation", $op_name)
                        .with_metadata("args", args_debug)
                        .with_metadata("error", format!("{:?}", e))
                        .with_metadata("status", "error")
                        .with_metadata("duration_ms", duration.as_millis() as i64);
                    if let Some(count) = retry_count {
                        record = record.with_metadata("retry_count", count);
                    }
                    let _ = logger.log(record).await;
                }
                _ => {}
            }
        }

        result
    }};
}

/// 内部宏 - 执行 AOP 逻辑（带 clone，支持重试）
#[doc(hidden)]
#[macro_export]
macro_rules! __aop_execute_with_clone {
    ($aop:expr, [$($clone_args:ident),+], $op_name:expr, ($($args:expr),*), $expr:expr) => {{
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;
        use tracing::Instrument;

        // 只在需要时创建 start_time
        let start_time = $aop.as_ref()
            .and_then(|aop| aop.logger.as_ref())
            .map(|_| Instant::now());

        // 获取 tracing 配置
        let tracing_config = $aop.as_ref().and_then(|aop| aop.tracing_config.as_ref());
        let tracing_name = tracing_config.map(|cfg| cfg.name.clone());
        let with_args = tracing_config.map(|cfg| cfg.with_args).unwrap_or(false);

        // Metric: 增加 in_progress
        'metric_inc: {
            let Some(ref aop) = $aop else { break 'metric_inc };
            let Some(ref in_progress) = aop.metric_in_progress else { break 'metric_inc };
            let Some(ref default_label) = aop.metric_default_operation_label else { break 'metric_inc };

            let labels = $crate::aop::aop::OperationLabel {
                operation: $op_name.to_string(),
                ..default_label.clone()
            };
            in_progress.get_or_create(&labels).inc();
        }

        // 定义执行闭包，在内部 clone 参数，每次执行创建 span
        let execute = || {
            // clone tracing_name 以支持多次调用（重试）
            let tracing_name = tracing_name.clone();
            $(let $clone_args = $clone_args.clone();)+
            async move {
                // 每次执行（包括重试）创建 span
                let span = tracing_name.as_ref().map(|name| {
                    tracing::info_span!(
                        "aop",
                        name = %name,
                        operation = %$op_name,
                        args = tracing::field::Empty,
                        result = tracing::field::Empty,
                        error = tracing::field::Empty,
                    )
                }).unwrap_or_else(tracing::Span::none);

                // 如果配置了 with_args，记录 args
                if with_args {
                    let args_debug = format!("{:?}", ($(&$clone_args,)*));
                    span.record("args", args_debug);
                }

                async {
                    let result = $expr;
                    // 记录本次执行的结果
                    match &result {
                        Ok(v) => tracing::Span::current().record("result", format!("{:?}", v)),
                        Err(e) => tracing::Span::current().record("error", format!("{:?}", e)),
                    };
                    result
                }.instrument(span).await
            }
        };

        // 执行操作（带 retry、tracing，参数会被 clone）
        let (result, retry_count) = 'exec: {
            let Some(ref aop) = $aop else { break 'exec (execute().await, None) };
            let Some(backoff) = aop.build_backoff() else { break 'exec (execute().await, None) };

            use backon::Retryable;
            let retry_count = AtomicUsize::new(0);
            let result = execute.retry(backoff)
                .notify(|_err, _dur| {
                    retry_count.fetch_add(1, Ordering::SeqCst);
                })
                .await;
            (result, Some(retry_count.load(Ordering::SeqCst) as i64))
        };

        // Metric: 减少 in_progress 并记录结果指标
        'metric_record: {
            let Some(ref aop) = $aop else { break 'metric_record };
            let Some(ref default_op_label) = aop.metric_default_operation_label else { break 'metric_record };
            let Some(ref default_metric_labels) = aop.metric_default_metric_labels else { break 'metric_record };

            // 预创建 OperationLabel，复用避免多次 clone
            let op_label = $crate::aop::aop::OperationLabel {
                operation: $op_name.to_string(),
                ..default_op_label.clone()
            };

            // 减少 in_progress
            if let Some(ref in_progress) = aop.metric_in_progress {
                in_progress.get_or_create(&op_label).dec();
            }

            // 记录 duration
            if let Some(ref duration_metric) = aop.metric_duration {
                let duration = start_time.unwrap().elapsed();
                duration_metric.get_or_create(&op_label).observe(duration.as_millis() as f64);
            }

            // 记录 retry_count
            if let Some(count) = retry_count {
                if let Some(ref retry_count_metric) = aop.metric_retry_count {
                    retry_count_metric.get_or_create(&op_label).inc_by(count as u64);
                }
            }

            // 记录 total
            if let Some(ref total_metric) = aop.metric_total {
                let status = match &result {
                    Ok(_) => "success",
                    Err(_) => "error",
                };
                let labels = $crate::aop::aop::MetricLabels {
                    operation: $op_name.to_string(),
                    status: Some(status.to_string()),
                    ..default_metric_labels.clone()
                };
                total_metric.get_or_create(&labels).inc();
            }
        }

        // 记录结果日志
        'log: {
            let Some(ref aop) = $aop else { break 'log };
            let Some(ref logger) = aop.logger else { break 'log };

            let args_debug = format!("{:?}", ($($args,)*));
            let duration = start_time.unwrap().elapsed();

            use $crate::log::LogLevel;

            match &result {
                Ok(v) if ::rand::random::<f32>() < aop.info_sample_rate => {
                    let mut record = $crate::log::LogRecord::new(LogLevel::Info, format!("[AOP] {} completed", $op_name))
                        .with_location(file!().to_string(), line!())
                        .with_module(module_path!().to_string())
                        .with_metadata("operation", $op_name)
                        .with_metadata("args", args_debug)
                        .with_metadata("result", format!("{:?}", v))
                        .with_metadata("status", "success")
                        .with_metadata("duration_ms", duration.as_millis() as i64);
                    if let Some(count) = retry_count {
                        record = record.with_metadata("retry_count", count);
                    }
                    let _ = logger.log(record).await;
                }
                Err(e) if ::rand::random::<f32>() < aop.warn_sample_rate => {
                    let mut record = $crate::log::LogRecord::new(LogLevel::Warn, format!("[AOP] {} failed", $op_name))
                        .with_location(file!().to_string(), line!())
                        .with_module(module_path!().to_string())
                        .with_metadata("operation", $op_name)
                        .with_metadata("args", args_debug)
                        .with_metadata("error", format!("{:?}", e))
                        .with_metadata("status", "error")
                        .with_metadata("duration_ms", duration.as_millis() as i64);
                    if let Some(count) = retry_count {
                        record = record.with_metadata("retry_count", count);
                    }
                    let _ = logger.log(record).await;
                }
                _ => {}
            }
        }

        result
    }};
}

/// 支持同步代码的 AOP 宏
///
/// # 使用方式
///
/// ```ignore
/// aop_sync!(&self.aop, expression)
/// ```
#[macro_export]
macro_rules! aop_sync {
    ($aop:expr, $($tokens:tt)+) => {{
        $crate::__aop_sync_extract_path!($aop, [], $($tokens)+)
    }};
}

/// 内部宏 - 提取同步表达式路径
#[doc(hidden)]
#[macro_export]
macro_rules! __aop_sync_extract_path {
    // 匹配到函数调用
    ($aop:expr, [$($path:tt)+], ( $($args:tt)* )) => {
        $crate::__aop_sync_execute!($aop, stringify!($($path)+), ($($args)*), $($path)+($($args)*))
    };

    // 匹配到函数调用，后面还有其他链式调用
    ($aop:expr, [$($path:tt)+], ( $($args:tt)* ) . $($rest:tt)+) => {
        $crate::__aop_sync_extract_path!($aop, [$($path)+ ( $($args)* ) .], $($rest)+)
    };

    // 继续累积路径
    ($aop:expr, [$($path:tt)*], $tt:tt $($rest:tt)*) => {
        $crate::__aop_sync_extract_path!($aop, [$($path)* $tt], $($rest)*)
    };
}

/// 内部宏 - 执行同步 AOP 逻辑
#[doc(hidden)]
#[macro_export]
macro_rules! __aop_sync_execute {
    ($aop:expr, $op_name:expr, ($($args:expr),*), $expr:expr) => {{
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;

        // 只在需要时创建 start_time
        let start_time = $aop.as_ref()
            .and_then(|aop| aop.logger.as_ref())
            .map(|_| Instant::now());

        // 获取 tracing 配置
        let tracing_config = $aop.as_ref().and_then(|aop| aop.tracing_config.as_ref());
        let tracing_name = tracing_config.map(|cfg| cfg.name.clone());
        let with_args = tracing_config.map(|cfg| cfg.with_args).unwrap_or(false);

        // Metric: 增加 in_progress
        'metric_inc: {
            let Some(ref aop) = $aop else { break 'metric_inc };
            let Some(ref in_progress) = aop.metric_in_progress else { break 'metric_inc };
            let Some(ref default_label) = aop.metric_default_operation_label else { break 'metric_inc };

            let labels = $crate::aop::aop::OperationLabel {
                operation: $op_name.to_string(),
                ..default_label.clone()
            };
            in_progress.get_or_create(&labels).inc();
        }

        // 定义执行闭包，每次执行创建 span
        let execute = || {
            // clone tracing_name 以支持多次调用（重试）
            let tracing_name = tracing_name.clone();

            // 每次执行（包括重试）创建 span
            let span = tracing_name.as_ref().map(|name| {
                tracing::info_span!(
                    "aop",
                    name = %name,
                    operation = %$op_name,
                    args = tracing::field::Empty,
                    result = tracing::field::Empty,
                    error = tracing::field::Empty,
                )
            }).unwrap_or_else(tracing::Span::none);

            // 如果配置了 with_args，记录 args
            if with_args {
                let args_debug = format!("{:?}", ($($args,)*));
                span.record("args", args_debug);
            }

            let _guard = span.enter();

            let result = $expr;
            // 记录本次执行的结果
            match &result {
                Ok(v) => span.record("result", format!("{:?}", v)),
                Err(e) => span.record("error", format!("{:?}", e)),
            };
            result
        };

        // 执行操作（带 retry）
        let (result, retry_count) = 'exec: {
            let Some(ref aop) = $aop else { break 'exec (execute(), None) };
            let Some(backoff) = aop.build_backoff() else { break 'exec (execute(), None) };

            use backon::BlockingRetryable;
            let retry_count = AtomicUsize::new(0);
            let result = execute.retry(backoff)
                .notify(|_err, _dur| {
                    retry_count.fetch_add(1, Ordering::SeqCst);
                })
                .call();
            (result, Some(retry_count.load(Ordering::SeqCst) as i64))
        };

        // Metric: 减少 in_progress 并记录结果指标
        'metric_record: {
            let Some(ref aop) = $aop else { break 'metric_record };
            let Some(ref default_op_label) = aop.metric_default_operation_label else { break 'metric_record };
            let Some(ref default_metric_labels) = aop.metric_default_metric_labels else { break 'metric_record };

            // 预创建 OperationLabel，复用避免多次 clone
            let op_label = $crate::aop::aop::OperationLabel {
                operation: $op_name.to_string(),
                ..default_op_label.clone()
            };

            // 减少 in_progress
            if let Some(ref in_progress) = aop.metric_in_progress {
                in_progress.get_or_create(&op_label).dec();
            }

            // 记录 duration
            if let Some(ref duration_metric) = aop.metric_duration {
                let duration = start_time.unwrap().elapsed();
                duration_metric.get_or_create(&op_label).observe(duration.as_millis() as f64);
            }

            // 记录 retry_count
            if let Some(count) = retry_count {
                if let Some(ref retry_count_metric) = aop.metric_retry_count {
                    retry_count_metric.get_or_create(&op_label).inc_by(count as u64);
                }
            }

            // 记录 total
            if let Some(ref total_metric) = aop.metric_total {
                let status = match &result {
                    Ok(_) => "success",
                    Err(_) => "error",
                };
                let labels = $crate::aop::aop::MetricLabels {
                    operation: $op_name.to_string(),
                    status: Some(status.to_string()),
                    ..default_metric_labels.clone()
                };
                total_metric.get_or_create(&labels).inc();
            }
        }

        // 记录结果日志
        'log: {
            let Some(ref aop) = $aop else { break 'log };
            let Some(ref logger) = aop.logger else { break 'log };
            let Ok(handle) = tokio::runtime::Handle::try_current() else { break 'log };

            let args_debug = format!("{:?}", ($($args,)*));
            let duration = start_time.unwrap().elapsed();

            use $crate::log::LogLevel;

            match &result {
                Ok(v) if ::rand::random::<f32>() < aop.info_sample_rate => {
                    let mut record = $crate::log::LogRecord::new(LogLevel::Info, format!("[AOP] {} completed", $op_name))
                        .with_location(file!().to_string(), line!())
                        .with_module(module_path!().to_string())
                        .with_metadata("operation", $op_name)
                        .with_metadata("args", args_debug)
                        .with_metadata("result", format!("{:?}", v))
                        .with_metadata("status", "success")
                        .with_metadata("duration_ms", duration.as_millis() as i64);
                    if let Some(count) = retry_count {
                        record = record.with_metadata("retry_count", count);
                    }
                    let lg = logger.clone();
                    handle.spawn(async move {
                        let _ = lg.log(record).await;
                    });
                }
                Err(e) if ::rand::random::<f32>() < aop.warn_sample_rate => {
                    let mut record = $crate::log::LogRecord::new(LogLevel::Warn, format!("[AOP] {} failed", $op_name))
                        .with_location(file!().to_string(), line!())
                        .with_module(module_path!().to_string())
                        .with_metadata("operation", $op_name)
                        .with_metadata("args", args_debug)
                        .with_metadata("error", format!("{:?}", e))
                        .with_metadata("status", "error")
                        .with_metadata("duration_ms", duration.as_millis() as i64);
                    if let Some(count) = retry_count {
                        record = record.with_metadata("retry_count", count);
                    }
                    let lg = logger.clone();
                    handle.spawn(async move {
                        let _ = lg.log(record).await;
                    });
                }
                _ => {}
            }
        }

        result
    }};
}
