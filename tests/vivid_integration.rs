//! Integration tests using vivid virtual camera.
//!
//! These tests require:
//! - The `integration` feature flag: `cargo test --features integration`
//! - The vivid kernel module loaded via: `./scripts/dev-setup.sh load-vivid`
//! - Access to /dev/video* devices (may require sudo or video group membership)
//!
//! Expected vivid configuration (set by `dev-setup.sh`):
//! - Device 1: Gray Ramp pattern (gradient) - `test_pattern=20`
//! - Device 2: 100% Colorbar pattern - `test_pattern=1`
//! - Format: 640x480 YUYV
//!
//! Tests will fail if vivid is not available or not configured correctly.

#![cfg(feature = "integration")]

use pi_cam_capture::device::V4L2Device;
use pi_cam_capture::traits::{CameraDevice, CaptureStream, Format, FourCC};
use pi_cam_capture::validation::{validate_color_bars, validate_frame_sequence, validate_gradient};
use serial_test::serial;
use std::fs;
use std::path::Path;

/// Find all available vivid virtual camera devices.
///
/// Uses sysfs to check device driver name before opening, avoiding
/// unnecessary device opens on real cameras.
///
/// Returns a vector of device indices for all vivid devices found.
fn find_vivid_devices() -> Vec<u32> {
    let video4linux = Path::new("/sys/class/video4linux");
    if !video4linux.exists() {
        return Vec::new();
    }

    let mut devices = Vec::new();
    for index in 0..10 {
        let name_path = video4linux.join(format!("video{index}")).join("name");
        let Ok(name) = fs::read_to_string(&name_path) else {
            continue;
        };

        if !name.to_lowercase().contains("vivid") {
            continue;
        }

        // Verify we can actually open it
        if V4L2Device::open(index).is_ok() {
            devices.push(index);
        }
    }
    devices
}

/// Macro to fail test if vivid is not available.
///
/// Returns the first vivid device index.
/// Integration tests MUST have vivid loaded - they should fail, not silently skip.
/// This ensures CI catches missing vivid configuration.
macro_rules! require_vivid {
    () => {
        match find_vivid_devices().first().copied() {
            Some(idx) => idx,
            None => {
                panic!(
                    "vivid virtual camera not available.\n\
                     Load vivid with: ./scripts/dev-setup.sh load-vivid\n\
                     Or run unit tests only: cargo test --lib"
                );
            }
        }
    };
}

/// Macro to get both vivid devices (for pattern-specific tests).
///
/// Returns a tuple of (gradient device index, colorbar device index).
/// Expects `dev-setup.sh` configuration:
/// - First device: Gray Ramp (gradient)
/// - Second device: 75% Colorbar
macro_rules! require_vivid_pair {
    () => {{
        let devices = find_vivid_devices();
        if devices.len() < 2 {
            panic!(
                "Two vivid devices required but found {}.\n\
                 Load vivid with: ./scripts/dev-setup.sh load-vivid\n\
                 Or run unit tests only: cargo test --lib",
                devices.len()
            );
        }
        (devices[0], devices[1])
    }};
}

#[test]
#[serial]
fn test_vivid_device_open() {
    let device_index = require_vivid!();

    let device = V4L2Device::open(device_index).expect("Failed to open vivid device");
    let caps = device.capabilities();

    assert!(caps.driver.contains("vivid"), "Expected vivid driver");
    assert!(caps.can_capture, "vivid should support capture");
    assert!(caps.can_stream, "vivid should support streaming");

    println!("Opened vivid device:");
    println!("  Driver: {}", caps.driver);
    println!("  Card: {}", caps.card);
    println!("  Bus: {}", caps.bus_info);
}

#[test]
#[serial]
fn test_vivid_format_query() {
    let device_index = require_vivid!();

    let device = V4L2Device::open(device_index).expect("Failed to open vivid device");
    let format = device.format().expect("Failed to query format");

    println!("Current format:");
    println!("  Resolution: {}x{}", format.width, format.height);
    println!("  FourCC: {:?}", format.fourcc);
    println!("  Stride: {}", format.stride);
    println!("  Size: {}", format.size);

    assert!(format.width > 0, "Width should be positive");
    assert!(format.height > 0, "Height should be positive");
}

#[test]
#[serial]
fn test_vivid_set_format() {
    let device_index = require_vivid!();

    let mut device = V4L2Device::open(device_index).expect("Failed to open vivid device");

    // Request a specific format
    let requested = Format::new(640, 480, FourCC::YUYV);
    let actual = device
        .set_format(&requested)
        .expect("Failed to set format");

    println!("Requested: {}x{} {:?}", requested.width, requested.height, requested.fourcc);
    println!("Actual: {}x{} {:?}", actual.width, actual.height, actual.fourcc);

    // vivid should accept common formats
    assert_eq!(actual.width, 640, "Width mismatch");
    assert_eq!(actual.height, 480, "Height mismatch");
}

