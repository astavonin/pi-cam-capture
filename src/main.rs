//! Pi-cam-capture binary for testing camera capture.

use pi_cam_capture::{CameraDevice, CaptureConfig, CaptureStream, Format, V4L2Device};

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run() -> pi_cam_capture::Result<()> {
    // Create configuration using builder pattern
    let config = CaptureConfig::builder()
        .device(0)
        .resolution(1280, 720)
        .fps(30)
        .buffer_count(4)
        .build()?;

    println!("Configuration:");
    println!("  Device: /dev/video{}", config.device_index);
    println!("  Resolution: {}x{}", config.width, config.height);
    println!("  Format: {:?}", config.format);
    println!("  Target FPS: {}", config.fps);
    println!("  Buffer count: {}", config.buffer_count);
    println!();

    // Open device
    let mut device = V4L2Device::open(config.device_index)?;

    println!("Device: {}", device.capabilities().card);
    println!("Driver: {}", device.capabilities().driver);

    // Set format from config
    let format = Format::new(config.width, config.height, config.format);
    let actual_format = device.set_format(&format)?;

    println!(
        "Format: {}x{} {:?}",
        actual_format.width, actual_format.height, actual_format.fourcc
    );
    println!();

    // Create stream with configured buffer count
    let mut stream = device.create_stream(config.buffer_count)?;

    println!("Capturing frames (Ctrl+C to stop)...");

    loop {
        let frame = stream.next_frame()?;
        println!(
            "Frame {}: {} bytes, timestamp: {:?}",
            frame.metadata.sequence,
            frame.data.len(),
            frame.metadata.timestamp
        );
    }
}
