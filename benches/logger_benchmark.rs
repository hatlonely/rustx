use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rustx::log::{Logger, LoggerConfig, LogLevel, register_formatters, register_appenders};

/// 创建 benchmark 用的 logger
fn create_benchmark_logger(colored: bool) -> Logger {
    // 注册类型
    register_formatters().unwrap();
    register_appenders().unwrap();

    LoggerConfig {
        level: "debug".to_string(),
        formatter: rustx::cfg::TypeOptions::from_json(
            &format!(r#"{{"type":"TextFormatter","options":{{"colored":{}}}}}"#, colored)
        ).unwrap(),
        appender: rustx::cfg::TypeOptions::from_json(
            r#"{"type":"ConsoleAppender","options":{"use_colors":false}}"#
        ).unwrap(),
    }.into()
}

fn benchmark_basic_logging(c: &mut Criterion) {
    let logger_plain = create_benchmark_logger(false);
    let logger_colored = create_benchmark_logger(true);

    let mut group = c.benchmark_group("logger_basic");

    // 测试简单的日志输出
    group.bench_function("plain", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                black_box(
                    logger_plain.info(black_box("Simple log message")).await
                )
            })
        })
    });

    group.bench_function("colored", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                black_box(
                    logger_colored.info(black_box("Simple log message")).await
                )
            })
        })
    });

    group.finish();
}

fn benchmark_different_message_sizes(c: &mut Criterion) {
    let logger = create_benchmark_logger(false);

    let mut group = c.benchmark_group("message_sizes");

    for size in [10, 50, 100, 500, 1000].iter() {
        let message = "x".repeat(*size);

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &message,
            |b, msg| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                b.iter(|| {
                    rt.block_on(async {
                        black_box(logger.info(black_box(msg.clone())).await)
                    })
                })
            },
        );
    }

    group.finish();
}

fn benchmark_with_metadata(c: &mut Criterion) {
    let logger = create_benchmark_logger(false);

    let mut group = c.benchmark_group("with_metadata");

    // 不带 metadata
    group.bench_function("no_metadata", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                black_box(
                    logger.info(black_box("Message without metadata")).await
                )
            })
        })
    });

    // 带 1 个 metadata
    group.bench_function("one_metadata", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                black_box(
                    logger.infom(
                        black_box("Message with metadata"),
                        vec![("key1".to_string(), black_box(12345i64.into()))]
                    ).await
                )
            })
        })
    });

    // 带 3 个 metadata
    group.bench_function("three_metadata", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                black_box(
                    logger.infom(
                        black_box("Message with metadata"),
                        vec![
                            ("user_id".to_string(), black_box(12345i64.into())),
                            ("action".to_string(), black_box("login".into())),
                            ("success".to_string(), black_box(true.into())),
                        ]
                    ).await
                )
            })
        })
    });

    // 带 10 个 metadata
    group.bench_function("ten_metadata", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let metadata: Vec<(String, rustx::log::record::MetadataValue)> = (0..10)
                    .map(|i| (format!("key_{}", i), black_box(i.into())))
                    .collect();
                black_box(
                    logger.infom(
                        black_box("Message with many metadata"),
                        metadata
                    ).await
                )
            })
        })
    });

    group.finish();
}

fn benchmark_throughput(c: &mut Criterion) {
    let logger = create_benchmark_logger(false);

    let mut group = c.benchmark_group("throughput");
    group.throughput(Throughput::Elements(1));

    // 测试不同日志级别的吞吐量
    for level in [LogLevel::Trace, LogLevel::Debug, LogLevel::Info, LogLevel::Warn, LogLevel::Error] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{:?}", level)),
            &level,
            |b, &level| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                b.iter(|| {
                    rt.block_on(async {
                        let record = rustx::log::record::LogRecord::new(level, "Benchmark message".to_string());
                        black_box(logger.log(record)).await
                    })
                })
            },
        );
    }

    group.finish();
}

fn benchmark_concurrent_logging(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent");

    // 单线程
    group.bench_function("single_task", |b| {
        let logger = create_benchmark_logger(false);
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                black_box(
                    logger.info(black_box("Concurrent log message")).await
                )
            })
        })
    });

    // 多个并发任务
    for tasks in [2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_tasks", tasks),
            tasks,
            |b, &n| {
                use std::sync::Arc;
                let logger = Arc::new(create_benchmark_logger(false));
                let rt = tokio::runtime::Runtime::new().unwrap();
                b.iter(|| {
                    let logger = logger.clone();
                    rt.block_on(async move {
                        let handles: Vec<_> = (0..n)
                            .map(|_| {
                                let logger = logger.clone();
                                tokio::spawn(async move {
                                    black_box(
                                        logger.info(black_box("Concurrent log message")).await
                                    )
                                })
                            })
                            .collect();

                        for handle in handles {
                            let _ = handle.await.unwrap();
                        }
                    })
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_basic_logging,
    benchmark_different_message_sizes,
    benchmark_with_metadata,
    benchmark_throughput,
    benchmark_concurrent_logging
);
criterion_main!(benches);
