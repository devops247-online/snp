# SNP Release Guide

This document provides comprehensive guidelines for creating, managing, and distributing SNP releases.

## Overview

SNP uses a modern, automated release pipeline that builds cross-platform binaries, publishes to GitHub Releases, and deploys to crates.io using industry best practices for 2025.

## Release Types

### Regular Releases
- **Patch releases** (0.0.X): Bug fixes, security patches, minor improvements
- **Minor releases** (0.X.0): New features, language support, backward-compatible changes
- **Major releases** (X.0.0): Breaking changes, major architecture updates

### Pre-release Versions
- **Alpha** (X.Y.Z-alpha.N): Early development, unstable features
- **Beta** (X.Y.Z-beta.N): Feature complete, testing phase
- **Release Candidate** (X.Y.Z-rc.N): Final testing before stable release

## Release Process

### 1. Preparation

Before creating a release, ensure:

```bash
# Check release readiness
./scripts/version.sh check

# Run comprehensive tests
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

### 2. Version Management

#### Using Scripts (Recommended)

```bash
# Bump version automatically
./scripts/version.sh bump patch    # 0.1.0 -> 0.1.1
./scripts/version.sh bump minor    # 0.1.0 -> 0.2.0
./scripts/version.sh bump major    # 0.1.0 -> 1.0.0

# Prepare specific version
./scripts/version.sh prepare 1.0.0
```

#### Manual Version Update

1. Update `version` in `Cargo.toml`
2. Update `CHANGELOG.md` with release notes
3. Commit changes: `git commit -m "Release X.Y.Z"`
4. Create tag: `git tag -a vX.Y.Z -m "Release X.Y.Z"`

### 3. Automated Release

#### Full Release Process

```bash
# Complete automated release
./scripts/release.sh 1.0.0

# Dry run first (recommended)
./scripts/release.sh 1.0.0 --dry-run
```

This script will:
1. ✅ Validate version format and repository state
2. ✅ Run comprehensive tests and checks
3. ✅ Update version and changelog
4. ✅ Create git commit and tag
5. ✅ Push to GitHub (triggers automated workflows)
6. ✅ Build and verify local artifacts

#### GitHub Actions Workflow

Pushing a version tag (e.g., `v1.0.0`) automatically triggers:

1. **Cross-platform builds** for:
   - Linux: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`
   - macOS: `x86_64-apple-darwin`, `aarch64-apple-darwin`
   - Windows: `x86_64-pc-windows-msvc`

2. **Release creation** with:
   - Automated changelog extraction
   - Binary archives (`.tar.gz` for Unix, `.zip` for Windows)
   - SHA256 checksums for all assets
   - Comprehensive checksums summary file

3. **crates.io publishing** using Trusted Publishing (OIDC-based, no tokens required)

### 4. Manual Workflow Trigger

You can also trigger releases manually via GitHub Actions:

1. Go to **Actions** → **Release** workflow
2. Click **Run workflow**
3. Enter the release tag (e.g., `v1.0.0`)
4. Click **Run workflow**

## Distribution Channels

### 1. GitHub Releases

Primary distribution method with cross-platform binaries:
- **URL**: `https://github.com/devops247-online/snp/releases`
- **Assets**: Platform-specific archives with checksums
- **Installation**: Via automated scripts

### 2. crates.io

Rust package registry for `cargo install`:
- **URL**: `https://crates.io/crates/snp`
- **Installation**: `cargo install snp`
- **Updates**: `cargo install snp --force`

### 3. Installation Scripts

#### Unix (Linux/macOS)
```bash
# Latest version
curl -fsSL https://raw.githubusercontent.com/devops247-online/snp/main/scripts/install/install.sh | sh

# Specific version
curl -fsSL https://raw.githubusercontent.com/devops247-online/snp/main/scripts/install/install.sh | sh -s -- --version v1.0.0

# Custom directory
curl -fsSL https://raw.githubusercontent.com/devops247-online/snp/main/scripts/install/install.sh | sh -s -- --install-dir ~/.local/bin
```

#### Windows PowerShell
```powershell
# Latest version
iwr -useb https://raw.githubusercontent.com/devops247-online/snp/main/scripts/install/install.ps1 | iex

# Specific version
iwr -useb https://raw.githubusercontent.com/devops247-online/snp/main/scripts/install/install.ps1 | iex; install.ps1 -Version v1.0.0

# Custom directory
iwr -useb https://raw.githubusercontent.com/devops247-online/snp/main/scripts/install/install.ps1 | iex; install.ps1 -InstallDir "C:\Tools\SNP"
```

