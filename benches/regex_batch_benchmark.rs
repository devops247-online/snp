// Benchmark comparing individual vs batch regex processing performance
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use snp::regex_processor::{OptimizedBatchRegexProcessor, RegexConfig, RegexProcessor};
use std::hint::black_box;

// Removed unused TEST_FILES constant

const LANGUAGE_PATTERNS: &[(&str, &str)] = &[
    ("python", r"\.py$"),
    ("rust", r"\.rs$"),
    ("javascript", r"\.(js|ts|jsx|tsx)$"),
    ("java", r"\.java$"),
    ("go", r"\.go$"),
    ("ruby", r"\.rb$"),
    ("php", r"\.php$"),
    ("cpp", r"\.(cpp|hpp|cc|c|h)$"),
    ("markdown", r"\.md$"),
    ("yaml", r"\.(yaml|yml)$"),
    ("json", r"\.json$"),
    ("toml", r"\.toml$"),
    ("text", r"\.txt$"),
    ("tests", r"tests?/.*\.py$"),
    ("src", r"src/.*\.(py|rs|js|ts)$"),
];

fn generate_more_files(count: usize) -> Vec<String> {
    let extensions = [
        "py", "rs", "js", "ts", "java", "go", "rb", "php", "cpp", "h", "md", "yaml",
    ];
    (0..count)
        .map(|i| {
            let ext = extensions[i % extensions.len()];
            format!("file_{i}.{ext}")
        })
        .collect()
}

fn benchmark_individual_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("individual_processing");

    for file_count in [100, 500, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("files", file_count),
            file_count,
            |b, &file_count| {
                let test_files = generate_more_files(file_count);
                let mut processor = RegexProcessor::new(RegexConfig::default());

                b.iter(|| {
                    let mut matches = 0;
                    for file in &test_files {
                        for (_, pattern) in LANGUAGE_PATTERNS {
                            if processor.is_match(pattern, file).unwrap_or(false) {
                                matches += 1;
                            }
                        }
                    }
                    black_box(matches);
                });
            },
        );
    }
    group.finish();
}

fn benchmark_batch_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_processing");

    for file_count in [100, 500, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("files", file_count),
            file_count,
            |b, &file_count| {
                let test_files = generate_more_files(file_count);
                let patterns: Vec<(String, String)> = LANGUAGE_PATTERNS
                    .iter()
                    .map(|(name, pattern)| (name.to_string(), pattern.to_string()))
                    .collect();
                let processor = OptimizedBatchRegexProcessor::new(patterns).unwrap();

                b.iter(|| {
                    let mut matches = 0;
                    for file in &test_files {
                        matches += processor.match_all(file).len();
                    }
                    black_box(matches);
                });
            },
        );
    }
    group.finish();
}

fn benchmark_pattern_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("pattern_compilation");

    let patterns: Vec<(String, String)> = LANGUAGE_PATTERNS
        .iter()
        .map(|(name, pattern)| (name.to_string(), pattern.to_string()))
        .collect();

    group.bench_function("individual_compilation", |b| {
        b.iter(|| {
            let mut processor = RegexProcessor::new(RegexConfig::default());
            for (_, pattern) in &patterns {
                let _ = processor.compile(pattern).unwrap();
            }
        });
    });

    group.bench_function("batch_compilation", |b| {
        b.iter(|| {
            let _ = OptimizedBatchRegexProcessor::new(patterns.clone()).unwrap();
        });
    });

    group.finish();
}

fn benchmark_small_vs_large_pattern_sets(c: &mut Criterion) {
    let mut group = c.benchmark_group("pattern_set_size");

    let test_files = generate_more_files(500);

    for pattern_count in [3, 10, 25, 50].iter() {
        let patterns: Vec<(String, String)> = (0..*pattern_count)
            .map(|i| (format!("pattern_{i}"), format!(r"pattern_{i}.*\.ext$")))
            .collect();

        group.bench_with_input(
            BenchmarkId::new("batch", pattern_count),
            pattern_count,
            |b, _| {
                let processor = OptimizedBatchRegexProcessor::new(patterns.clone()).unwrap();
                b.iter(|| {
                    let mut matches = 0;
                    for file in &test_files {
                        matches += processor.match_all(file).len();
                    }
                    black_box(matches);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("individual", pattern_count),
            pattern_count,
            |b, _| {
                let mut processor = RegexProcessor::new(RegexConfig::default());
                b.iter(|| {
                    let mut matches = 0;
                    for file in &test_files {
                        for (_, pattern) in &patterns {
                            if processor.is_match(pattern, file).unwrap_or(false) {
                                matches += 1;
                            }
                        }
                    }
                    black_box(matches);
                });
            },
        );
    }

    group.finish();
}

fn benchmark_real_world_scenario(c: &mut Criterion) {
    let mut group = c.benchmark_group("real_world");

    // Simulate a typical pre-commit scenario with file filtering
    let file_patterns = vec![
        ("files".to_string(), r"\.(py|rs|js|ts|java|go)$".to_string()),
        (
            "exclude".to_string(),
            r"(__pycache__|\.git|node_modules)/".to_string(),
        ),
        (
            "tests".to_string(),
            r"tests?/.*\.(py|rs|js|ts)$".to_string(),
        ),
        ("src".to_string(), r"src/.*\.(py|rs|js|ts)$".to_string()),
        ("config".to_string(), r"\.(json|yaml|toml|cfg)$".to_string()),
    ];

    let realistic_files = vec![
        "src/main.py",
        "src/lib.py",
        "src/utils.py",
        "src/__pycache__/main.pyc",
        "tests/test_main.py",
        "tests/test_lib.py",
        "tests/__pycache__/test.pyc",
        "lib.rs",
        "main.rs",
        "config.rs",
        "src/core.rs",
        "tests/integration_test.rs",
        "app.js",
        "index.ts",
        "components/Button.tsx",
        "src/utils.js",
        "Main.java",
        "src/com/example/App.java",
        "tests/AppTest.java",
        "main.go",
        "src/handler.go",
        "pkg/utils.go",
        "package.json",
        "config.yaml",
        "Cargo.toml",
        "requirements.txt",
        "README.md",
        "docs/guide.md",
        ".gitignore",
        "node_modules/react/index.js",
        ".git/config",
    ];

    group.bench_function("batch_real_world", |b| {
        let processor = OptimizedBatchRegexProcessor::new(file_patterns.clone()).unwrap();
        b.iter(|| {
            let mut total_matches = 0;
            for file in &realistic_files {
                total_matches += processor.match_all(file).len();
            }
            black_box(total_matches);
        });
    });

    group.bench_function("individual_real_world", |b| {
        let mut processor = RegexProcessor::new(RegexConfig::default());
        b.iter(|| {
            let mut total_matches = 0;
            for file in &realistic_files {
                for (_, pattern) in &file_patterns {
                    if processor.is_match(pattern, file).unwrap_or(false) {
                        total_matches += 1;
                    }
                }
            }
            black_box(total_matches);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_individual_processing,
    benchmark_batch_processing,
    benchmark_pattern_compilation,
    benchmark_small_vs_large_pattern_sets,
    benchmark_real_world_scenario
);
criterion_main!(benches);
