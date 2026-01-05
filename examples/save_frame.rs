//! Save camera frame to file example.
//!
//! Captures a single frame and saves it as raw YUYV data.
//! You can convert the raw file to other formats using ffmpeg.
//!
//! Usage:
//!   `cargo run --example save_frame [output_file]`

use pi_cam_capture::{CaptureConfig, CaptureSession};
use std::fs::File;
use std::io::Write;

fn main() {
    env_logger::init();

    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run() -> pi_cam_capture::Result<()> {
    let output_file = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "frame.yuyv".to_owned());

    println!("Pi Camera Capture - Save Frame Example\n");

    // Configure for 1920x1080 at 30 FPS
    let config = CaptureConfig::builder()
        .resolution(1920, 1080)
        .fps(30)
        .build()?;

    println!("Opening camera...");
    let mut session = CaptureSession::new(config)?;

    println!(
        "Format: {}x{} {:?}",
        session.actual_format().width,
        session.actual_format().height,
        session.actual_format().fourcc
    );

    // Start streaming
    session.start_stream()?;

    // Skip first few frames (camera warmup)
    println!("Warming up camera...");
    for _ in 0..5 {
        session.next_frame()?;
    }

    // Capture the frame
    println!("Capturing frame...");
    let frame = session.next_frame()?;

    // Save to file
    println!("Saving to {output_file}...");
    let mut file = File::create(&output_file)?;
    file.write_all(&frame.data)?;

    let bytes_saved = frame.data.len();
    println!("\nSuccess! Saved {bytes_saved} bytes to {output_file}");

    let seq = frame.metadata.sequence;
    let ts = frame.metadata.timestamp;
    println!("Frame info: seq={seq}, ts={ts:?}");

    println!("\nConvert to PNG with:");
    let width = session.actual_format().width;
    let height = session.actual_format().height;
    println!("  ffmpeg -f rawvideo -pixel_format yuyv422 -video_size {width}x{height} \\");
    println!("    -i {output_file} frame.png");

    Ok(())
}