## Security and Verification

### 1. Checksums

All release assets include SHA256 checksums:

```bash
# Download and verify manually
wget https://github.com/devops247-online/snp/releases/download/v1.0.0/snp-x86_64-unknown-linux-musl.tar.gz
wget https://github.com/devops247-online/snp/releases/download/v1.0.0/snp-x86_64-unknown-linux-musl.tar.gz.sha256

# Verify checksum
shasum -a 256 -c snp-x86_64-unknown-linux-musl.tar.gz.sha256
```

### 2. Automated Verification

Use the verification script:

```bash
# Verify downloaded binary
./scripts/verify-release.sh snp

# Verify against specific version
./scripts/verify-release.sh --version v1.0.0 snp

# Checksum only
./scripts/verify-release.sh --checksum-only snp
```

### 3. Future Security Features

Planned security enhancements:
- **GPG signatures** for release assets
- **Supply chain attestation** with SLSA framework
- **Binary provenance** verification
- **Reproducible builds** verification

## Troubleshooting

### Common Issues

#### 1. Release Workflow Fails

Check GitHub Actions logs:
1. Go to **Actions** tab in repository
2. Click on failed workflow run
3. Review job logs for errors

Common fixes:
- Ensure all tests pass locally
- Check for uncommitted changes
- Verify tag format matches `v*.*.*`

#### 2. crates.io Publishing Fails

Possible causes:
- **Trusted Publishing not configured**: Set up OIDC trust relationship
- **Version already exists**: Versions cannot be overwritten
- **Metadata issues**: Check `Cargo.toml` completeness

#### 3. Cross-compilation Issues

Platform-specific problems:
- **musl targets**: Ensure `cross` tool is properly configured
- **macOS Apple Silicon**: May require Xcode command line tools
- **Windows**: Check for proper MSVC toolchain

### Recovery Procedures

#### Failed Release Recovery

If a release fails partway through:

1. **Delete the failed tag**:
   ```bash
   git tag -d vX.Y.Z
   git push origin :refs/tags/vX.Y.Z
   ```

2. **Delete GitHub release** (if created)

3. **Fix the issue** and retry release process

#### Emergency Patch Release

For critical security fixes:

```bash
# Quick patch release
./scripts/version.sh bump patch
./scripts/release.sh $(./scripts/version.sh current) --skip-tests
```

## Monitoring and Analytics

### Release Metrics

Track release success:
- **GitHub Actions** workflow success rate
- **Download statistics** from GitHub Releases
- **crates.io download** statistics
- **User feedback** and issue reports

### Performance Monitoring

Monitor release artifacts:
- **Binary size** across platforms
- **Build times** for each target
- **Test execution** duration
- **Release frequency** and cadence

## Best Practices

### 1. Version Planning

- **Semantic versioning**: Follow SemVer strictly
- **Breaking changes**: Always increment major version
- **Deprecation**: Provide migration path for removed features
- **Communication**: Announce breaking changes in advance

### 2. Release Notes

- **Clear descriptions**: Explain changes in user terms
- **Migration guides**: Provide upgrade instructions
- **Security notices**: Highlight security-related changes
- **Breaking changes**: Document all incompatibilities

### 3. Testing Strategy

- **Pre-release testing**: Use alpha/beta versions for validation
- **Platform coverage**: Test on all supported platforms
- **Regression testing**: Ensure existing functionality works
- **Performance testing**: Verify no performance regressions

### 4. Rollback Strategy

- **Version pinning**: Allow users to pin specific versions
- **Previous version support**: Maintain previous major version
- **Quick fixes**: Ability to rapidly patch critical issues
- **Communication**: Clear rollback instructions

## Release Schedule

### Regular Schedule

- **Patch releases**: As needed (1-4 weeks for bug fixes)
- **Minor releases**: Monthly or bi-monthly
- **Major releases**: 6-12 months

### Emergency Releases

- **Security patches**: Immediate (within 24 hours)
- **Critical bugs**: 1-3 days
- **Hotfixes**: Same day for blocking issues

---

This release guide ensures consistent, reliable, and secure distribution of SNP across all supported platforms and channels.
