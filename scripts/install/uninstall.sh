#!/bin/bash
# SNP Uninstallation Script for Unix-like systems (Linux, macOS)
# This script removes SNP installation

set -euo pipefail

# Configuration
BINARY_NAME="snp"
DEFAULT_INSTALL_DIRS=("/usr/local/bin" "$HOME/.local/bin" "/opt/snp/bin")

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
SNP Uninstallation Script

Usage: $0 [options]

Options:
    --install-dir DIR    Specific installation directory to remove from
    --all                Remove from all standard locations
    --force              Remove without confirmation
    --help               Show this help message

Examples:
    $0                              # Interactive removal
    $0 --all                        # Remove from all standard locations
    $0 --install-dir /usr/local/bin # Remove from specific directory

EOF
}

# Find SNP installations
find_installations() {
    local dirs_to_check=("${DEFAULT_INSTALL_DIRS[@]}")
    local found_installations=()

    # Add PATH directories
    while IFS=: read -ra PATH_DIRS; do
        for dir in "${PATH_DIRS[@]}"; do
            if [[ -n "$dir" && "$dir" != "." ]]; then
                dirs_to_check+=("$dir")
            fi
        done
    done <<< "$PATH"

    # Remove duplicates and check for SNP binary
    for dir in $(printf "%s\n" "${dirs_to_check[@]}" | sort -u); do
        if [[ -d "$dir" && -f "$dir/$BINARY_NAME" ]]; then
            found_installations+=("$dir/$BINARY_NAME")
        fi
    done

    printf "%s\n" "${found_installations[@]}"
}

# Remove SNP from specific location
remove_installation() {
    local install_path="$1"
    local install_dir
    install_dir=$(dirname "$install_path")

    if [[ ! -f "$install_path" ]]; then
        log_error "SNP not found at $install_path"
        return 1
    fi

    # Check if we can write to the directory
    if [[ ! -w "$install_dir" ]]; then
        log_error "Cannot write to $install_dir. Try running with sudo."
        return 1
    fi

    # Get version info before removal
    local version_info=""
    if command -v "$install_path" >/dev/null 2>&1; then
        version_info=$("$install_path" --version 2>/dev/null || echo "unknown version")
    fi

    log_info "Removing SNP ($version_info) from $install_path..."

    if rm "$install_path"; then
        log_success "Successfully removed $install_path"
        return 0
    else
        log_error "Failed to remove $install_path"
        return 1
    fi
}

# Interactive removal
interactive_removal() {
    local installations
    readarray -t installations < <(find_installations)

    if [[ ${#installations[@]} -eq 0 ]]; then
        log_info "No SNP installations found"
        return 0
    fi

    log_info "Found SNP installations:"
    for i in "${!installations[@]}"; do
        local version_info=""
        if command -v "${installations[$i]}" >/dev/null 2>&1; then
            version_info=$(${installations[$i]} --version 2>/dev/null || echo "unknown version")
        fi
        echo "  $((i+1)). ${installations[$i]} ($version_info)"
    done

    echo
    read -p "Which installation would you like to remove? (1-${#installations[@]}, 'all', or 'cancel'): " choice

    case "$choice" in
        cancel|c|C)
            log_info "Uninstallation cancelled"
            return 0
            ;;
        all|a|A)
            log_info "Removing all installations..."
            local success_count=0
            for installation in "${installations[@]}"; do
                if remove_installation "$installation"; then
                    ((success_count++))
                fi
            done
            log_success "Removed $success_count out of ${#installations[@]} installations"
            ;;
        [1-9]*)
            if [[ "$choice" -ge 1 && "$choice" -le ${#installations[@]} ]]; then
                local selected_installation="${installations[$((choice-1))]}"
                remove_installation "$selected_installation"
            else
                log_error "Invalid choice: $choice"
                return 1
            fi
            ;;
        *)
            log_error "Invalid choice: $choice"
            return 1
            ;;
    esac
}

# Main uninstallation process
main() {
    local install_dir=""
    local remove_all=false
    local force_remove=false

    # Parse command line arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --install-dir)
                install_dir="$2"
                shift 2
                ;;
            --all)
                remove_all=true
                shift
                ;;
            --force)
                force_remove=true
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

    log_info "Starting SNP uninstallation..."

    if [[ -n "$install_dir" ]]; then
        # Remove from specific directory
        local install_path="$install_dir/$BINARY_NAME"
        if [[ "$force_remove" == true ]]; then
            remove_installation "$install_path"
        else
            read -p "Remove SNP from $install_path? (y/N): " confirm
            if [[ "$confirm" =~ ^[Yy] ]]; then
                remove_installation "$install_path"
            else
                log_info "Uninstallation cancelled"
            fi
        fi
    elif [[ "$remove_all" == true ]]; then
        # Remove all installations
        local installations
        readarray -t installations < <(find_installations)

        if [[ ${#installations[@]} -eq 0 ]]; then
            log_info "No SNP installations found"
            return 0
        fi

        if [[ "$force_remove" != true ]]; then
            log_info "Found ${#installations[@]} SNP installation(s):"
            for installation in "${installations[@]}"; do
                echo "  - $installation"
            done
            echo
            read -p "Remove all installations? (y/N): " confirm
            if [[ ! "$confirm" =~ ^[Yy] ]]; then
                log_info "Uninstallation cancelled"
                return 0
            fi
        fi

        local success_count=0
        for installation in "${installations[@]}"; do
            if remove_installation "$installation"; then
                ((success_count++))
            fi
        done
        log_success "Removed $success_count out of ${#installations[@]} installations"
    else
        # Interactive removal
        interactive_removal
    fi

    log_success "SNP uninstallation completed!"
}

# Run main function with all arguments
main "$@"
