# SNP Versioning Strategy

This document outlines the versioning strategy for SNP (Shell Not Pass), a production-ready pre-commit framework written in Rust.

## Overview

SNP follows [Semantic Versioning](https://semver.org/) (SemVer) 2.0.0 to provide clear expectations about compatibility and changes between releases.

## Version Format

**MAJOR.MINOR.PATCH** (e.g., 1.2.3)

- **MAJOR**: Incompatible API changes
- **MINOR**: New functionality in a backward-compatible manner
- **PATCH**: Backward-compatible bug fixes

## Pre-release Versions

For pre-release versions, we append identifiers:
- **Alpha**: `X.Y.Z-alpha.N` - Early development, unstable, may have breaking changes
- **Beta**: `X.Y.Z-beta.N` - Feature complete, in testing phase, API should be stable
- **Release Candidate**: `X.Y.Z-rc.N` - Final testing before release

Examples: `1.0.0-alpha.1`, `1.0.0-beta.2`, `1.0.0-rc.1`

## What Constitutes Each Version Type

### Major Version (X.0.0)

Breaking changes that require user action:

- **CLI Interface Changes**
  - Removing or renaming commands
  - Changing command argument formats
  - Removing command-line options
  - Changing default behavior significantly

- **Configuration Format Changes**
  - Breaking changes to `.pre-commit-config.yaml` structure
  - Removing configuration options
  - Changing configuration option behavior incompatibly

- **Hook Execution Changes**
  - Changes to hook execution order or behavior
  - Breaking changes to hook interface
  - Changes to file processing behavior

- **Language Plugin API**
  - Breaking changes to language trait interfaces
  - Removing public APIs
  - Changing function signatures

- **Output Format Changes**
  - Breaking changes to JSON/structured output
  - Changes to exit codes
  - Significant changes to log format

### Minor Version (0.X.0)

New features and improvements that maintain backward compatibility:

- **New Features**
  - New CLI commands or subcommands
  - New command-line options (with sensible defaults)
  - New configuration options
  - New language support

- **Enhancements**
  - Performance improvements
  - Better error messages
  - Additional output formats
  - New integrations (GitHub, GitLab, etc.)

- **API Additions**
  - New public functions or traits
  - New optional parameters (with defaults)
  - Extended language plugin capabilities

### Patch Version (0.0.X)

Bug fixes and minor improvements:

- **Bug Fixes**
  - Fixing incorrect behavior
  - Security vulnerability fixes
  - Memory leak fixes
  - Race condition fixes

- **Minor Improvements**
  - Documentation improvements
  - Better error messages (without changing structure)
  - Performance optimizations (without behavior changes)
  - Dependency updates (non-breaking)

- **Internal Changes**
  - Refactoring without API changes
  - Code quality improvements
  - Test improvements

## Version Support Policy

### Current Release (1.x.x)
- **Full support**: New features, bug fixes, security updates
- **Duration**: Until next major release

### Previous Major Release (0.x.x)
- **Limited support**: Critical bug fixes and security updates only
- **Duration**: 6 months after new major release
- **End-of-life**: No support after 6 months

### Legacy Versions
- **No support**: Versions older than the previous major release
- **Recommendation**: Upgrade to supported versions

## Release Cycle

### Regular Releases
- **Patch releases**: As needed for bug fixes (typically 1-4 weeks)
- **Minor releases**: Monthly or bi-monthly for new features
- **Major releases**: 6-12 months for significant changes

### Emergency Releases
- **Security fixes**: Released immediately for critical vulnerabilities
- **Critical bugs**: Released within 1-3 days for blocking issues

## Release Process

### 1. Pre-release Preparation
```bash
# Check release readiness
./scripts/version.sh check

# Run comprehensive tests
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

### 2. Version Update
```bash
# Bump version (patch/minor/major)
./scripts/version.sh bump patch

# Or prepare specific version
./scripts/version.sh prepare 1.2.3
```

### 3. Release Creation
```bash
# Create full release
./scripts/release.sh 1.2.3

# Or dry-run first
./scripts/release.sh 1.2.3 --dry-run
```

### 4. Post-release
- Push to GitHub with tags
- Create GitHub release with artifacts
- Update documentation
- Announce release

## Version Numbering Guidelines

### Starting Point
- **v1.0.0**: First stable release with API stability guarantees
- **v0.x.x**: Pre-1.0 development versions (current: 0.1.0)

### Pre-1.0 Considerations
During the 0.x.x series:
- Minor version bumps (0.x.0) MAY include breaking changes
- Patch version bumps (0.x.y) should remain backward compatible
- Users should expect potential breaking changes in minor releases

### Post-1.0 Stability
Once we reach 1.0.0:
- Strict SemVer compliance
- Breaking changes ONLY in major versions
- Strong backward compatibility guarantees

## Migration Guidance

### For Breaking Changes (Major Versions)
- Provide migration guide in CHANGELOG.md
- Include deprecation warnings in previous minor releases when possible
- Document all breaking changes with examples
- Provide tooling for automated migration when feasible

### For Deprecations
- Mark features as deprecated in minor releases
- Remove deprecated features in next major release
- Provide at least one minor release cycle before removal
- Include migration path in deprecation warnings

## Git Tagging Strategy

### Tag Format
- Release tags: `v1.2.3`
- Pre-release tags: `v1.2.3-alpha.1`, `v1.2.3-beta.1`, `v1.2.3-rc.1`

### Tag Creation
```bash
# Annotated tags with release message
git tag -a v1.2.3 -m "Release 1.2.3"

# Include changelog summary in tag message for major releases
git tag -a v2.0.0 -m "Release 2.0.0

Major changes:
- New hook chaining system
- Improved performance
- Breaking: Updated CLI interface
"
```

## Version Verification

### Runtime Version Check
SNP includes build information in the version command:
```bash
snp --version
# SNP 1.2.3 (commit: abc123, built: 2024-01-15)
```

### Compatibility Verification
- Check minimum supported version requirements
- Validate configuration file compatibility
- Verify language plugin compatibility

## Communication Strategy

### Release Announcements
- GitHub releases with detailed changelog
- Social media announcements for major releases
- Documentation updates
- Community notifications

### Breaking Change Communication
- Advance notice in previous releases
- Detailed migration guides
- Example code and configuration updates
- Community discussion and feedback periods

## Tooling and Automation

### Scripts
- `scripts/version.sh`: Version management utilities
- `scripts/release.sh`: Automated release process

### CI/CD Integration
- Automated version validation
- Release artifact building
- Changelog generation
- Tag creation and pushing

### Dependencies
- `cargo-release`: Automated release management
- `semver`: Version parsing and comparison
- Comprehensive test suite validation

---

This versioning strategy ensures predictable, reliable releases while maintaining the flexibility to innovate and improve SNP continuously.
