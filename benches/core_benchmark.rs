use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use snp::core::{ExecutionContext, Hook, Repository, Stage};
use std::path::PathBuf;

fn benchmark_core_performance(c: &mut Criterion) {
    // Benchmark Stage conversion with large datasets
    c.bench_function("stage_conversion_large", |b| {
        let stage_names = vec![
            "pre-commit",
            "pre-push",
            "commit-msg",
            "post-commit",
            "pre-merge-commit",
            "prepare-commit-msg",
            "post-checkout",
            "post-merge",
            "pre-rebase",
            "post-rewrite",
            "manual",
        ];

        b.iter(|| {
            for _ in 0..1000 {
                for name in &stage_names {
                    let _ = black_box(Stage::from_str(black_box(name)));
                }
            }
        })
    });

    // Benchmark Hook creation with builder pattern
    c.bench_function("hook_creation_large", |b| {
        b.iter(|| {
            for i in 0..1000 {
                let hook = Hook::new(format!("hook-{i}"), format!("command-{i}"), "python")
                    .with_name(format!("Hook {i}"))
                    .with_files(r"\.(py|pyi)$".to_string())
                    .with_exclude(r"test_.*\.py$".to_string())
                    .with_args(vec!["--check".to_string(), format!("--config={i}")])
                    .with_stages(vec![Stage::PreCommit, Stage::PrePush])
                    .always_run(false)
                    .fail_fast(true);

                black_box(hook);
            }
        })
    });

    // Benchmark Hook validation on large dataset
    c.bench_function("hook_validation_large", |b| {
        let hooks: Vec<Hook> = (0..1000)
            .map(|i| {
                Hook::new(format!("hook-{i}"), format!("command-{i}"), "python")
                    .with_files(r"\.(py|rs|js)$".to_string())
                    .with_exclude(r"(test_|__pycache__|\.git/)".to_string())
            })
            .collect();

        b.iter(|| {
            for hook in &hooks {
                let _ = black_box(hook.validate());
            }
        })
    });

    // Benchmark Repository creation and validation
    c.bench_function("repository_creation_large", |b| {
        b.iter(|| {
            for i in 0..1000 {
                let remote = Repository::remote(
                    format!("https://github.com/org/repo-{i}"),
                    Some(format!("v1.{i}.0")),
                );
                let local = Repository::local(format!("/path/to/repo-{i}"));
                let meta = Repository::meta();

                black_box((remote, local, meta));
            }
        })
    });

    // Benchmark ExecutionContext file filtering
    let mut group = c.benchmark_group("file_filtering");
    for size in [100, 500, 1000, 5000].iter() {
        group.bench_with_input(BenchmarkId::new("filter_files", size), size, |b, &size| {
            let hook = Hook::new("test", "test", "python")
                .with_files(r"\.(py|rs)$".to_string())
                .with_exclude(r"test_.*".to_string());

            let files: Vec<PathBuf> = (0..size)
                .map(|i| {
                    let extension = if i % 3 == 0 {
                        "py"
                    } else if i % 3 == 1 {
                        "rs"
                    } else {
                        "js"
                    };
                    let prefix = if i % 10 == 0 { "test_" } else { "" };
                    PathBuf::from(format!("{prefix}file{i}.{extension}"))
                })
                .collect();

            let ctx = ExecutionContext::new(Stage::PreCommit).with_files(files);

            b.iter(|| {
                let filtered = ctx.filtered_files(black_box(&hook));
                black_box(filtered)
            })
        });
    }
    group.finish();

    // Benchmark serialization performance
    c.bench_function("serialization_large", |b| {
        let hooks: Vec<Hook> = (0..100)
            .map(|i| {
                Hook::new(format!("hook-{i}"), format!("command-{i}"), "python")
                    .with_name(format!("Hook {i}"))
                    .with_files(r"\.(py|pyi)$".to_string())
                    .with_args(vec![format!("--arg-{i}"), "--verbose".to_string()])
            })
            .collect();

        b.iter(|| {
            for hook in &hooks {
                let serialized = serde_json::to_string(black_box(hook)).unwrap();
                let _: Hook = serde_json::from_str(black_box(&serialized)).unwrap();
            }
        })
    });

    // Benchmark regex compilation in FileFilter
    c.bench_function("regex_compilation_large", |b| {
        let patterns = vec![
            r"\.(py|pyi)$",
            r"\.(rs|toml)$",
            r"\.(js|ts|jsx|tsx)$",
            r"test_.*\.(py|rs)$",
            r"(\.git/|__pycache__/|target/debug/)",
            r"^src/.*\.(py|rs|js)$",
            r".*\.(yaml|yml|json|toml)$",
            r"(migrations/|docs/|\.venv/)",
        ];

        b.iter(|| {
            for _ in 0..100 {
                for pattern in &patterns {
                    let hook = Hook::new("test", "test", "system").with_files(pattern.to_string());
                    let _ = black_box(hook.file_filter());
                }
            }
        })
    });

    // Benchmark complex ExecutionContext operations
    c.bench_function("execution_context_operations", |b| {
        b.iter(|| {
            for i in 0..100 {
                let mut ctx = ExecutionContext::new(Stage::PreCommit)
                    .with_files(
                        (0..50)
                            .map(|j| PathBuf::from(format!("file{j}.py")))
                            .collect(),
                    )
                    .with_verbose(true)
                    .with_show_diff(true);

                // Add environment variables
                for j in 0..10 {
                    ctx.add_env(format!("VAR_{j}"), format!("value_{i}_{j}"));
                }

                let hook = Hook::new(format!("hook-{i}"), "test", "python")
                    .with_files(r"\.py$".to_string());

                let _filtered = ctx.filtered_files(&hook);
                black_box(ctx);
            }
        })
    });
}

criterion_group!(benches, benchmark_core_performance);
criterion_main!(benches);
