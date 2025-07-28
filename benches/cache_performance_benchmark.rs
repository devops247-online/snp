// Performance benchmarks for multi-tier cache integration
// Measures the impact of L1/L2/L3 caching on regex operations in SNP

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use regex::Regex;
use snp::enhanced_regex_processor::EnhancedRegexProcessor;
use snp::filesystem::FileFilter;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Benchmark comparing direct regex compilation vs multi-tier cached compilation
fn bench_regex_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("regex_compilation");

    // Common regex patterns used in typical pre-commit scenarios
    let patterns = [
        r".*\.py$",
        r".*\.js$",
        r".*\.ts$",
        r".*\.rs$",
        r"__pycache__",
        r"\.git/",
        r"node_modules/",
        r"target/",
        r"\.pyc$",
        r"\.tmp$",
    ];

    group.throughput(Throughput::Elements(patterns.len() as u64));

    // Benchmark direct regex compilation (baseline)
    group.bench_function("direct_regex_new", |b| {
        b.iter(|| {
            for pattern in &patterns {
                let _ = Regex::new(pattern).unwrap();
            }
        })
    });

    // Benchmark enhanced regex processor (with multi-tier cache)
    group.bench_function("enhanced_regex_processor", |b| {
        let processor = EnhancedRegexProcessor::new();
        b.iter(|| {
            for pattern in &patterns {
                let _ = processor.compile_regex(pattern).unwrap();
            }
        })
    });

    group.finish();
}

/// Benchmark file filtering performance with and without multi-tier cache
fn bench_file_filtering(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_filtering");

    // Generate test files similar to a real project
    let test_files: Vec<PathBuf> = (0..1000)
        .flat_map(|i| {
            vec![
                PathBuf::from(format!("src/module_{i}.py")),
                PathBuf::from(format!("src/component_{i}.js")),
                PathBuf::from(format!("tests/test_{i}.py")),
                PathBuf::from(format!("__pycache__/module_{i}.pyc")),
                PathBuf::from(format!("node_modules/package_{i}/index.js")),
            ]
        })
        .collect();

    group.throughput(Throughput::Elements(test_files.len() as u64));

    // Benchmark with multi-tier cached FileFilter
    group.bench_function("cached_file_filter", |b| {
        let filter = FileFilter::new()
            .with_include_patterns(vec![r".*\.py$".to_string(), r".*\.js$".to_string()])
            .unwrap()
            .with_exclude_patterns(vec![
                r"__pycache__".to_string(),
                r"node_modules".to_string(),
            ])
            .unwrap();

        b.iter(|| {
            let _ = filter.filter_files(&test_files).unwrap();
        })
    });

    group.finish();
}

/// Benchmark cache hit performance across different tiers
fn bench_cache_tiers(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_tiers");

    let processor = EnhancedRegexProcessor::new();
    let patterns = [
        r".*\.py$",  // Will be promoted to L1 (hot)
        r".*\.js$",  // Will be in L2 (warm)
        r".*\.old$", // Will remain in L3 (cold)
    ];

    // Pre-populate cache with different access patterns
    for (i, pattern) in patterns.iter().enumerate() {
        processor.compile_regex(pattern).unwrap();

        // Create different access patterns to test tier promotion
        for _ in 0..i * 5 {
            processor.is_match(pattern, "test.py").unwrap();
        }
    }

    // Give cache time to settle into tiers
    std::thread::sleep(Duration::from_millis(100));

    group.bench_function("hot_pattern_l1", |b| {
        b.iter(|| processor.is_match(r".*\.py$", "test.py").unwrap())
    });

    group.bench_function("warm_pattern_l2", |b| {
        b.iter(|| processor.is_match(r".*\.js$", "test.js").unwrap())
    });

    group.bench_function("cold_pattern_l3", |b| {
        b.iter(|| processor.is_match(r".*\.old$", "test.old").unwrap())
    });

    group.finish();
}

/// Benchmark repeated filtering operations (simulating real SNP usage)
fn bench_repeated_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("repeated_operations");

    let test_files: Vec<PathBuf> = vec![
        PathBuf::from("src/main.py"),
        PathBuf::from("src/utils.py"),
        PathBuf::from("tests/test_main.py"),
        PathBuf::from("__pycache__/main.pyc"),
        PathBuf::from("README.md"),
        PathBuf::from("setup.py"),
        PathBuf::from("requirements.txt"),
    ];

    // This simulates a typical pre-commit scenario where the same patterns
    // are applied to files multiple times during development
    group.bench_function("repeated_file_filtering", |b| {
        let filter = FileFilter::new()
            .with_include_patterns(vec![r".*\.py$".to_string()])
            .unwrap()
            .with_exclude_patterns(vec![r"__pycache__".to_string()])
            .unwrap();

        b.iter(|| {
            // Simulate 10 repeated filter operations
            for _ in 0..10 {
                let _ = filter.filter_files(&test_files).unwrap();
            }
        })
    });

    group.finish();
}

/// Benchmark memory usage and cache efficiency
fn bench_cache_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_memory_efficiency");

    // Test with many unique patterns to test cache eviction
    let patterns: Vec<String> = (0..1000).map(|i| format!(r".*\.type{i}$")).collect();

    group.throughput(Throughput::Elements(patterns.len() as u64));

    group.bench_function("large_pattern_set", |b| {
        let processor = EnhancedRegexProcessor::new();

        b.iter(|| {
            for pattern in &patterns {
                let _ = processor.compile_regex(pattern).unwrap();
            }
        })
    });

    group.finish();
}

/// Benchmark concurrent access to multi-tier cache
fn bench_concurrent_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_access");

    let processor = Arc::new(EnhancedRegexProcessor::new());
    let patterns = vec![
        r".*\.py$".to_string(),
        r".*\.js$".to_string(),
        r".*\.rs$".to_string(),
    ];

    group.bench_function("concurrent_compilation", |b| {
        b.iter(|| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let processor_clone = Arc::clone(&processor);
                    let patterns_clone = patterns.clone();
                    std::thread::spawn(move || {
                        for pattern in &patterns_clone {
                            let _ = processor_clone.compile_regex(pattern).unwrap();
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_regex_compilation,
    bench_file_filtering,
    bench_cache_tiers,
    bench_repeated_operations,
    bench_cache_memory_efficiency,
    bench_concurrent_access
);

criterion_main!(benches);
