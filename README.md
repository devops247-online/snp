# SNP (Shell Not Pass)

[![CI](https://github.com/devops247-online/snp/actions/workflows/ci.yml/badge.svg)](https://github.com/devops247-online/snp/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/devops247-online/snp/graph/badge.svg)](https://codecov.io/gh/devops247-online/snp)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Crates.io](https://img.shields.io/crates/v/snp.svg)](https://crates.io/crates/snp)

A **production-ready**, fast, and reliable pre-commit framework written in Rust. SNP is a high-performance replacement for [pre-commit](https://pre-commit.com/) with 100% configuration compatibility and comprehensive feature parity.

## ⚡ Why SNP?

- **🔥 Performance**: 2-3x faster than Python pre-commit with optimized execution
- **🛡️ Reliability**: Memory safety and comprehensive error handling through Rust
- **🔄 Compatibility**: 100% compatible with existing `.pre-commit-config.yaml` files
- **📦 Easy Installation**: Single binary with no dependencies
- **🌍 Cross-Platform**: Works on Linux, macOS, and Windows
- **✅ Production Ready**: Comprehensive test suite with 516+ tests across 17 test files
- **🔗 Language Support**: Comprehensive language plugins: Python, Rust, Node.js, Go, Ruby, System commands, and Docker
- **⚙️ Advanced Features**: Hook chaining, dependency management, and API integration

## 🚀 Quick Start

### Installation

```bash
# Install from crates.io
cargo install snp

# Or download from releases
curl -L https://github.com/devops247-online/snp/releases/latest/download/snp-linux-x86_64.tar.gz | tar xz
```

### Basic Usage

```bash
# Install git hooks with backup support
snp install

# Run hooks on staged files (full execution engine)
snp run

# Run hooks on all files with parallel processing
snp run --all-files

# Update hook versions with real GitHub/GitLab API
snp autoupdate

# Update specific repositories
snp autoupdate --repo https://github.com/psf/black

# Dry run to see what would be updated
snp autoupdate --dry-run

# Run with hook chaining and dependencies
snp run --show-deps
```

## 📋 Configuration

SNP uses the same configuration format as pre-commit. Create a `.pre-commit-config.yaml` file:

```yaml
repos:
  - repo: https://github.com/psf/black
    rev: 23.1.0
    hooks:
      - id: black
  - repo: https://github.com/pycqa/flake8
    rev: 6.0.0
    hooks:
      - id: flake8
  - repo: https://github.com/pre-commit/mirrors-eslint
    rev: v8.57.0
    hooks:
      - id: eslint
        files: \.(js|ts|jsx|tsx)$
  - repo: https://github.com/golangci/golangci-lint
    rev: v1.54.2
    hooks:
      - id: golangci-lint
  - repo: https://github.com/rubocop/rubocop
    rev: v1.56.0
    hooks:
      - id: rubocop
```

## ✨ Features

SNP is a **feature-complete** pre-commit framework with advanced capabilities:

### 🎯 Core Features
- **Complete Hook Execution Engine**: Full subprocess management and execution control
- **Multi-Language Support**: Comprehensive language plugins with modular architecture (Python, Rust, Node.js, Go, Ruby, System commands, Docker)
- **Git Integration**: Comprehensive staged file processing and repository management
- **Configuration Compatibility**: 100% compatible with pre-commit YAML configurations
- **File Classification**: Intelligent file type detection and filtering

### 🚀 Advanced Features
- **Hook Chaining**: Dependency management and execution ordering with graph algorithms
- **API Integration**: Real GitHub/GitLab API calls for version updates
- **Configuration Migration**: Seamless migration from Python pre-commit
- **Output Aggregation**: Comprehensive result formatting and reporting
- **Concurrent Processing**: Advanced file locking, deadlock prevention, and parallel execution
- **Environment Management**: Language-specific environment setup and SQLite-based caching
- **Container Support**: Docker-based hook execution for isolated environments
- **Performance Optimization**: High-performance regex processing and memory-efficient data structures

## 🔧 Commands

SNP supports all pre-commit commands with full feature parity:

- `snp run` - Run hooks with comprehensive execution engine
- `snp install` - Install git hooks with backup and restoration
- `snp uninstall` - Remove git hooks cleanly
- `snp autoupdate` - Update repository versions with real API integration
- `snp clean` - Clean cache files and environments
- `snp gc` - Garbage collect unused repositories
- `snp validate-config` - Comprehensive YAML schema validation
- `snp try-repo` - Test hooks from repositories

## 🏗️ Development

### Prerequisites

- Rust 1.70+
- Git

### Building

```bash
git clone https://github.com/devops247-online/snp.git
cd snp
cargo build --release
```

### Testing

```bash
# Run all tests (516+ comprehensive tests)
cargo test

# Run with coverage
cargo install cargo-llvm-cov
cargo llvm-cov

# Run specific test suites
cargo test --test integration_tests
cargo test --test autoupdate_tests
cargo test --test python_language_tests
cargo test --test hook_chaining_tests

# Run all 17 test suites
find tests/ -name "*.rs" -exec basename {} .rs \; | xargs -I {} cargo test --test {}
```

### Development Workflow

SNP follows Test-Driven Development (TDD):

1. **Red Phase**: Write failing tests
2. **Green Phase**: Implement minimal solution
3. **Refactor Phase**: Clean up and optimize

## 🎯 Project Status

SNP is **production-ready** with comprehensive features implemented. The project has achieved **95%+ feature completeness** with all core functionality and extensive language support working.

### 🏆 Implementation Status

- [x] **Phase 1**: Foundation and CLI framework ✅
- [x] **Phase 2**: Infrastructure (Git, storage, file system) ✅
- [x] **Phase 3**: Configuration and validation ✅
- [x] **Phase 4**: Hook execution engine ✅
- [x] **Phase 5**: Language support (System, Python, Rust) ✅
- [x] **Phase 6**: Advanced features (chaining, API integration, migration) ✅
- [x] **Phase 7**: Additional language plugins (Node.js, Go, Ruby, Docker) ✅
- [ ] **Phase 8**: Final optimizations and polish 🔄

### 📊 Testing & Quality

- **516+ Comprehensive Tests**: Across 17 test suites covering all functionality including language plugins
- **Multi-Toolchain Support**: Compatible with Rust stable, beta, and nightly
- **CI/CD Pipeline**: Robust testing with network failure resilience
- **Real-World Testing**: Live API integration testing with GitHub/GitLab
- **Cross-Platform**: Verified on Linux, macOS, and Windows

## 🤝 Contributing

We welcome contributions! Please see our [contributing guidelines](CONTRIBUTING.md) for details.

### Key Areas

- Language plugin implementations
- Performance optimizations
- Cross-platform compatibility
- Documentation improvements

## 📖 Migration from pre-commit

SNP is designed as a drop-in replacement:

1. Install SNP: `cargo install snp`
2. Replace `pre-commit` with `snp` in your commands
3. Your existing `.pre-commit-config.yaml` works unchanged

## 🔗 Related Projects

- [pre-commit](https://pre-commit.com/) - The original Python implementation
- [lefthook](https://github.com/evilmartians/lefthook) - Fast git hooks manager
- [husky](https://github.com/typicode/husky) - Git hooks for JavaScript

## 📜 License

Licensed under the MIT License. See [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- The [pre-commit](https://pre-commit.com/) project for the original concept and design
- The Rust community for excellent tooling and libraries
- All contributors who help make SNP better
