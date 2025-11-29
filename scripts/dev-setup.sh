#!/bin/bash
#
# Development Environment Setup Script for pi-cam-capture
#
# Installs required packages for V4L2 development and testing.
#
# Supported platforms:
#   - Ubuntu
#   - Debian
#   - Raspberry Pi OS
#

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() { echo -e "${BLUE}[INFO]${NC} $1"; }
success() { echo -e "${GREEN}[OK]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Check if running as root or with sudo available
check_sudo() {
    if [[ $EUID -ne 0 ]]; then
        if ! command -v sudo &> /dev/null; then
            error "This script requires sudo privileges"
            exit 1
        fi
        SUDO="sudo"
    else
        SUDO=""
    fi
}

# Detect the Linux distribution
detect_distro() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        echo "$ID"
    else
        echo "unknown"
    fi
}

# Install required packages
install_deps() {
    local distro=$(detect_distro)
    info "Detected distribution: $distro"
    info "Kernel version: $(uname -r)"

    case $distro in
        ubuntu|debian|raspbian)
            info "Installing packages..."
            $SUDO apt-get update
            $SUDO apt-get install -y v4l-utils ffmpeg

            # Install linux-modules-extra for vivid (Ubuntu only, not available on Raspberry Pi OS)
            local kernel_version=$(uname -r)
            if apt-cache show "linux-modules-extra-${kernel_version}" &>/dev/null; then
                info "Installing linux-modules-extra-${kernel_version}..."
                $SUDO apt-get install -y "linux-modules-extra-${kernel_version}"
            else
                warn "linux-modules-extra-${kernel_version} not available"
                warn "vivid module may not be available (normal on Raspberry Pi OS)"
            fi

            # Install v4l2loopback
            info "Installing v4l2loopback-dkms..."
            $SUDO apt-get install -y v4l2loopback-dkms || warn "v4l2loopback-dkms installation failed"
            ;;

        *)
            error "Unsupported distribution: $distro"
            error "This script only supports Ubuntu, Debian, and Raspberry Pi OS"
            exit 1
            ;;
    esac
}

# Verify installation
verify_install() {
    local ok=true

    echo ""
    info "Verifying installation..."
    echo ""

    # Check commands
    for cmd in v4l2-ctl ffmpeg; do
        if command -v "$cmd" &>/dev/null; then
            success "$cmd installed"
        else
            error "$cmd not found"
            ok=false
        fi
    done

    # Check kernel modules
    for mod in vivid v4l2loopback; do
        if modinfo "$mod" &>/dev/null; then
            success "$mod module available"
        else
            warn "$mod module not available"
        fi
    done

    echo ""
    if [ "$ok" = true ]; then
        success "All required packages installed"
    else
        error "Some packages are missing"
        return 1
    fi
}

# Main
main() {
    case "${1:-install}" in
        install)
            check_sudo
            install_deps
            verify_install
            ;;
        verify)
            verify_install
            ;;
        *)
            echo "Usage: $0 [install|verify]"
            echo ""
            echo "Commands:"
            echo "  install   Install required packages (default)"
            echo "  verify    Verify installation"
            exit 1
            ;;
    esac
}

main "$@"
