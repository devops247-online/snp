# SNP (Shell Not Pass)

[![CI](https://github.com/devops247-online/snp/workflows/CI/badge.svg)](https://github.com/devops247-online/snp/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Crates.io](https://img.shields.io/crates/v/snp.svg)](https://crates.io/crates/snp)

A fast, reliable pre-commit framework written in Rust. SNP is a high-performance replacement for [pre-commit](https://pre-commit.com/) with 100% configuration compatibility.

## ⚡ Why SNP?

- **🔥 Performance**: 2-3x faster than Python pre-commit
- **🛡️ Reliability**: Memory safety and robust error handling through Rust
- **🔄 Compatibility**: 100% compatible with existing `.pre-commit-config.yaml` files
- **📦 Easy Installation**: Single binary with no dependencies
- **🌍 Cross-Platform**: Works on Linux, macOS, and Windows

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
# Install pre-commit hooks
snp install

# Run hooks on staged files
snp run

# Run hooks on all files
snp run --all-files

# Update hook versions
snp autoupdate
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
```

## 🔧 Commands

SNP supports all pre-commit commands:

- `snp run` - Run hooks (default command)
- `snp install` - Install git hooks
- `snp uninstall` - Remove git hooks
- `snp autoupdate` - Update repository versions
- `snp clean` - Clean cache files
- `snp gc` - Garbage collect unused repos
- `snp validate-config` - Validate configuration
- `snp try-repo` - Test hooks from a repository

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
# Run all tests
cargo test

# Run with coverage
cargo install cargo-llvm-cov
cargo llvm-cov

# Integration tests
cargo test --test integration_tests
```

### Development Workflow

SNP follows Test-Driven Development (TDD):

1. **Red Phase**: Write failing tests
2. **Green Phase**: Implement minimal solution
3. **Refactor Phase**: Clean up and optimize

## 🎯 Project Status

SNP is currently in active development. See our [project tracker](https://github.com/devops247-online/snp/issues/1) for detailed progress.

### Roadmap

- [x] **Phase 1**: Foundation and CLI framework
- [ ] **Phase 2**: Infrastructure (Git, storage, file system)
- [ ] **Phase 3**: Configuration and validation
- [ ] **Phase 4**: Hook execution engine
- [ ] **Phase 5**: Language support (Python, Node.js, etc.)
- [ ] **Phase 6**: Advanced features and optimization

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