#[test]
#[serial]
fn test_vivid_capture_single_frame() {
    let device_index = require_vivid!();

    let mut device = V4L2Device::open(device_index).expect("Failed to open vivid device");

    // Set a known format
    let format = Format::new(640, 480, FourCC::YUYV);
    let format = device.set_format(&format).expect("Failed to set format");

    // Create stream and capture a frame
    let mut stream = device.create_stream(4).expect("Failed to create stream");
    let frame = stream.next_frame().expect("Failed to capture frame");

    println!("Captured frame:");
    println!("  Sequence: {}", frame.metadata.sequence);
    println!("  Timestamp: {:?}", frame.metadata.timestamp);
    println!("  Bytes used: {}", frame.metadata.bytes_used);
    println!("  Data length: {}", frame.data.len());

    // Verify frame data
    let expected_size = (format.width * format.height * 2) as usize; // YUYV = 2 bytes/pixel
    assert!(
        frame.data.len() >= expected_size,
        "Frame data too small: {} < {}",
        frame.data.len(),
        expected_size
    );
    assert!(frame.metadata.bytes_used > 0, "Bytes used should be positive");
}

#[test]
#[serial]
fn test_vivid_capture_multiple_frames() {
    let device_index = require_vivid!();

    let mut device = V4L2Device::open(device_index).expect("Failed to open vivid device");

    let format = Format::new(640, 480, FourCC::YUYV);
    device.set_format(&format).expect("Failed to set format");

    let mut stream = device.create_stream(4).expect("Failed to create stream");

    // Capture multiple frames
    let frame_count = 10;
    let mut frames = Vec::with_capacity(frame_count);

    for i in 0..frame_count {
        let frame = stream.next_frame().expect("Failed to capture frame");
        println!(
            "Frame {}: seq={}, ts={:?}",
            i, frame.metadata.sequence, frame.metadata.timestamp
        );
        frames.push(frame);
    }

    // Validate frame sequence
    let result = validate_frame_sequence(&frames);
    assert!(
        result.is_ok(),
        "Frame sequence validation failed: {:?}",
        result.err()
    );
}

#[test]
#[serial]
fn test_vivid_gradient_pattern() {
    let (gradient_device, _) = require_vivid_pair!();

    let mut device =
        V4L2Device::open(gradient_device).expect("Failed to open vivid gradient device");

    let format = Format::new(640, 480, FourCC::YUYV);
    let format = device.set_format(&format).expect("Failed to set format");

    let mut stream = device.create_stream(4).expect("Failed to create stream");
    let frame = stream.next_frame().expect("Failed to capture frame");

    // First vivid device should be configured with Gray Ramp (gradient) pattern
    // by dev-setup.sh (test_pattern=20)
    let result = validate_gradient(&frame, &format);
    assert!(
        result.is_ok(),
        "Gradient validation failed on first vivid device.\n\
         Expected Gray Ramp pattern (test_pattern=20).\n\
         Run: ./scripts/dev-setup.sh load-vivid\n\
         Error: {:?}",
        result.err()
    );
    println!("Gradient pattern validation passed");
}

#[test]
#[serial]
fn test_vivid_colorbar_pattern() {
    let (_, colorbar_device) = require_vivid_pair!();

    let mut device =
        V4L2Device::open(colorbar_device).expect("Failed to open vivid colorbar device");

    let format = Format::new(640, 480, FourCC::YUYV);
    let format = device.set_format(&format).expect("Failed to set format");

    let mut stream = device.create_stream(4).expect("Failed to create stream");
    let frame = stream.next_frame().expect("Failed to capture frame");

    // Second vivid device should be configured with 100% Colorbar pattern
    // by dev-setup.sh (test_pattern=1)
    let result = validate_color_bars(&frame, &format);
    assert!(
        result.is_ok(),
        "Color bars validation failed on second vivid device.\n\
         Expected 100% Colorbar pattern (test_pattern=1).\n\
         Run: ./scripts/dev-setup.sh load-vivid\n\
         Error: {:?}",
        result.err()
    );
    println!("Color bars pattern validation passed");
}

#[test]
#[serial]
fn test_vivid_pixel_access() {
    let device_index = require_vivid!();

    let mut device = V4L2Device::open(device_index).expect("Failed to open vivid device");

    let format = Format::new(640, 480, FourCC::YUYV);
    let format = device.set_format(&format).expect("Failed to set format");

    let mut stream = device.create_stream(4).expect("Failed to create stream");
    let frame = stream.next_frame().expect("Failed to capture frame");

    // Test pixel access at various positions
    let test_points = [(0, 0), (320, 240), (639, 479), (100, 100)];

    for (x, y) in test_points {
        if let Some((r, g, b)) = frame.pixel_at(x, y, format.width) {
            println!("Pixel at ({x}, {y}): RGB({r}, {g}, {b})");
        } else {
            println!("Pixel at ({x}, {y}): out of bounds or invalid");
        }
    }

    // Verify center pixel is accessible
    let center = frame.pixel_at(format.width / 2, format.height / 2, format.width);
    assert!(center.is_some(), "Center pixel should be accessible");
}
