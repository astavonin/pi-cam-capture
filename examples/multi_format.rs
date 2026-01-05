//! Multiple format configuration example.
//!
//! Demonstrates how to configure different resolutions and FPS settings.
//! Shows what the camera driver actually provides vs what was requested.
//!
//! Usage:
//!   `cargo run --example multi_format`

use pi_cam_capture::{CaptureConfig, CaptureSession, FourCC};

fn main() {
    env_logger::init();

    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run() -> pi_cam_capture::Result<()> {
    println!("Pi Camera Capture - Multi-Format Example\n");

    // Test different configurations
    let configs = vec![
        ("High resolution, 30 FPS", 1920, 1080, 30),
        ("Medium resolution, 60 FPS", 1280, 720, 60),
        ("Low resolution, 120 FPS", 640, 480, 120),
    ];

    for (name, width, height, fps) in configs {
        println!("=== {name} ===");
        test_config(width, height, fps)?;
        println!();
    }

    Ok(())
}

fn test_config(width: u32, height: u32, fps: u32) -> pi_cam_capture::Result<()> {
    let config = CaptureConfig::builder()
        .resolution(width, height)
        .fps(fps)
        .format(FourCC::YUYV)
        .buffer_count(4)
        .build()?;

    println!("Requested: {width}x{height} at {fps} FPS");

    let mut session = CaptureSession::new(config)?;

    println!(
        "Actual:    {}x{} {:?} at {} FPS",
        session.actual_format().width,
        session.actual_format().height,
        session.actual_format().fourcc,
        session.config().fps
    );

    // Start streaming to get actual FPS
    session.start_stream()?;

    println!("Streaming: {} FPS (driver negotiated)", session.actual_fps());

    // Capture a test frame
    let frame = session.next_frame()?;
    println!(
        "Frame:     {} bytes, seq={}",
        frame.data.len(),
        frame.metadata.sequence
    );

    Ok(())
}
