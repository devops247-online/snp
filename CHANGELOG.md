# Changelog

All notable changes to SNP (Shell Not Pass) will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.1] - 2025-07-31

### Added
- Advanced work-stealing task scheduler for optimal parallel execution with load balancing
- Multi-tier caching architecture (L1/L2/L3) with intelligent promotion/demotion algorithms
- Lock-free data structures for zero-contention concurrent access
- Bloom filter-based negative caching for fast miss detection and performance optimization
- Batch regex processing with compiled RegexSet optimization for improved pattern matching
- Arena-based memory management for hot execution paths to reduce allocations
- Zero-copy string operations in hook command generation
- Incremental file change detection to reduce I/O overhead
- Async-first file I/O with intelligent batching for improved throughput
- Resource pool pattern implementation for improved performance
- Comprehensive event-driven hook lifecycle management system

### Fixed
- Cache deadlock issues in multi-tier cache promotion logic
- Work stealing scheduler test failures and hanging issues
- Task timeout handling in scheduler execution
- Dependency resolution logic for proper task submission
- Work stealing metrics recording to prevent double-counting

### Performance
- 3-5x performance improvement over Python pre-commit through comprehensive optimizations
- Significantly reduced memory allocations through arena-based management
- Improved concurrent execution through lock-free data structures
- Enhanced cache efficiency with multi-tier architecture
- Optimized regex processing with batch compilation

### Testing
- Expanded test suite to 758+ comprehensive tests across 49 test files
- Added comprehensive work-stealing scheduler tests
- Added multi-tier cache system tests with deadlock prevention validation
- Enhanced performance and concurrency testing coverage

## [1.0.0] - 2025-07-27

## [1.0.0] - 2025-07-27

## [1.0.0] - 2025-07-27

### Added
- Comprehensive semantic versioning strategy
- CHANGELOG.md for tracking all changes
- Version management automation tools
- Enhanced version command with build information

## [0.1.0] - 2024-01-XX

### Added
- Complete pre-commit framework implementation
- Full compatibility with Python pre-commit configurations
- Multi-language support:
  - Python with virtual environment management
  - Node.js with npm/yarn package management
  - Rust with Cargo toolchain integration
  - Go with module support
  - Ruby with gem management
  - Docker container execution
  - System command execution
- Advanced features:
  - Hook chaining with dependency resolution
  - Parallel execution with concurrency controls
  - SQLite-based caching and storage
  - Comprehensive file locking with deadlock prevention
  - High-performance regex processing
- Command-line interface:
  - `snp run` - Execute hooks with comprehensive options
  - `snp install` - Install git hooks
  - `snp clean` - Clean hook installations and caches
  - `snp autoupdate` - Update repository versions with GitHub/GitLab API
  - `snp migrate-config` - Migrate from Python pre-commit
  - `snp try-repo` - Test repository configurations
- Production-ready features:
  - Comprehensive error handling and logging
  - Cross-platform support (Linux, macOS, Windows)
  - Performance optimizations with LTO and single codegen unit
  - Security-focused design with input validation
- Testing infrastructure:
  - 516+ comprehensive tests across 17 test suites
  - Integration tests for end-to-end workflows
  - Performance benchmarks for critical paths
  - Cross-platform compatibility testing

---

## Versioning Strategy

### Version Number Format
SNP follows [Semantic Versioning](https://semver.org/) (SemVer):
- **MAJOR** version for incompatible API changes
- **MINOR** version for backward-compatible functionality additions
- **PATCH** version for backward-compatible bug fixes

### What Constitutes Breaking Changes (MAJOR)
- Changes to CLI command interfaces or arguments
- Modifications to configuration file format (.pre-commit-config.yaml)
- Breaking changes to hook execution behavior
- Changes to language plugin APIs
- Removal of deprecated features

### What Constitutes New Features (MINOR)
- New language support additions
- New CLI commands or options (backward-compatible)
- Performance improvements
- New configuration options
- Enhanced API integrations

### What Constitutes Bug Fixes (PATCH)
- Bug fixes that don't change API
- Security patches
- Documentation improvements
- Performance optimizations that don't change behavior
- Dependency updates (non-breaking)

### Pre-release Versions
- Alpha: `X.Y.Z-alpha.N` - Early development, unstable
- Beta: `X.Y.Z-beta.N` - Feature complete, testing phase
- RC: `X.Y.Z-rc.N` - Release candidate, final testing

### Release Process
1. Update version in Cargo.toml
2. Update CHANGELOG.md with release notes
3. Create git tag with version number
4. Build and test release artifacts
5. Publish to package repositories
6. Create GitHub release with artifacts

### Version Support Policy
- **Current version (1.x.x)**: Full support with security updates and bug fixes
- **Previous major (0.x.x)**: Security updates only for 6 months after new major release
- **End-of-life**: No support after 6 months

---

## Types of Changes
- `Added` for new features
- `Changed` for changes in existing functionality
- `Deprecated` for soon-to-be removed features
- `Removed` for now removed features
- `Fixed` for any bug fixes
- `Security` for vulnerability fixes
