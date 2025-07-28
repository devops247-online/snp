#!/bin/bash
# SNP Release Script
# Automates the complete release process

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Display usage information
usage() {
    cat << EOF
SNP Release Script

Usage: $0 <version> [options]

Options:
    --dry-run       Show what would be done without making changes
    --skip-tests    Skip running tests (not recommended)
    --skip-build    Skip release build verification
    --help          Show this help message

Examples:
    $0 1.0.0
    $0 1.2.3 --dry-run
    $0 2.0.0 --skip-tests

Release Process:
1. Validate version format
2. Check git repository state
3. Run comprehensive tests
4. Build release artifacts
5. Update version and changelog
6. Create git commit and tag
7. Build final release artifacts
8. Display next steps

EOF
}

# Parse command line arguments
parse_args() {
    VERSION=""
    DRY_RUN=false
    SKIP_TESTS=false
    SKIP_BUILD=false

    while [[ $# -gt 0 ]]; do
        case $1 in
            --dry-run)
                DRY_RUN=true
                shift
                ;;
            --skip-tests)
                SKIP_TESTS=true
                shift
                ;;
            --skip-build)
                SKIP_BUILD=true
                shift
                ;;
            --help|-h)
                usage
                exit 0
                ;;
            -*)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
            *)
                if [[ -z "$VERSION" ]]; then
                    VERSION="$1"
                else
                    log_error "Multiple versions specified"
                    usage
                    exit 1
                fi
                shift
                ;;
        esac
    done

    if [[ -z "$VERSION" ]]; then
        log_error "Version required"
        usage
        exit 1
    fi
}

# Validate version format
validate_version() {
    local version="$1"
    if [[ ! "$version" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)(-[a-zA-Z0-9.-]+)?$ ]]; then
        log_error "Invalid version format: $version (expected X.Y.Z or X.Y.Z-suffix)"
        exit 1
    fi
    log_info "Version format is valid: $version"
}

# Check git repository state
check_git_state() {
    log_info "Checking git repository state..."

    # Check if we're in a git repository
    if ! git rev-parse --git-dir >/dev/null 2>&1; then
        log_error "Not in a git repository"
        exit 1
    fi

    # Check if working directory is clean
    if ! git diff --quiet; then
        log_error "Working directory has uncommitted changes"
        git status --porcelain
        exit 1
    fi

    # Check if staged changes exist
    if ! git diff --cached --quiet; then
        log_error "Staged changes exist"
        git diff --cached --name-only
        exit 1
    fi

    # Check if on main branch
    local current_branch
    current_branch=$(git rev-parse --abbrev-ref HEAD)
    if [[ "$current_branch" != "main" ]]; then
        log_error "Not on main branch (currently on: $current_branch)"
        exit 1
    fi

    # Check if tag already exists
    if git tag -l | grep -q "^v$VERSION$"; then
        log_error "Tag v$VERSION already exists"
        exit 1
    fi

    log_success "Git repository state is clean"
}

# Run comprehensive tests
run_tests() {
    if [[ "$SKIP_TESTS" == true ]]; then
        log_warning "Skipping tests (--skip-tests specified)"
        return 0
    fi

    log_info "Running comprehensive test suite..."

    # Run all tests
    if ! cargo test --all-targets --all-features; then
        log_error "Tests failed"
        exit 1
    fi

    # Run clippy
    log_info "Running clippy checks..."
    if ! cargo clippy --all-targets --all-features -- -D warnings; then
        log_error "Clippy checks failed"
        exit 1
    fi

    # Check formatting
    log_info "Checking code formatting..."
    if ! cargo fmt --all -- --check; then
        log_error "Code formatting issues found. Run 'cargo fmt' to fix."
        exit 1
    fi

    log_success "All tests and checks passed"
}

# Build release artifacts
build_release() {
    if [[ "$SKIP_BUILD" == true ]]; then
        log_warning "Skipping release build (--skip-build specified)"
        return 0
    fi

    log_info "Building release artifacts..."

    # Clean previous builds
    cargo clean

    # Build release
    if ! cargo build --release; then
        log_error "Release build failed"
        exit 1
    fi

    # Verify binary works
    local binary_path="$PROJECT_ROOT/target/release/snp"
    if [[ ! -x "$binary_path" ]]; then
        log_error "Release binary not found or not executable: $binary_path"
        exit 1
    fi

    # Test binary
    if ! "$binary_path" --version >/dev/null; then
        log_error "Release binary fails to run"
        exit 1
    fi

    log_success "Release build completed successfully"
}

