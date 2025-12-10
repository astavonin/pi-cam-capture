#!/bin/bash
#
# Camera Diagnostic Information Collector
#
# Collects comprehensive diagnostic information about V4L2 camera devices
# for debugging and sharing. Captures system info, device details, kernel
# messages, and hardware configuration.
#
# Usage: ./dump-camera-info.sh [OPTIONS]
#
# Options:
#   --device <path>   Camera device to diagnose (default: /dev/video0)
#   --output <path>   Output file path (default: camera-info.txt)
#   --help            Show this help message
#
# Example:
#   ./dump-camera-info.sh --device /dev/video2 --output /tmp/camera-diag.txt
#

set -uo pipefail

# Colors for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info() { echo -e "${BLUE}[INFO]${NC} $1" >&2; }
success() { echo -e "${GREEN}[OK]${NC} $1" >&2; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1" >&2; }
error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }
progress() { echo -e "${CYAN}[*]${NC} $1" >&2; }

# Default values
DEVICE="/dev/video0"
OUTPUT="camera-info.txt"

# Show usage information
show_usage() {
    cat << EOF
Camera Diagnostic Information Collector

Usage: $0 [OPTIONS]

Options:
  --device <path>   Camera device to diagnose (default: /dev/video0)
  --output <path>   Output file path (default: camera-info.txt)
  --help            Show this help message

Description:
  Collects comprehensive diagnostic information about V4L2 camera devices,
  including system info, device capabilities, supported formats, kernel
  modules, and recent kernel messages. Useful for debugging camera issues
  or sharing configuration details.

Information Collected:
  - System information (kernel, Raspberry Pi model)
  - All video devices (/dev/video*)
  - V4L2 device list and capabilities
  - Camera detection (Raspberry Pi specific)
  - Loaded kernel modules (video/camera related)
  - Boot configuration (camera settings)
  - Device formats and resolutions
  - FFmpeg device probe
  - Processes using video devices
  - Media controller topology
  - Recent kernel messages about cameras

Examples:
  # Diagnose default device, save to default file
  $0

  # Diagnose specific device
  $0 --device /dev/video2

  # Save to custom location
  $0 --output /tmp/camera-diag-\$(date +%Y%m%d-%H%M%S).txt

  # Diagnose specific device and custom output
  $0 --device /dev/video1 --output ~/pi-camera-info.txt

Requirements:
  - v4l2-ctl (from v4l-utils package)
  - ffmpeg (optional, for format probing)
  - media-ctl (optional, for media controller topology)

Notes:
  - The script continues even if individual commands fail
  - Failed sections are marked with [FAILED] or [NOT AVAILABLE]
  - Output is plain text suitable for sharing/pasting
  - Progress messages go to stderr, data goes to output file
EOF
}

# Parse command-line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --device)
                DEVICE="$2"
                shift 2
                ;;
            --output)
                OUTPUT="$2"
                shift 2
                ;;
            --help|-h)
                show_usage
                exit 0
                ;;
            *)
                error "Unknown option: $1"
                echo "" >&2
                show_usage
                exit 1
                ;;
        esac
    done
}

# Validate output directory
validate_output() {
    local output_dir
    output_dir=$(dirname "$OUTPUT")

    if [ ! -d "$output_dir" ]; then
        error "Output directory does not exist: $output_dir"
        return 1
    fi

    if [ ! -w "$output_dir" ]; then
        error "Output directory is not writable: $output_dir"
        return 1
    fi
}

# Print section header to output file
print_section() {
    local title="$1"
    echo "" >> "$OUTPUT"
    echo "========================================================================" >> "$OUTPUT"
    echo "  $title" >> "$OUTPUT"
    echo "========================================================================" >> "$OUTPUT"
    echo "" >> "$OUTPUT"
}

# Print subsection header to output file
print_subsection() {
    local title="$1"
    echo "" >> "$OUTPUT"
    echo "--- $title ---" >> "$OUTPUT"
    echo "" >> "$OUTPUT"
}

# Execute command and capture output, handle failures gracefully
run_command() {
    local description="$1"
    local command="$2"

    print_subsection "$description"
    echo "Command: $command" >> "$OUTPUT"
    echo "" >> "$OUTPUT"

    if output=$(eval "$command" 2>&1); then
        if [ -z "$output" ]; then
            echo "[NO OUTPUT]" >> "$OUTPUT"
        else
            echo "$output" >> "$OUTPUT"
        fi
    else
        echo "[FAILED]" >> "$OUTPUT"
        if [ -n "$output" ]; then
            echo "" >> "$OUTPUT"
            echo "Error output:" >> "$OUTPUT"
            echo "$output" >> "$OUTPUT"
        fi
    fi
}

