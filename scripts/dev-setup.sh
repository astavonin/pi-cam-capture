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

# Load vivid module with recommended configuration
load_vivid() {
    info "Loading vivid module..."

    # Check if vivid module is available
    if ! modinfo vivid &>/dev/null; then
        error "vivid module not available"
        error "Install with: sudo apt-get install -y linux-modules-extra-\$(uname -r)"
        return 1
    fi

    # Check if already loaded
    if lsmod | grep -q "^vivid"; then
        warn "vivid module already loaded"
        info "Unload first with: $0 unload-vivid"
        return 1
    fi

    # Load vivid with recommended parameters:
    # - n_devs=2: Create 2 virtual devices
    # - node_types=0x1,0x1: Both are video capture devices
    # - input_types=0x81,0x81: Webcam (0x01) + HDMI (0x80) inputs
    info "Configuration: n_devs=2 node_types=0x1,0x1 input_types=0x81,0x81"
    $SUDO modprobe vivid n_devs=2 node_types=0x1,0x1 input_types=0x81,0x81

    if [ $? -eq 0 ]; then
        success "vivid module loaded successfully"
        echo ""
        info "Verifying device creation..."

        # Wait a moment for devices to appear
        sleep 1

        # List video devices
        if ls /dev/video* &>/dev/null; then
            success "Video devices created:"
            ls -la /dev/video* | while read -r line; do
                echo "  $line"
            done
        else
            error "No video devices found"
            return 1
        fi

        echo ""
        # List devices with v4l2-ctl
        if command -v v4l2-ctl &>/dev/null; then
            info "Device information:"
            $SUDO v4l2-ctl --list-devices
        fi

        # Configure test patterns on vivid devices
        configure_vivid_patterns
    else
        error "Failed to load vivid module"
        return 1
    fi
}

# Configure different test patterns on vivid devices
# Device 1: Horizontal Gradient (pattern 14)
# Device 2: Vertical Lines (pattern 16)
configure_vivid_patterns() {
    info "Configuring test patterns on vivid devices..."

    # Find vivid devices by checking sysfs
    local vivid_devices=()
    for dev in /sys/class/video4linux/video*; do
        if [ -f "$dev/name" ]; then
            local name=$(cat "$dev/name" 2>/dev/null)
            if echo "$name" | grep -qi "vivid"; then
                local devnum=$(basename "$dev")
                vivid_devices+=("/dev/$devnum")
            fi
        fi
    done

    if [ ${#vivid_devices[@]} -eq 0 ]; then
        warn "No vivid devices found in sysfs"
        return 1
    fi

    # Pattern assignments (from v4l2-ctl --list-ctrls-menus):
    # 20 = Gray Ramp (gradient)
    # 0  = 75% Colorbar
    local patterns=(20 0)
    local pattern_names=("Gray Ramp" "75% Colorbar")

    for i in "${!vivid_devices[@]}"; do
        local dev="${vivid_devices[$i]}"
        local pattern_idx=$((i % ${#patterns[@]}))
        local pattern="${patterns[$pattern_idx]}"
        local pattern_name="${pattern_names[$pattern_idx]}"

        # Set format to YUYV 640x480 for consistent testing
        if v4l2-ctl -d "$dev" --set-fmt-video=width=640,height=480,pixelformat=YUYV 2>/dev/null; then
            success "$dev: Set format to 640x480 YUYV"
        else
            warn "$dev: Could not set format (may not be a capture device)"
            continue
        fi

        # Set test pattern
        if v4l2-ctl -d "$dev" --set-ctrl=test_pattern="$pattern" 2>/dev/null; then
            success "$dev: Set test pattern to $pattern ($pattern_name)"
        else
            warn "$dev: Could not set test pattern"
        fi
    done

    echo ""
    info "Test pattern configuration complete"
    info "View with: ffplay -f v4l2 <device> or v4l2-ctl -d <device> --stream-mmap --stream-count=1 --stream-to=frame.raw"
}

# Unload vivid module
unload_vivid() {
    info "Unloading vivid module..."

    # Check if vivid is loaded
    if ! lsmod | grep -q "^vivid"; then
        warn "vivid module is not loaded"
        return 0
    fi

    $SUDO modprobe -r vivid

    if [ $? -eq 0 ]; then
        success "vivid module unloaded successfully"
    else
        error "Failed to unload vivid module"
        error "Check if devices are in use: lsof /dev/video*"
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
        load-vivid)
            check_sudo
            load_vivid
            ;;
        unload-vivid)
            check_sudo
            unload_vivid
            ;;
        *)
            echo "Usage: $0 [install|verify|load-vivid|unload-vivid]"
            echo ""
            echo "Commands:"
            echo "  install       Install required packages (default)"
            echo "  verify        Verify installation"
            echo "  load-vivid    Load vivid module with recommended configuration"
            echo "  unload-vivid  Unload vivid module"
            exit 1
            ;;
    esac
}

main "$@"
