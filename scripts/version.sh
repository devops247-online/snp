#!/bin/bash
# SNP Version Management Script
# Automates version bumping and release preparation

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
SNP Version Management Script

Usage: $0 <command> [options]

Commands:
    current                 Show current version
    bump <level>           Bump version (major|minor|patch)
    prepare <version>      Prepare release for specific version
    check                  Check if ready for release
    help                   Show this help message

Examples:
    $0 current
    $0 bump patch
    $0 bump minor
    $0 bump major
    $0 prepare 1.0.0
    $0 check

EOF
}

# Get current version from Cargo.toml
get_current_version() {
    grep '^version = ' "$PROJECT_ROOT/Cargo.toml" | sed 's/version = "\(.*\)"/\1/'
}

# Parse version into components
parse_version() {
    local version="$1"
    if [[ ! "$version" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
        log_error "Invalid version format: $version (expected X.Y.Z)"
        exit 1
    fi

    MAJOR=${BASH_REMATCH[1]}
    MINOR=${BASH_REMATCH[2]}
    PATCH=${BASH_REMATCH[3]}
}

# Bump version based on level
bump_version() {
    local level="$1"
    local current_version
    current_version=$(get_current_version)

    parse_version "$current_version"

    case "$level" in
        major)
            MAJOR=$((MAJOR + 1))
            MINOR=0
            PATCH=0
            ;;
        minor)
            MINOR=$((MINOR + 1))
            PATCH=0
            ;;
        patch)
            PATCH=$((PATCH + 1))
            ;;
        *)
            log_error "Invalid bump level: $level (use major|minor|patch)"
            exit 1
            ;;
    esac

    local new_version="$MAJOR.$MINOR.$PATCH"
    log_info "Bumping version from $current_version to $new_version"

    # Update Cargo.toml
    sed -i "s/^version = \".*\"/version = \"$new_version\"/" "$PROJECT_ROOT/Cargo.toml"

    log_success "Version updated to $new_version"
    echo "$new_version"
}

# Check if repository is ready for release
check_release_readiness() {
    local issues=0

    log_info "Checking release readiness..."

    # Check if working directory is clean
    if ! git diff --quiet; then
        log_warning "Working directory has uncommitted changes"
        issues=$((issues + 1))
    fi

    # Check if staged changes exist
    if ! git diff --cached --quiet; then
        log_warning "Staged changes exist"
        issues=$((issues + 1))
    fi

    # Check if on main branch
    local current_branch
    current_branch=$(git rev-parse --abbrev-ref HEAD)
    if [[ "$current_branch" != "main" ]]; then
        log_warning "Not on main branch (currently on: $current_branch)"
        issues=$((issues + 1))
    fi

    # Check if tests pass
    log_info "Running tests..."
    if ! cargo test --quiet; then
        log_error "Tests are failing"
        issues=$((issues + 1))
    fi

    # Check if code compiles in release mode
    log_info "Checking release build..."
    if ! cargo build --release --quiet; then
        log_error "Release build fails"
        issues=$((issues + 1))
    fi

    # Check if clippy passes
    log_info "Running clippy..."
    if ! cargo clippy --all-targets --all-features -- -D warnings &>/dev/null; then
        log_error "Clippy checks fail"
        issues=$((issues + 1))
    fi

    # Check if formatting is correct
    log_info "Checking code formatting..."
    if ! cargo fmt --all -- --check &>/dev/null; then
        log_error "Code formatting issues found"
        issues=$((issues + 1))
    fi

    if [[ $issues -eq 0 ]]; then
        log_success "Repository is ready for release!"
        return 0
    else
        log_error "Found $issues issue(s) that need to be resolved before release"
        return 1
    fi
}

# Prepare release
prepare_release() {
    local version="$1"

    log_info "Preparing release for version $version"

    # Validate version format
    parse_version "$version"

    # Check release readiness
    if ! check_release_readiness; then
        log_error "Repository is not ready for release"
        exit 1
    fi

    # Update version in Cargo.toml
    local current_version
    current_version=$(get_current_version)
    if [[ "$current_version" != "$version" ]]; then
        log_info "Updating version from $current_version to $version"
        sed -i "s/^version = \".*\"/version = \"$version\"/" "$PROJECT_ROOT/Cargo.toml"
    fi

    # Use cargo-release if available
    if command -v cargo-release &> /dev/null; then
        log_info "Using cargo-release for release preparation"
        cargo release "$version" --no-publish --no-push
    else
        log_warning "cargo-release not available, manual release preparation"

        # Update CHANGELOG.md
        local today
        today=$(date +%Y-%m-%d)
        sed -i "s/## \\[Unreleased\\]/## [Unreleased]\\n\\n## [$version] - $today/" "$PROJECT_ROOT/CHANGELOG.md"

        # Create commit and tag
        git add -A
        git commit -m "Release $version"
        git tag -a "v$version" -m "Release $version"

        log_info "Created commit and tag for version $version"
        log_warning "Remember to push with: git push origin main --tags"
    fi

    log_success "Release $version prepared successfully!"
}

# Main script logic
main() {
    cd "$PROJECT_ROOT"

    case "${1:-help}" in
        current)
            current_version=$(get_current_version)
            echo "Current version: $current_version"
            ;;
        bump)
            if [[ $# -lt 2 ]]; then
                log_error "Bump level required (major|minor|patch)"
                usage
                exit 1
            fi
            bump_version "$2"
            ;;
        prepare)
            if [[ $# -lt 2 ]]; then
                log_error "Version required for prepare command"
                usage
                exit 1
            fi
            prepare_release "$2"
            ;;
        check)
            check_release_readiness
            ;;
        help|--help|-h)
            usage
            ;;
        *)
            log_error "Unknown command: $1"
            usage
            exit 1
            ;;
    esac
}

# Run main function with all arguments
main "$@"
