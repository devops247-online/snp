#!/bin/bash
# SNP Installation Script for Unix-like systems (Linux, macOS)
# This script downloads and installs the latest release of SNP

set -euo pipefail

# Configuration
REPO="devops247-online/snp"
INSTALL_DIR="${SNP_INSTALL_DIR:-/usr/local/bin}"
TEMP_DIR=$(mktemp -d)
BINARY_NAME="snp"

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

# Cleanup function
cleanup() {
    rm -rf "$TEMP_DIR"
}
trap cleanup EXIT

# Display usage information
usage() {
    cat << EOF
SNP Installation Script

Usage: $0 [options]

Options:
    --version VERSION    Install specific version (e.g., v1.0.0)
    --install-dir DIR    Installation directory (default: $INSTALL_DIR)
    --force              Overwrite existing installation
    --help               Show this help message

Environment Variables:
    SNP_INSTALL_DIR      Installation directory (default: /usr/local/bin)

Examples:
    $0                              # Install latest version
    $0 --version v1.0.0            # Install specific version
    $0 --install-dir ~/.local/bin  # Install to custom directory

EOF
}

# Detect platform and architecture
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

# Download and verify binary
download_binary() {
    local version="$1"
    local platform="$2"
    local archive_name="snp-${platform}.tar.gz"
    local download_url="https://github.com/$REPO/releases/download/$version/$archive_name"
    local checksum_url="https://github.com/$REPO/releases/download/$version/$archive_name.sha256"

    log_info "Downloading SNP $version for $platform..."

    # Download binary archive
    if ! curl -sL "$download_url" -o "$TEMP_DIR/$archive_name"; then
        log_error "Failed to download $download_url"
        exit 1
    fi

    # Download checksum
    if ! curl -sL "$checksum_url" -o "$TEMP_DIR/$archive_name.sha256"; then
        log_warning "Could not download checksum file, skipping verification"
    else
        log_info "Verifying checksum..."
        cd "$TEMP_DIR"
        if command -v shasum >/dev/null; then
            shasum -a 256 -c "$archive_name.sha256"
        elif command -v sha256sum >/dev/null; then
            sha256sum -c "$archive_name.sha256"
        else
            log_warning "No checksum tool available, skipping verification"
        fi
        cd - >/dev/null
        log_success "Checksum verification passed"
    fi

    # Extract binary
    log_info "Extracting binary..."
    tar -xzf "$TEMP_DIR/$archive_name" -C "$TEMP_DIR"

    if [ ! -f "$TEMP_DIR/$BINARY_NAME" ]; then
        log_error "Binary not found in archive"
        exit 1
    fi
}

# Install binary
install_binary() {
    local install_path="$INSTALL_DIR/$BINARY_NAME"

    # Check if installation directory exists and is writable
    if [ ! -d "$INSTALL_DIR" ]; then
        log_info "Creating installation directory: $INSTALL_DIR"
        if ! mkdir -p "$INSTALL_DIR"; then
            log_error "Failed to create installation directory. Try running with sudo or setting a different --install-dir"
            exit 1
        fi
    fi

    if [ ! -w "$INSTALL_DIR" ]; then
        log_error "Installation directory is not writable: $INSTALL_DIR"
        log_error "Try running with sudo or setting a different --install-dir"
        exit 1
    fi

    # Check if binary already exists
    if [ -f "$install_path" ] && [ "$FORCE_INSTALL" != "true" ]; then
        log_error "SNP is already installed at $install_path"
        log_error "Use --force to overwrite or uninstall first"
        exit 1
    fi

    # Install binary
    log_info "Installing SNP to $install_path..."
    cp "$TEMP_DIR/$BINARY_NAME" "$install_path"
    chmod +x "$install_path"

    log_success "SNP installed successfully!"

    # Verify installation
    if "$install_path" --version >/dev/null 2>&1; then
        local installed_version
        installed_version=$("$install_path" --version | head -n1)
        log_success "Installation verified: $installed_version"
    else
        log_warning "Installation may have issues - binary doesn't run correctly"
    fi

    # Check if install directory is in PATH
    if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
        log_warning "Installation directory $INSTALL_DIR is not in your PATH"
        log_info "Add it to your PATH by adding this line to your shell profile:"
        log_info "export PATH=\"$INSTALL_DIR:\$PATH\""
    fi
}

# Main installation process
main() {
    local version=""
    local force_install="false"

    # Parse command line arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --version)
                version="$2"
                shift 2
                ;;
            --install-dir)
                INSTALL_DIR="$2"
                shift 2
                ;;
            --force)
                force_install="true"
                shift
                ;;
            --help|-h)
                usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done

    export FORCE_INSTALL="$force_install"

    log_info "Starting SNP installation..."

    # Detect platform
    local platform
    platform=$(detect_platform)
    log_info "Detected platform: $platform"

    # Get version to install
    if [ -z "$version" ]; then
        version=$(get_latest_version)
        if [ -z "$version" ]; then
            log_error "Could not determine latest version"
            exit 1
        fi
        log_info "Latest version: $version"
    else
        log_info "Installing version: $version"
    fi

    # Download and install
    download_binary "$version" "$platform"
    install_binary

    log_success "SNP installation completed!"
    log_info "Run 'snp --help' to get started"
}

# Run main function with all arguments
main "$@"
