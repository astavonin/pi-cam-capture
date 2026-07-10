//! Pi-cam-capture binary for testing camera capture.

use pi_cam_capture::{CaptureConfig, CaptureSession};

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
    println!("  Device: /dev/video{}", config.device_index());
    println!("  Resolution: {}x{}", config.width(), config.height());
    println!("  Format: {:?}", config.format());
    println!("  Target FPS: {}", config.fps());
    println!("  Buffer count: {}", config.buffer_count());
    println!();

    // Create capture session (opens device, sets format)
    let mut session = CaptureSession::new(config)?;

    println!("Device: {}", session.capabilities().card);
    println!("Driver: {}", session.capabilities().driver);
    println!(
        "Format: {}x{} {:?}",
        session.frame_layout().width,
        session.frame_layout().height,
        session.frame_layout().fourcc
    );
    println!();

    // Create streaming guard (starts streaming, allocates buffers)
    let mut stream = session.streaming()?;

    println!(
        "Streaming at {} FPS (Ctrl+C to stop)...",
        stream.actual_fps()
    );
    println!();

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