# Collect system information
collect_system_info() {
    progress "Collecting system information..."
    print_section "SYSTEM INFORMATION"

    run_command "Kernel and system info" "uname -a"
    run_command "OS Release" "cat /etc/os-release"

    # Raspberry Pi model detection
    if [ -f /proc/device-tree/model ]; then
        run_command "Raspberry Pi Model" "cat /proc/device-tree/model"
    else
        print_subsection "Raspberry Pi Model"
        echo "Command: cat /proc/device-tree/model" >> "$OUTPUT"
        echo "" >> "$OUTPUT"
        echo "[NOT AVAILABLE - Not a Raspberry Pi or file missing]" >> "$OUTPUT"
    fi

    run_command "CPU Information" "lscpu 2>&1 | head -20"
    run_command "Memory Information" "free -h"
}

# Collect video device information
collect_device_list() {
    progress "Collecting video device list..."
    print_section "VIDEO DEVICES"

    run_command "Video device files" "ls -la /dev/video* 2>&1"
    run_command "Media device files" "ls -la /dev/media* 2>&1"

    # v4l2-ctl device list
    if command -v v4l2-ctl &>/dev/null; then
        run_command "V4L2 device list" "v4l2-ctl --list-devices"
    else
        print_subsection "V4L2 device list"
        echo "Command: v4l2-ctl --list-devices" >> "$OUTPUT"
        echo "" >> "$OUTPUT"
        echo "[NOT AVAILABLE - v4l2-ctl not installed]" >> "$OUTPUT"
    fi
}

# Collect Raspberry Pi camera detection
collect_camera_detection() {
    progress "Checking camera detection..."
    print_section "CAMERA DETECTION"

    # Raspberry Pi specific camera detection
    if command -v vcgencmd &>/dev/null; then
        run_command "vcgencmd camera detection" "vcgencmd get_camera"
    else
        print_subsection "vcgencmd camera detection"
        echo "Command: vcgencmd get_camera" >> "$OUTPUT"
        echo "" >> "$OUTPUT"
        echo "[NOT AVAILABLE - Not a Raspberry Pi or vcgencmd not installed]" >> "$OUTPUT"
    fi

    # libcamera detection
    if command -v libcamera-hello &>/dev/null; then
        run_command "libcamera camera list" "libcamera-hello --list-cameras 2>&1 | head -50"
    else
        print_subsection "libcamera camera list"
        echo "Command: libcamera-hello --list-cameras" >> "$OUTPUT"
        echo "" >> "$OUTPUT"
        echo "[NOT AVAILABLE - libcamera not installed]" >> "$OUTPUT"
    fi
}

# Collect kernel module information
collect_kernel_modules() {
    progress "Collecting kernel module information..."
    print_section "KERNEL MODULES"

    run_command "Video/Camera related modules" "lsmod | grep -E 'video|camera|v4l|imx|unicam|bcm2835' || echo '[No matching modules loaded]'"

    # Check specific module info
    for module in videodev v4l2_common videobuf2_core videobuf2_v4l2 videobuf2_vmalloc videobuf2_dma_contig bcm2835_v4l2 bcm2835_unicam imx708 vivid v4l2loopback; do
        if modinfo "$module" &>/dev/null; then
            run_command "Module info: $module" "modinfo '$module' | head -20"
        fi
    done
}

# Collect boot configuration
collect_boot_config() {
    progress "Collecting boot configuration..."
    print_section "BOOT CONFIGURATION"

    # Check various possible locations for boot config
    local config_files=(
        "/boot/firmware/config.txt"
        "/boot/config.txt"
        "/boot/firmware/usercfg.txt"
    )

    for config_file in "${config_files[@]}"; do
        if [ -f "$config_file" ]; then
            run_command "Camera settings in $config_file" "grep -E 'camera|dtoverlay.*imx|dtoverlay.*ov|start_x|gpu_mem' '$config_file' 2>&1 || echo '[No camera-related settings found]'"
        fi
    done

    # Device tree overlays
    if [ -d "/proc/device-tree" ]; then
        run_command "Device tree compatible" "cat /proc/device-tree/compatible 2>&1 | tr '\0' '\n'"
    fi
}

# Collect specific device information
collect_device_info() {
    local device="$1"

    progress "Collecting information for $device..."
    print_section "DEVICE: $device"

    # Check if device exists
    if [ ! -e "$device" ]; then
        print_subsection "Device Status"
        echo "[DEVICE DOES NOT EXIST]" >> "$OUTPUT"
        return
    fi

    if [ ! -r "$device" ]; then
        print_subsection "Device Status"
        echo "[DEVICE NOT READABLE - Check permissions]" >> "$OUTPUT"
        return
    fi

    # v4l2-ctl commands
    if command -v v4l2-ctl &>/dev/null; then
        run_command "Current format" "v4l2-ctl --device='$device' --get-fmt-video"
        run_command "Supported formats and resolutions" "v4l2-ctl --device='$device' --list-formats-ext"
        run_command "All capabilities and settings" "v4l2-ctl --device='$device' --all"
    else
        print_subsection "V4L2 device information"
        echo "[NOT AVAILABLE - v4l2-ctl not installed]" >> "$OUTPUT"
    fi

    # ffmpeg format probe
    if command -v ffmpeg &>/dev/null; then
        run_command "FFmpeg format probe" "ffmpeg -hide_banner -f v4l2 -list_formats all -i '$device' 2>&1 | grep -v '^ffmpeg version' | head -100"
    else
        print_subsection "FFmpeg format probe"
        echo "[NOT AVAILABLE - ffmpeg not installed]" >> "$OUTPUT"
    fi
}

