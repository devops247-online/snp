// Arena allocation performance benchmarks
// Measures the performance improvement from arena-based memory management

use bumpalo::Bump;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use snp::core::{ArenaExecutionContext, ExecutionContext, Hook, Stage};
use std::collections::HashMap;
use std::path::PathBuf;

fn create_test_data(size: usize) -> (Vec<PathBuf>, HashMap<String, String>) {
    let files: Vec<PathBuf> = (0..size)
        .map(|i| PathBuf::from(format!("test_file_{i}.rs")))
        .collect();

    let mut environment = HashMap::new();
    for i in 0..size {
        environment.insert(format!("VAR_{i}"), format!("value_{i}"));
    }

    (files, environment)
}

fn create_test_hook() -> Hook {
    Hook::new("test_hook", "rustc", "rust")
        .with_args(vec!["--check".to_string(), "--edition=2021".to_string()])
        .with_stages(vec![Stage::PreCommit])
}

// Benchmark execution context creation
fn bench_execution_context_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution_context_creation");

    for size in [10, 100, 1000].iter() {
        let (files, environment) = create_test_data(*size);

        group.bench_with_input(BenchmarkId::new("regular_context", size), size, |b, _| {
            b.iter(|| {
                let files = black_box(files.clone());
                let environment = black_box(environment.clone());

                let _context = ExecutionContext {
                    files,
                    stage: Stage::PreCommit,
                    verbose: false,
                    show_diff_on_failure: false,
                    environment,
                    working_directory: PathBuf::from("/tmp"),
                    color: true,
                };
            })
        });

        group.bench_with_input(BenchmarkId::new("arena_context", size), size, |b, _| {
            b.iter(|| {
                let arena = Bump::new();
                let files = black_box(files.clone());
                let environment = black_box(environment.clone());

                let _context =
                    ArenaExecutionContext::new(&arena, Stage::PreCommit, files, environment);
            })
        });
    }

    group.finish();
}

// Benchmark hook command generation
fn bench_hook_command_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("hook_command_generation");

    let hook = create_test_hook();

    group.bench_function("regular_command", |b| {
        b.iter(|| {
            let _command = black_box(hook.command());
        })
    });

    group.bench_function("arena_command", |b| {
        b.iter(|| {
            let arena = Bump::new();
            let _command = black_box(hook.command_arena(&arena));
        })
    });

    // Benchmark repeated command generation (where arena should excel)
    group.bench_function("regular_command_repeated", |b| {
        b.iter(|| {
            for _ in 0..100 {
                let _command = black_box(hook.command());
            }
        })
    });

    group.bench_function("arena_command_repeated", |b| {
        b.iter(|| {
            let arena = Bump::new();
            for _ in 0..100 {
                let _command = black_box(hook.command_arena(&arena));
            }
        })
    });

    group.finish();
}

// Benchmark memory allocation patterns
fn bench_memory_allocation_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation_patterns");

    for size in [100, 1000, 10000].iter() {
        let (files, environment) = create_test_data(*size);
        let hook = create_test_hook();

        group.bench_with_input(
            BenchmarkId::new("regular_full_pipeline", size),
            size,
            |b, _| {
                b.iter(|| {
                    let files = black_box(files.clone());
                    let environment = black_box(environment.clone());

                    // Create context
                    let context = ExecutionContext {
                        files,
                        stage: Stage::PreCommit,
                        verbose: false,
                        show_diff_on_failure: false,
                        environment,
                        working_directory: PathBuf::from("/tmp"),
                        color: true,
                    };

                    // Generate command
                    let _command = hook.command();

                    // Filter files (simulate hook processing)
                    let _filtered = context.filtered_files(&hook).unwrap_or_default();
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("arena_full_pipeline", size),
            size,
            |b, _| {
                b.iter(|| {
                    let arena = Bump::new();
                    let files = black_box(files.clone());
                    let environment = black_box(environment.clone());

                    // Create arena context
                    let context =
                        ArenaExecutionContext::new(&arena, Stage::PreCommit, files, environment);

                    // Generate command with arena
                    let _command = hook.command_arena(&arena);

                    // Filter files with arena (simulate hook processing)
                    let _filtered = context.filtered_files(&hook).unwrap_or_default();
                })
            },
        );
    }

    group.finish();
}

// Benchmark memory usage and cleanup
fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");

    let (files, environment) = create_test_data(1000);

    group.bench_function("arena_memory_stats", |b| {
        b.iter(|| {
            let arena = Bump::new();
            let context = ArenaExecutionContext::new(
                &arena,
                Stage::PreCommit,
                files.clone(),
                environment.clone(),
            );
            let _stats = black_box(context.memory_stats());
        })
    });

    group.bench_function("arena_reset_and_reuse", |b| {
        let mut arena = Bump::new();

        b.iter(|| {
            arena.reset();
            let _context = ArenaExecutionContext::new(
                &arena,
                Stage::PreCommit,
                files.clone(),
                environment.clone(),
            );
        })
    });

    group.finish();
}

// Benchmark zero-copy hook command generation
fn bench_zero_copy_commands(c: &mut Criterion) {
    let mut group = c.benchmark_group("hook_command_generation_zero_copy");

    // Create hooks with varying number of arguments to test scalability
    let hooks: Vec<Hook> = vec![
        Hook::new("simple", "rustc", "rust"),
        Hook::new("few_args", "cargo", "rust")
            .with_args(vec!["build".to_string(), "--release".to_string()]),
        Hook::new("many_args", "rustfmt", "rust")
            .with_args(vec![
                "--check".to_string(),
                "--edition".to_string(),
                "2021".to_string(),
                "--config".to_string(),
                "hard_tabs=true".to_string(),
                "--emit".to_string(),
                "files".to_string(),
                "--color".to_string(),
                "always".to_string(),
            ]),
    ];

    for hook in &hooks {
        group.bench_with_input(
            BenchmarkId::new("command_traditional", &hook.id),
            hook,
            |b, hook| {
                b.iter(|| {
                    let _command = black_box(hook.command());
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("command_cow", &hook.id),
            hook,
            |b, hook| {
                b.iter(|| {
                    let _command = black_box(hook.command_cow());
                })
            },
        );
    }

    // Benchmark frequent command generation (hot path simulation)
    let hook = Hook::new("hot_path", "black", "python")
        .with_args(vec![
            "--check".to_string(),
            "--diff".to_string(),
            "--color".to_string(),
            "--line-length".to_string(),
            "88".to_string(),
        ]);

    group.bench_function("hot_path_traditional", |b| {
        b.iter(|| {
            for _ in 0..100 {
                let _command = black_box(hook.command());
            }
        })
    });

    group.bench_function("hot_path_cow", |b| {
        b.iter(|| {
            for _ in 0..100 {
                let _command = black_box(hook.command_cow());
            }
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_execution_context_creation,
    bench_hook_command_generation,
    bench_memory_allocation_patterns,
    bench_memory_usage,
    bench_zero_copy_commands
);
criterion_main!(benches);
