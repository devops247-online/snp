use clap::Parser;
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use snp::cli::Cli;

fn benchmark_cli_parsing(c: &mut Criterion) {
    c.bench_function("cli_parse_help", |b| {
        b.iter(|| {
            let result = Cli::try_parse_from(black_box(["snp", "--help"]));
            // Help parsing will error (expected), but we're measuring parse time
            let _ = black_box(result);
        })
    });

    c.bench_function("cli_parse_run_simple", |b| {
        b.iter(|| {
            let result = Cli::try_parse_from(black_box(["snp", "run"]));
            let _ = black_box(result);
        })
    });

    c.bench_function("cli_parse_run_complex", |b| {
        b.iter(|| {
            let result = Cli::try_parse_from(black_box([
                "snp",
                "run",
                "--all-files",
                "--verbose",
                "--show-diff-on-failure",
                "--files",
                "file1.py",
                "file2.rs",
                "--hook-stage",
                "pre-push",
            ]));
            let _ = black_box(result);
        })
    });

    c.bench_function("cli_parse_install_complex", |b| {
        b.iter(|| {
            let result = Cli::try_parse_from(black_box([
                "snp",
                "install",
                "--hook-type",
                "pre-commit",
                "--hook-type",
                "pre-push",
                "--overwrite",
                "--install-hooks",
                "--allow-missing-config",
            ]));
            let _ = black_box(result);
        })
    });

    c.bench_function("cli_parse_autoupdate_complex", |b| {
        b.iter(|| {
            let result = Cli::try_parse_from(black_box([
                "snp",
                "autoupdate",
                "--bleeding-edge",
                "--freeze",
                "--repo",
                "repo1",
                "--repo",
                "repo2",
                "--jobs",
                "4",
            ]));
            let _ = black_box(result);
        })
    });

    c.bench_function("cli_parse_all_commands", |b| {
        let commands = [
            ["snp", "run"],
            ["snp", "install"],
            ["snp", "uninstall"],
            ["snp", "autoupdate"],
            ["snp", "clean"],
            ["snp", "gc"],
            ["snp", "install-hooks"],
            ["snp", "validate-config"],
            ["snp", "sample-config"],
        ];

        b.iter(|| {
            for cmd in &commands {
                let result = Cli::try_parse_from(black_box(cmd));
                let _ = black_box(result);
            }
        })
    });
}

criterion_group!(benches, benchmark_cli_parsing);
criterion_main!(benches);