# Collect process information
collect_process_info() {
    progress "Collecting process information..."
    print_section "PROCESSES USING VIDEO DEVICES"

    if command -v lsof &>/dev/null; then
        run_command "Processes with open video devices" "lsof /dev/video* 2>&1 || echo '[No processes using video devices or no video devices found]'"
    else
        print_subsection "Processes with open video devices"
        echo "Command: lsof /dev/video*" >> "$OUTPUT"
        echo "" >> "$OUTPUT"
        echo "[NOT AVAILABLE - lsof not installed]" >> "$OUTPUT"
    fi

    # Alternative using fuser if lsof not available
    if ! command -v lsof &>/dev/null && command -v fuser &>/dev/null; then
        run_command "fuser check for video devices" "fuser /dev/video* 2>&1 || echo '[No processes using video devices]'"
    fi
}

# Collect media controller topology
collect_media_controller() {
    progress "Collecting media controller topology..."
    print_section "MEDIA CONTROLLER TOPOLOGY"

    if ! command -v media-ctl &>/dev/null; then
        print_subsection "Media Controller"
        echo "[NOT AVAILABLE - media-ctl not installed]" >> "$OUTPUT"
        echo "" >> "$OUTPUT"
        echo "Install with: sudo apt-get install -y v4l-utils" >> "$OUTPUT"
        return
    fi

    # Check each media device
    local found_media=false
    for media_dev in /dev/media*; do
        if [ -e "$media_dev" ]; then
            found_media=true
            run_command "Media controller topology for $media_dev" "media-ctl -p -d '$media_dev' 2>&1"
        fi
    done

    if [ "$found_media" = false ]; then
        print_subsection "Media Controller"
        echo "[NO MEDIA DEVICES FOUND]" >> "$OUTPUT"
    fi
}

# Collect kernel messages
collect_kernel_messages() {
    progress "Collecting kernel messages..."
    print_section "KERNEL MESSAGES"

    run_command "Camera/Video related kernel messages" "dmesg | grep -iE 'video|camera|v4l|imx708|imx219|imx477|ov5647|unicam|bcm2835.*v4l' || echo '[No camera-related kernel messages found]'"

    # System journal if available
    if command -v journalctl &>/dev/null; then
        run_command "Recent camera messages from journal" "journalctl -k --no-pager -n 100 | grep -iE 'video|camera|v4l|imx|unicam' || echo '[No camera-related journal messages found]'"
    fi
}

# Collect USB device information (for USB cameras)
collect_usb_info() {
    progress "Collecting USB device information..."
    print_section "USB DEVICES"

    if command -v lsusb &>/dev/null; then
        run_command "All USB devices" "lsusb"
        run_command "USB device tree" "lsusb -t"
    else
        print_subsection "USB devices"
        echo "[NOT AVAILABLE - lsusb not installed]" >> "$OUTPUT"
    fi
}

# Write file header
write_header() {
    cat > "$OUTPUT" << EOF
========================================================================
CAMERA DIAGNOSTIC INFORMATION
========================================================================

Generated: $(date '+%Y-%m-%d %H:%M:%S %Z')
Hostname:  $(hostname)
User:      $(whoami)

Target Device: $DEVICE

This file contains comprehensive diagnostic information about V4L2
camera devices for debugging and sharing purposes.

========================================================================
EOF
}

# Write file footer
write_footer() {
    cat >> "$OUTPUT" << EOF

========================================================================
END OF DIAGNOSTIC INFORMATION
========================================================================

Generated by: $0
Date: $(date '+%Y-%m-%d %H:%M:%S %Z')

EOF
}

# Main execution
main() {
    # Parse arguments
    parse_args "$@"

    # Validate output
    info "Camera Diagnostic Information Collector"
    info "Output file: $OUTPUT"
    info "Target device: $DEVICE"
    echo "" >&2

    validate_output || exit 1

    # Initialize output file
    write_header

    # Collect all diagnostic information
    collect_system_info
    collect_device_list
    collect_camera_detection
    collect_kernel_modules
    collect_boot_config
    collect_device_info "$DEVICE"
    collect_process_info
    collect_media_controller
    collect_usb_info
    collect_kernel_messages

    # Finalize output file
    write_footer

    # Summary
    echo "" >&2
    success "Diagnostic information collected successfully"
    success "Output saved to: $OUTPUT"

    # Show file size
    local filesize
    filesize=$(stat -c "%s" "$OUTPUT" 2>/dev/null || stat -f "%z" "$OUTPUT" 2>/dev/null)
    if [ -n "$filesize" ]; then
        success "File size: $filesize bytes"
    fi

    echo "" >&2
    info "You can now share this file for debugging or analysis"
    info "Review the file with: cat $OUTPUT"
    info "Or open in editor: nano $OUTPUT"
    echo "" >&2

    exit 0
}

main "$@"
