//! Benchmarks for logging system performance
use criterion::{criterion_group, criterion_main, Criterion};
use snp::logging::{utils, ColorConfig, LogConfig, LogFormat};
use std::hint::black_box;
use std::time::Instant;
use tracing::{debug, info, warn, Level};

fn bench_logging_init(c: &mut Criterion) {
    c.bench_function("logging_init", |b| {
        b.iter(|| {
            let config = LogConfig {
                level: Level::INFO,
                format: LogFormat::Pretty,
                color: ColorConfig::Never,
                show_targets: false,
                show_timestamps: false,
            };
            // Note: We can't actually init multiple times in the same process
            // so we just benchmark config creation
            black_box(config);
        })
    });
}

fn bench_logging_overhead(c: &mut Criterion) {
    c.bench_function("logging_overhead", |b| {
        b.iter(|| {
            // Simulate typical logging operations
            let span = utils::hook_execution_span("test-hook", 10);
            let _enter = span.enter();

            info!(hook_id = "test", "Hook started");
            debug!("Processing file");
            warn!("Warning message");
            info!(duration_ms = 100u128, "Hook completed");
        })
    });
}

fn bench_logging_vs_no_logging(c: &mut Criterion) {
    let mut group = c.benchmark_group("logging_comparison");

    group.bench_function("with_logging", |b| {
        b.iter(|| {
            let start = Instant::now();
            for i in 0..1000 {
                let span = utils::hook_execution_span(&format!("hook-{i}"), 1);
                let _enter = span.enter();
                info!("Processing iteration {}", i);
                debug!("Debug message {}", i);
            }
            black_box(start.elapsed())
        })
    });

    group.bench_function("without_logging", |b| {
        b.iter(|| {
            let start = Instant::now();
            for i in 0..1000 {
                // Simulate the same work without logging
                black_box(format!("hook-{i}"));
                black_box(format!("Processing iteration {i}"));
                black_box(format!("Debug message {i}"));
            }
            black_box(start.elapsed())
        })
    });

    group.finish();
}

fn bench_span_creation(c: &mut Criterion) {
    c.bench_function("span_creation", |b| {
        b.iter(|| {
            let span = utils::hook_execution_span("test-hook", black_box(5));
            black_box(span);
        })
    });
}

criterion_group!(
    benches,
    bench_logging_init,
    bench_logging_overhead,
    bench_logging_vs_no_logging,
    bench_span_creation
);
criterion_main!(benches);
