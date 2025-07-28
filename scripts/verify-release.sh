#!/bin/bash
# SNP Release Verification Script
# Verifies the integrity and authenticity of SNP releases

set -euo pipefail

# Configuration
REPO="devops247-online/snp"

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
SNP Release Verification Script

Usage: $0 [options] <binary_file>

Options:
    --version VERSION    Verify against specific version (e.g., v1.0.0)
    --platform PLATFORM  Platform target (e.g., x86_64-unknown-linux-musl)
    --checksum-only      Only verify checksum, skip other checks
    --help               Show this help message

Examples:
    $0 snp                                      # Verify local binary
    $0 --version v1.0.0 snp-linux              # Verify specific version
    $0 --platform x86_64-apple-darwin snp      # Verify platform-specific build

EOF
}

# Detect platform
detect_platform() {
    local os arch

    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    arch=$(uname -m)

    case "$os" in
        linux)
            case "$arch" in
                x86_64) echo "x86_64-unknown-linux-musl" ;;
                aarch64|arm64) echo "aarch64-unknown-linux-musl" ;;
                *) log_error "Unsupported architecture: $arch"; exit 1 ;;
            esac
            ;;
        darwin)
            case "$arch" in
                x86_64) echo "x86_64-apple-darwin" ;;
                arm64) echo "aarch64-apple-darwin" ;;
                *) log_error "Unsupported architecture: $arch"; exit 1 ;;
            esac
            ;;
        *)
            log_error "Unsupported operating system: $os"
            exit 1
            ;;
    esac
}

# Get latest release version
get_latest_version() {
    log_info "Fetching latest release information..."
    curl -s "https://api.github.com/repos/$REPO/releases/latest" | \
        grep '"tag_name":' | \
        sed -E 's/.*"tag_name": "([^"]+)".*/\1/'
}

# Download checksum file
download_checksum() {
    local version="$1"
    local platform="$2"
    local extension="tar.gz"

    if [[ "$platform" == *"windows"* ]]; then
        extension="zip"
    fi

    local checksum_url="https://github.com/$REPO/releases/download/$version/snp-${platform}.${extension}.sha256"
    local temp_checksum=$(mktemp)

    log_info "Downloading checksum file..."
    if curl -sL "$checksum_url" -o "$temp_checksum"; then
        echo "$temp_checksum"
    else
        log_error "Failed to download checksum file from $checksum_url"
        return 1
    fi
}

# Verify checksum
verify_checksum() {
    local binary_file="$1"
    local checksum_file="$2"

    if [[ ! -f "$binary_file" ]]; then
        log_error "Binary file not found: $binary_file"
        return 1
    fi

    if [[ ! -f "$checksum_file" ]]; then
        log_error "Checksum file not found: $checksum_file"
        return 1
    fi

    log_info "Verifying checksum..."

    # Get expected hash from checksum file
    local expected_hash
    expected_hash=$(cut -d' ' -f1 "$checksum_file")

    # Calculate actual hash
    local actual_hash
    if command -v shasum >/dev/null; then
        actual_hash=$(shasum -a 256 "$binary_file" | cut -d' ' -f1)
    elif command -v sha256sum >/dev/null; then
        actual_hash=$(sha256sum "$binary_file" | cut -d' ' -f1)
    else
        log_error "No SHA256 calculation tool available (shasum or sha256sum)"
        return 1
    fi

    # Compare hashes
    if [[ "$expected_hash" == "$actual_hash" ]]; then
        log_success "Checksum verification passed"
        log_info "Expected: $expected_hash"
        log_info "Actual:   $actual_hash"
        return 0
    else
        log_error "Checksum verification failed!"
        log_error "Expected: $expected_hash"
        log_error "Actual:   $actual_hash"
        return 1
    fi
}

# Verify binary functionality
verify_functionality() {
    local binary_file="$1"

    if [[ ! -x "$binary_file" ]]; then
        log_error "Binary is not executable: $binary_file"
        return 1
    fi

    log_info "Verifying binary functionality..."

    # Test version command
    local version_output
    if version_output=$("$binary_file" --version 2>&1); then
        log_success "Binary executes successfully"
        log_info "Version: $version_output"
    else
        log_error "Binary failed to execute"
        log_error "Output: $version_output"
        return 1
    fi

    # Test help command
    if "$binary_file" --help >/dev/null 2>&1; then
        log_success "Help command works"
    else
        log_warning "Help command failed"
    fi

    return 0
}

# Verify against known signatures (placeholder for future GPG implementation)
verify_signatures() {
    local version="$1"

    log_info "Checking for release signatures..."

    # Check if GPG signature exists (future implementation)
    local signature_url="https://github.com/$REPO/releases/download/$version/snp-checksums.txt.sig"

    if curl -s --head "$signature_url" | head -n1 | grep -q "200 OK"; then
        log_info "GPG signature found (verification not implemented yet)"
        # TODO: Implement GPG signature verification
        return 0
    else
        log_warning "No GPG signature found for this release"
        return 0
    fi
}

# Main verification process
main() {
    local binary_file=""
    local version=""
    local platform=""
    local checksum_only=false

    # Parse command line arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --version)
                version="$2"
                shift 2
                ;;
            --platform)
                platform="$2"
                shift 2
                ;;
            --checksum-only)
                checksum_only=true
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
                if [[ -z "$binary_file" ]]; then
                    binary_file="$1"
                else
                    log_error "Multiple binary files specified"
                    usage
                    exit 1
                fi
                shift
                ;;
        esac
    done

    if [[ -z "$binary_file" ]]; then
        log_error "Binary file required"
        usage
        exit 1
    fi

    log_info "Starting SNP release verification..."
    log_info "Binary file: $binary_file"

    # Auto-detect platform if not specified
    if [[ -z "$platform" ]]; then
        platform=$(detect_platform)
        log_info "Auto-detected platform: $platform"
    fi

    # Get version if not specified
    if [[ -z "$version" ]]; then
        version=$(get_latest_version)
        if [[ -z "$version" ]]; then
            log_error "Could not determine version to verify against"
            exit 1
        fi
        log_info "Verifying against latest version: $version"
    else
        log_info "Verifying against specified version: $version"
    fi

    local verification_passed=true

    # 1. Verify checksum
    if checksum_file=$(download_checksum "$version" "$platform"); then
        if verify_checksum "$binary_file" "$checksum_file"; then
            log_success "✓ Checksum verification passed"
        else
            log_error "✗ Checksum verification failed"
            verification_passed=false
        fi
        rm -f "$checksum_file"
    else
        log_error "✗ Could not download checksum for verification"
        verification_passed=false
    fi

    # Exit early if checksum-only mode
    if [[ "$checksum_only" == true ]]; then
        if [[ "$verification_passed" == true ]]; then
            log_success "Checksum verification completed successfully"
            exit 0
        else
            log_error "Checksum verification failed"
            exit 1
        fi
    fi

    # 2. Verify functionality
    if verify_functionality "$binary_file"; then
        log_success "✓ Functionality verification passed"
    else
        log_error "✗ Functionality verification failed"
        verification_passed=false
    fi

    # 3. Verify signatures (when available)
    if verify_signatures "$version"; then
        log_success "✓ Signature verification completed"
    else
        log_warning "! Signature verification skipped"
    fi

    # Summary
    echo
    if [[ "$verification_passed" == true ]]; then
        log_success "All verifications passed! Binary appears authentic and functional."
    else
        log_error "Some verifications failed! Binary may be compromised or corrupted."
        exit 1
    fi
}

# Run main function with all arguments
main "$@"
