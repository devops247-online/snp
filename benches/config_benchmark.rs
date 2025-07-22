use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use snp::config::Config;

fn benchmark_config_parsing(c: &mut Criterion) {
    // Small config
    let small_config = r#"
repos:
- repo: https://github.com/psf/black
  rev: 23.1.0
  hooks:
  - id: black
    entry: black
    language: python
"#;

    // Medium config
    let medium_config = r#"
repos:
- repo: https://github.com/psf/black
  rev: 23.1.0
  hooks:
  - id: black
    entry: black --check --diff
    language: python
    types: [python]
    args: [--line-length=88]
    exclude: ^tests/
- repo: https://github.com/pycqa/flake8
  rev: 6.0.0
  hooks:
  - id: flake8
    entry: flake8
    language: python
  - id: flake8-docstrings
    entry: flake8
    language: python
    additional_dependencies: [flake8-docstrings]
- repo: https://github.com/pycqa/isort
  rev: 5.12.0
  hooks:
  - id: isort
    entry: isort
    language: python
    args: [--profile, black]
default_install_hook_types: [pre-commit, pre-push]
default_language_version:
  python: python3.9
  node: 18.16.0
files: ^src/
exclude: ^(tests/|docs/)
fail_fast: true
"#;

    // Large config with many repos and hooks
    let large_config = r#"
repos:
- repo: https://github.com/pre-commit/pre-commit-hooks
  rev: v4.4.0
  hooks:
  - id: trailing-whitespace
  - id: end-of-file-fixer
  - id: check-yaml
  - id: check-json
  - id: check-toml
  - id: check-xml
  - id: check-added-large-files
  - id: check-case-conflict
  - id: check-merge-conflict
  - id: check-symlinks
  - id: check-executables-have-shebangs
  - id: fix-byte-order-marker
  - id: mixed-line-ending

- repo: https://github.com/psf/black
  rev: 23.1.0
  hooks:
  - id: black
    entry: black --check --diff
    language: python
    types: [python]
    args: [--line-length=88, --target-version=py39]
    exclude: ^(migrations/|docs/)

- repo: https://github.com/pycqa/flake8
  rev: 6.0.0
  hooks:
  - id: flake8
    entry: flake8
    language: python
    types: [python]
    additional_dependencies:
      - flake8-docstrings
      - flake8-import-order
      - flake8-bugbear
      - flake8-comprehensions

- repo: https://github.com/pycqa/isort
  rev: 5.12.0
  hooks:
  - id: isort
    entry: isort
    language: python
    types: [python]
    args: [--profile, black, --line-length=88]

- repo: https://github.com/pycqa/bandit
  rev: 1.7.5
  hooks:
  - id: bandit
    entry: bandit
    language: python
    types: [python]
    args: [-r, ., -f, json]
    exclude: ^tests/

- repo: https://github.com/pre-commit/mirrors-mypy
  rev: v1.0.1
  hooks:
  - id: mypy
    entry: mypy
    language: python
    types: [python]
    additional_dependencies: [types-requests, types-PyYAML]

- repo: https://github.com/hadolint/hadolint
  rev: v2.12.0
  hooks:
  - id: hadolint
    entry: hadolint
    language: system
    types: [dockerfile]

- repo: https://github.com/shellcheck-py/shellcheck-py
  rev: v0.9.0.2
  hooks:
  - id: shellcheck
    entry: shellcheck
    language: python
    types: [shell]

- repo: https://github.com/adrienverge/yamllint
  rev: v1.29.0
  hooks:
  - id: yamllint
    entry: yamllint
    language: python
    types: [yaml]

- repo: local
  hooks:
  - id: pytest
    name: Run pytest
    entry: pytest
    language: system
    types: [python]
    pass_filenames: false
    always_run: false
  - id: coverage
    name: Check coverage
    entry: coverage
    language: system
    args: [report, --min-coverage=90]
    pass_filenames: false

default_install_hook_types: [pre-commit, pre-push, commit-msg]
default_language_version:
  python: python3.9
  node: 18.16.0
  rust: 1.70.0
default_stages: [commit, push]
files: ^(src/|tests/|scripts/)
exclude: ^(migrations/|docs/|\.venv/)
fail_fast: true
minimum_pre_commit_version: 3.0.0
ci:
  autofix_commit_msg: 'style: auto fixes from pre-commit.ci'
  autofix_prs: true
  autoupdate_schedule: weekly
  skip: [mypy, pytest]
"#;

    let configs = [
        ("small", small_config),
        ("medium", medium_config),
        ("large", large_config),
    ];

    // Benchmark parsing different config sizes
    let mut group = c.benchmark_group("config_parsing");
    for (name, config) in configs.iter() {
        group.bench_with_input(BenchmarkId::new("parse_yaml", name), config, |b, config| {
            b.iter(|| {
                let result = Config::from_yaml(black_box(config));
                black_box(result)
            })
        });
    }
    group.finish();

    // Benchmark repeated parsing of the same config
    c.bench_function("parse_medium_config_repeated", |b| {
        b.iter(|| {
            let result = Config::from_yaml(black_box(medium_config));
            black_box(result)
        })
    });

    // Benchmark error handling performance
    let invalid_configs = vec![
        "repos: [unclosed",
        "repos:\n  - invalid: :",
        "invalid: yaml: structure: bad",
    ];

    c.bench_function("parse_invalid_configs", |b| {
        b.iter(|| {
            for config in &invalid_configs {
                let result = Config::from_yaml(black_box(config));
                let _ = black_box(result);
            }
        })
    });

    // Benchmark file parsing (create temporary file)
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut temp_file = NamedTempFile::new().unwrap();
    write!(temp_file, "{large_config}").unwrap();
    let file_path = temp_file.path().to_path_buf();

    c.bench_function("parse_config_from_file", |b| {
        b.iter(|| {
            let result = Config::from_file(black_box(&file_path));
            black_box(result)
        })
    });
}

criterion_group!(benches, benchmark_config_parsing);
criterion_main!(benches);
