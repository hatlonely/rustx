/// AOP 宏 - 为方法调用添加切面功能（logging 和 retry）
///
/// # 使用方式
///
/// ```ignore
/// aop!(&self.aop, expression)
/// ```
///
/// # 参数
///
/// - `$aop`: `Option<Aop>` 的引用
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
///     pub async fn get_value(&self, key: &str) -> Result<String> {
///         aop!(&self.aop, self.client.get(key).await)
///     }
/// }
/// ```
#[macro_export]
macro_rules! aop {
    ($aop:expr, $($tokens:tt)+) => {{
        $crate::__aop_extract_path!($aop, [], $($tokens)+)
    }};
}

/// 内部宏 - 提取表达式路径
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

/// 内部宏 - 执行 AOP 逻辑
#[doc(hidden)]
#[macro_export]
macro_rules! __aop_execute {
    ($aop:expr, $op_name:expr, ($($args:expr),*), $expr:expr) => {{
        use std::cell::Cell;
        use std::time::Instant;

        // 只在需要时创建 start_time
        let (start_time, need_log) = if let Some(ref aop) = $aop {
            (aop.logger.is_some().then(Instant::now), aop.logger.is_some())
        } else {
            (None, false)
        };

        // 执行操作（带 retry）
        let (result, retry_count) = if let Some(ref aop) = $aop {
            if aop.retry_config.is_some() {
                // 使用 backon 的 Retryable trait
                use backon::Retryable;

                let backoff = aop.build_backoff();
                let retry_count = Cell::new(0);

                if let Some(backoff) = backoff {
                    let logger = aop.logger.clone();

                    let result = (|| async { $expr }).retry(backoff)
                    .notify(|err, dur: std::time::Duration| {
                        let current_retry = retry_count.get() + 1;
                        retry_count.set(current_retry);

                        if let Ok(handle) = tokio::runtime::Handle::try_current() {
                            use $crate::log::LogLevel;
                            let record = $crate::log::LogRecord::new(
                                LogLevel::Warn,
                                format!("[AOP] {} retry {}", $op_name, current_retry)
                            )
                            .with_location(file!().to_string(), line!())
                            .with_module(module_path!().to_string())
                            .with_metadata("operation", $op_name)
                            .with_metadata("retry_count", current_retry as i64)
                            .with_metadata("error", format!("{:?}", err))
                            .with_metadata("retry_delay_ms", dur.as_millis() as i64);
                            if let Some(ref lg) = logger {
                                let lg = lg.clone();
                                handle.spawn(async move {
                                    let _ = lg.log(record).await;
                                });
                            }
                        }
                    })
                    .await;

                    (result, Some(retry_count.get()))
                } else {
                    ($expr, None)
                }
            } else {
                ($expr, None)
            }
        } else {
            ($expr, None)
        };

        // 记录结果日志（根据采样率）
        if need_log {
            if let Some(ref aop) = $aop {
                if let Some(ref logger) = aop.logger {
                    // 格式化参数（用于日志）
                    let args_debug = format!("{:?}", ($($args,)*));

                    // 计算耗时
                    let duration = start_time.unwrap().elapsed();

                    use $crate::log::LogLevel;

                    match &result {
                        Ok(v) => {
                            if ::rand::random::<f32>() < aop.info_sample_rate {
                                let mut record = $crate::log::LogRecord::new(LogLevel::Info, format!("[AOP] {} completed", $op_name))
                                    .with_location(file!().to_string(), line!())
                                    .with_module(module_path!().to_string())
                                    .with_metadata("operation", $op_name)
                                    .with_metadata("args", args_debug)
                                    .with_metadata("result", format!("{:?}", v))
                                    .with_metadata("status", "success")
                                    .with_metadata("duration_ms", duration.as_millis() as i64);
                                // 如果有重试，添加重试次数到日志
                                if let Some(count) = retry_count {
                                    record = record.with_metadata("retry_count", count);
                                }
                                let _ = logger.log(record).await;
                            }
                        }
                        Err(e) => {
                            if ::rand::random::<f32>() < aop.warn_sample_rate {
                                let mut record = $crate::log::LogRecord::new(LogLevel::Warn, format!("[AOP] {} failed", $op_name))
                                    .with_location(file!().to_string(), line!())
                                    .with_module(module_path!().to_string())
                                    .with_metadata("operation", $op_name)
                                    .with_metadata("args", args_debug)
                                    .with_metadata("error", format!("{:?}", e))
                                    .with_metadata("status", "error")
                                    .with_metadata("duration_ms", duration.as_millis() as i64);
                                // 如果有重试，添加重试次数到日志
                                if let Some(count) = retry_count {
                                    record = record.with_metadata("retry_count", count);
                                }
                                let _ = logger.log(record).await;
                            }
                        }
                    }
                }
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
        use std::time::Instant;

        // 只在需要时创建 start_time
        let (start_time, need_log) = if let Some(ref aop) = $aop {
            (aop.logger.is_some().then(Instant::now), aop.logger.is_some())
        } else {
            (None, false)
        };

        // 执行操作（同步，不支持 retry）
        let result = $expr;

        // 记录结果日志（根据采样率）
        if need_log {
            if let Some(ref aop) = $aop {
                if let Some(ref logger) = aop.logger {
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        use $crate::log::LogLevel;

                        // 格式化参数（用于日志）
                        let args_debug = format!("{:?}", ($($args,)*));

                        // 计算耗时
                        let duration = start_time.unwrap().elapsed();

                        match &result {
                            Ok(v) => {
                                if ::rand::random::<f32>() < aop.info_sample_rate {
                                    let record = $crate::log::LogRecord::new(LogLevel::Info, format!("[AOP] {} completed", $op_name))
                                        .with_location(file!().to_string(), line!())
                                        .with_module(module_path!().to_string())
                                        .with_metadata("operation", $op_name)
                                        .with_metadata("args", args_debug)
                                        .with_metadata("result", format!("{:?}", v))
                                        .with_metadata("status", "success")
                                        .with_metadata("duration_ms", duration.as_millis() as i64);
                                    let lg = logger.clone();
                                    handle.spawn(async move {
                                        let _ = lg.log(record).await;
                                    });
                                }
                            }
                            Err(e) => {
                                if ::rand::random::<f32>() < aop.warn_sample_rate {
                                    let record = $crate::log::LogRecord::new(LogLevel::Warn, format!("[AOP] {} failed", $op_name))
                                        .with_location(file!().to_string(), line!())
                                        .with_module(module_path!().to_string())
                                        .with_metadata("operation", $op_name)
                                        .with_metadata("args", args_debug)
                                        .with_metadata("error", format!("{:?}", e))
                                        .with_metadata("status", "error")
                                        .with_metadata("duration_ms", duration.as_millis() as i64);
                                    let lg = logger.clone();
                                    handle.spawn(async move {
                                        let _ = lg.log(record).await;
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        result
    }};
}
