// Focused benchmark to demonstrate multi-tier cache effectiveness
// Shows the performance benefit of cached vs uncached regex operations

use criterion::{criterion_group, criterion_main, Criterion};
use regex::Regex;
use snp::enhanced_regex_processor::EnhancedRegexProcessor;
use snp::filesystem::FileFilter;
use std::path::PathBuf;

/// Benchmark comparing repeated regex operations with and without caching
fn bench_cache_effectiveness(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_effectiveness");
    group.sample_size(50); // Reduce sample size for faster benchmarks

    // Common patterns used repeatedly in pre-commit scenarios
    let patterns = [r".*\.py$", r".*\.js$", r"__pycache__"];
    let test_text = "src/module.py";

    // Benchmark without caching (direct regex compilation each time)
    group.bench_function("without_cache", |b| {
        b.iter(|| {
            for pattern in &patterns {
                for _ in 0..10 {
                    let regex = Regex::new(pattern).unwrap();
                    let _ = regex.is_match(test_text);
                }
            }
        })
    });

    // Benchmark with multi-tier cache
    group.bench_function("with_multi_tier_cache", |b| {
        let processor = EnhancedRegexProcessor::new();

        // Pre-warm the cache
        for pattern in &patterns {
            processor.compile_regex(pattern).unwrap();
        }

        b.iter(|| {
            for pattern in &patterns {
                for _ in 0..10 {
                    let _ = processor.is_match(pattern, test_text).unwrap();
                }
            }
        })
    });

    group.finish();
}

/// Benchmark file filtering scenarios (typical SNP usage)
fn bench_file_filtering_scenarios(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_filtering_scenarios");
    group.sample_size(30); // Reduce for faster execution

    // Realistic file set from a Python project
    let test_files: Vec<PathBuf> = vec![
        PathBuf::from("src/main.py"),
        PathBuf::from("src/utils.py"),
        PathBuf::from("src/models.py"),
        PathBuf::from("tests/test_main.py"),
        PathBuf::from("tests/test_utils.py"),
        PathBuf::from("__pycache__/main.pyc"),
        PathBuf::from("__pycache__/utils.pyc"),
        PathBuf::from("README.md"),
        PathBuf::from("setup.py"),
        PathBuf::from(".gitignore"),
    ];

    // Benchmark typical pre-commit filtering (Python files only, exclude pycache)
    group.bench_function("python_files_filtering", |b| {
        let filter = FileFilter::new()
            .with_include_patterns(vec![r".*\.py$".to_string()])
            .unwrap()
            .with_exclude_patterns(vec![r"__pycache__".to_string()])
            .unwrap();

        b.iter(|| {
            // Simulate multiple filtering operations in one pre-commit run
            for _ in 0..5 {
                let _ = filter.filter_files(&test_files).unwrap();
            }
        })
    });

    group.finish();
}

/// Demonstrate cache warming and tier performance
fn bench_cache_tiers_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_tiers");
    group.sample_size(100);

    let processor = EnhancedRegexProcessor::new();

    // Pre-populate and create access patterns for different tiers
    let hot_pattern = r".*\.py$"; // Will be L1 (hot)
    let warm_pattern = r".*\.js$"; // Will be L2 (warm)
    let cold_pattern = r".*\.tmp$"; // Will be L3 (cold)

    // Create different access patterns
    processor.compile_regex(hot_pattern).unwrap();
    processor.compile_regex(warm_pattern).unwrap();
    processor.compile_regex(cold_pattern).unwrap();

    // Access hot pattern frequently to promote to L1
    for _ in 0..10 {
        processor.is_match(hot_pattern, "test.py").unwrap();
    }

    // Access warm pattern moderately to keep in L2
    for _ in 0..3 {
        processor.is_match(warm_pattern, "test.js").unwrap();
    }

    // Cold pattern accessed once, stays in L3
    processor.is_match(cold_pattern, "test.tmp").unwrap();

    group.bench_function("hot_pattern_access", |b| {
        b.iter(|| processor.is_match(hot_pattern, "test.py").unwrap())
    });

    group.bench_function("warm_pattern_access", |b| {
        b.iter(|| processor.is_match(warm_pattern, "test.js").unwrap())
    });

    group.bench_function("cold_pattern_access", |b| {
        b.iter(|| processor.is_match(cold_pattern, "test.tmp").unwrap())
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_cache_effectiveness,
    bench_file_filtering_scenarios,
    bench_cache_tiers_performance
);

criterion_main!(benches);
