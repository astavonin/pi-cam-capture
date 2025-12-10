# pi-cam-capture

A Rust library for capturing video from Raspberry Pi cameras using V4L2.

## What is this?

This library lets you capture video frames from Raspberry Pi Camera Module 3 (and other V4L2 cameras). It's designed to be testable - you can write tests without needing actual camera hardware.

## Features

- Capture frames from Pi Camera Module 3
- Works with any V4L2 camera device
- Mock camera for testing (no hardware needed)
- Supports YUYV, MJPEG, and RGB formats
- Strict code quality (no unwraps, no panics)

## Hardware Setup

**You need to do this once on your Raspberry Pi:**

Edit `/boot/firmware/config.txt`:

```ini
# Disable auto-detection (it doesn't work well)
camera_auto_detect=0

# Add this at the bottom under [all] section
dtoverlay=imx708,always-on,cam1
```

Then reboot:
```bash
sudo reboot
```

Check if it worked:
```bash
v4l2-ctl --list-devices
# Should show: rp1-cfe with /dev/video0
```

## Building

```bash
# For your computer (x86_64)
cargo build

# For Raspberry Pi
cross build --release --target aarch64-unknown-linux-gnu
```

## Testing

```bash
# Unit tests (no hardware needed, uses mock camera)
cargo test-unit

# Integration tests (needs virtual camera loaded)
cargo test-integration

# All tests
cargo test-all
```

## Development

```bash
# Check code quality
cargo lint

# Auto-fix issues
cargo fix

# Run on Raspberry Pi
cargo run
```

## Supported Cameras

- Raspberry Pi Camera Module 3 (IMX708 sensor)
- Any V4L2-compatible camera
- USB webcams
