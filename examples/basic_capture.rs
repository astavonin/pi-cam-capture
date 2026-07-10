//! Basic camera capture example.
//!
//! Demonstrates the simplest usage of the `CaptureSession` API to capture
//! and display frame metadata.
//!
//! Usage:
//!   `cargo run --example basic_capture`

use pi_cam_capture::{CaptureConfig, CaptureSession};

fn main() {
    env_logger::init();

    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run() -> pi_cam_capture::Result<()> {
    println!("Pi Camera Capture - Basic Example\n");

    // Use default configuration (1920x1080 YUYV at 30 FPS)
    let config = CaptureConfig::default();

    println!("Opening camera /dev/video{}...", config.device_index());

    // Create session - opens device and sets format
    let mut session = CaptureSession::new(config)?;

    // Display device info
    println!("Device: {}", session.capabilities().card);
    println!("Driver: {}", session.capabilities().driver);
    println!(
        "Format: {}x{} {:?}",
        session.frame_layout().width,
        session.frame_layout().height,
        session.frame_layout().fourcc
    );
    println!();

    // Create streaming guard - starts streaming, allocates buffers
    let mut stream = session.streaming()?;

    println!("Streaming at {} FPS", stream.actual_fps());
    println!("Press Ctrl+C to stop\n");

    // Capture and display 10 frames - stream persists, efficient
    for i in 1..=10 {
        let frame = stream.next_frame()?;
        println!(
            "Frame {}: seq={}, size={} bytes, ts={:?}",
            i,
            frame.metadata.sequence,
            frame.data.len(),
            frame.metadata.timestamp
        );
    }

    println!("\nCapture complete!");
    Ok(())
}
