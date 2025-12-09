//! Pi-cam-capture binary for testing camera capture.

use pi_cam_capture::{CameraDevice, CaptureStream, Format, FourCC, V4L2Device};

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run() -> pi_cam_capture::traits::Result<()> {
    let mut device = V4L2Device::open(0)?;

    println!("Device: {}", device.capabilities().card);
    println!("Driver: {}", device.capabilities().driver);

    let format = Format::new(1280, 720, FourCC::YUYV);
    let actual_format = device.set_format(&format)?;

    println!(
        "Format: {}x{} {:?}",
        actual_format.width, actual_format.height, actual_format.fourcc
    );

    let mut stream = device.create_stream(4)?;

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