# Update version and changelog
update_version() {
    log_info "Updating version to $VERSION..."

    if [[ "$DRY_RUN" == true ]]; then
        log_info "[DRY-RUN] Would update Cargo.toml version to $VERSION"
        log_info "[DRY-RUN] Would update CHANGELOG.md with release entry"
        return 0
    fi

    # Update Cargo.toml
    sed -i "s/^version = \".*\"/version = \"$VERSION\"/" "$PROJECT_ROOT/Cargo.toml"

    # Update CHANGELOG.md
    local today
    today=$(date +%Y-%m-%d)

    # Check if CHANGELOG.md exists and has the expected format
    if [[ ! -f "$PROJECT_ROOT/CHANGELOG.md" ]]; then
        log_error "CHANGELOG.md not found"
        exit 1
    fi

    # Update changelog
    sed -i "s/## \\[Unreleased\\]/## [Unreleased]\\n\\n## [$VERSION] - $today/" "$PROJECT_ROOT/CHANGELOG.md"

    log_success "Version and changelog updated"
}

# Create git commit and tag
create_release_commit() {
    log_info "Creating release commit and tag..."

    if [[ "$DRY_RUN" == true ]]; then
        log_info "[DRY-RUN] Would create commit: 'Release $VERSION'"
        log_info "[DRY-RUN] Would create tag: 'v$VERSION'"
        return 0
    fi

    # Add all changes
    git add -A

    # Create commit
    git commit -m "Release $VERSION"

    # Create annotated tag
    git tag -a "v$VERSION" -m "Release $VERSION"

    log_success "Created commit and tag for version $VERSION"
}

# Trigger GitHub Actions release
trigger_github_release() {
    log_info "Triggering GitHub Actions release workflow..."

    if [[ "$DRY_RUN" == true ]]; then
        log_info "[DRY-RUN] Would push commit and tag to trigger GitHub Actions"
        return 0
    fi

    # Push the commit and tag
    log_info "Pushing commit and tag to origin..."
    git push origin main
    git push origin "v$VERSION"

    log_success "Pushed to GitHub - release workflow should start automatically"

    # Wait a moment and check if workflow started
    sleep 5
    if command -v gh >/dev/null 2>&1; then
        log_info "Checking GitHub Actions workflow status..."
        gh run list --workflow=release.yml --limit=1 || log_warning "Could not check workflow status"
    else
        log_info "Install GitHub CLI 'gh' to monitor workflow status"
    fi
}

# Display next steps
show_next_steps() {
    log_success "Release $VERSION prepared successfully!"

    echo
    if [[ "$DRY_RUN" == true ]]; then
        echo "This was a dry run. To perform the actual release:"
        echo "1. Run without --dry-run:"
        echo "   ./scripts/release.sh $VERSION"
        echo
        echo "2. Or manually push the changes:"
        echo "   git push origin main"
        echo "   git push origin v$VERSION"
    else
        echo "Release process completed! The following happened:"
        echo "1. ✓ Version updated in Cargo.toml"
        echo "2. ✓ Changelog updated with release notes"
        echo "3. ✓ Git commit and tag created"
        echo "4. ✓ Changes pushed to GitHub"
        echo "5. ✓ GitHub Actions release workflow triggered"
        echo
        echo "Next steps:"
        echo "1. Monitor the GitHub Actions workflow:"
        echo "   https://github.com/devops247-online/snp/actions"
        echo
        echo "2. The workflow will automatically:"
        echo "   - Build cross-platform binaries"
        echo "   - Create GitHub release with assets"
        echo "   - Publish to crates.io (if configured)"
        echo
        echo "3. Once complete, the release will be available at:"
        echo "   https://github.com/devops247-online/snp/releases/tag/v$VERSION"
    fi

    echo
    echo "Installation commands for users:"
    echo "# Unix (Linux/macOS)"
    echo "curl -fsSL https://raw.githubusercontent.com/devops247-online/snp/main/scripts/install/install.sh | sh"
    echo
    echo "# Windows PowerShell"
    echo "iwr -useb https://raw.githubusercontent.com/devops247-online/snp/main/scripts/install/install.ps1 | iex"
    echo
    echo "# Rust users"
    echo "cargo install snp"
}

# Main release process
main() {
    cd "$PROJECT_ROOT"

    log_info "Starting release process for SNP"
    log_info "Version: $VERSION"
    log_info "Dry run: $DRY_RUN"
    echo

    # 1. Validate version
    validate_version "$VERSION"

    # 2. Check git state
    check_git_state

    # 3. Run tests
    run_tests

    # 4. Build release
    build_release

    # 5. Update version
    update_version

    # 6. Create commit and tag
    create_release_commit

    # 7. Trigger GitHub Actions release
    trigger_github_release

    # 8. Build final artifacts locally (if not dry run, for verification)
    if [[ "$DRY_RUN" == false ]]; then
        log_info "Building final release artifacts locally for verification..."
        build_release
    fi

    # 9. Show next steps
    show_next_steps
}

# Parse arguments and run main
parse_args "$@"
main
