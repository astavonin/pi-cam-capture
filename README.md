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

## Usage

### Quick Start

Add to your `Cargo.toml`:
```toml
[dependencies]
pi-cam-capture = "0.1"
```

Basic capture example:
```rust
use pi_cam_capture::{CaptureConfig, CaptureSession};

fn main() -> pi_cam_capture::Result<()> {
    // Use default configuration (1920x1080 YUYV at 30 FPS)
    let config = CaptureConfig::default();

    // Create session - opens device and sets format
    let mut session = CaptureSession::new(config)?;

    // Start streaming — returns a guard that borrows the session.
    // While the guard exists, the session cannot be reconfigured (compile-time guarantee).
    let mut stream = session.streaming()?;

    // Capture frames — the guard efficiently reuses mmap buffers across calls.
    for _ in 0..10 {
        let frame = stream.next_frame()?;
        println!("Frame {}: {} bytes", frame.metadata.sequence, frame.data.len());
    }
    // Guard dropped here: streaming stops, buffers released automatically.

    Ok(())
}
```

### Custom Configuration

```rust
use pi_cam_capture::{CaptureConfig, CaptureSession, FourCC};

let config = CaptureConfig::builder()
    .device(0)                    // /dev/video0
    .resolution(1280, 720)        // 720p
    .format(FourCC::YUYV)         // Pixel format
    .fps(60)                      // Target FPS
    .buffer_count(4)              // Number of buffers
    .build()?;

let mut session = CaptureSession::new(config)?;

// Start streaming — guard pattern ensures clean resource lifecycle
let mut stream = session.streaming()?;

// Capture a frame
let frame = stream.next_frame()?;
```

### Examples

See the `examples/` directory for more:

```bash
# Basic capture - capture and display frame info
cargo run --example basic_capture

# Save frame - capture and save raw YUYV data
cargo run --example save_frame frame.yuyv

# Multi-format - test different resolutions and FPS
cargo run --example multi_format
```

## Supported Cameras

- Raspberry Pi Camera Module 3 (IMX708 sensor)
- Any V4L2-compatible camera
- USB webcams
