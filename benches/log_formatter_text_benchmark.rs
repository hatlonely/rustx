use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use rustx::log::{TextFormatter, TextFormatterConfig, LogLevel, LogRecord, LogFormatter};

fn benchmark_formatter(c: &mut Criterion) {
    let formatter_colored = TextFormatter::new(TextFormatterConfig { colored: true });
    let formatter_plain = TextFormatter::new(TextFormatterConfig { colored: false });

    // 基础记录
    let basic_record = LogRecord::new(
        LogLevel::Info,
        "This is a test message".to_string(),
    );

    // 带位置的记录
    let record_with_location = LogRecord::new(
        LogLevel::Error,
        "Error occurred in module".to_string(),
    )
    .with_location("src/module.rs".to_string(), 42);

    // 长消息记录
    let long_message = "A".repeat(1000);
    let record_with_long_message = LogRecord::new(
        LogLevel::Warn,
        long_message,
    );

    let mut group = c.benchmark_group("formatter");

    // Baseline: 什么都不做的基准测试
    group.bench_function("baseline", |b| {
        b.iter(|| {
            black_box(());
        })
    });

    // 测试不同场景
    let cases: [(&str, &LogRecord); 3] = [
        ("basic", &basic_record),
        ("with_location", &record_with_location),
        ("with_long_message", &record_with_long_message),
    ];

    for (name, record) in cases {
        group.bench_with_input(
            BenchmarkId::new("plain", name),
            record,
            |b, record: &LogRecord| {
                b.iter(|| {
                    black_box(formatter_plain.format(black_box(record)).unwrap())
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("colored", name),
            record,
            |b, record: &LogRecord| {
                b.iter(|| {
                    black_box(formatter_colored.format(black_box(record)).unwrap())
                })
            },
        );
    }

    group.finish();
}

fn benchmark_throughput(c: &mut Criterion) {
    let formatter = TextFormatter::new(TextFormatterConfig { colored: false });

    let mut group = c.benchmark_group("throughput");
    group.throughput(criterion::Throughput::Elements(1));

    // Baseline: 什么都不做的基准测试
    group.bench_function("baseline", |b| {
        b.iter(|| {
            black_box(());
        })
    });

    // 测试不同级别的吞吐量
    for level in [LogLevel::Error, LogLevel::Warn, LogLevel::Info, LogLevel::Debug] {
        let record = LogRecord::new(level, "Benchmark message".to_string());

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{:?}", level)),
            &record,
            |b, record| {
                b.iter(|| {
                    black_box(formatter.format(black_box(record)).unwrap())
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_formatter, benchmark_throughput);
criterion_main!(benches);
