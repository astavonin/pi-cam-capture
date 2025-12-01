use pi_cam_capture::{CameraDevice, CaptureStream, Format, FourCC, V4L2Device};

fn main() {
    let mut device = V4L2Device::open(0).expect("Failed to open device");

    println!("Device: {}", device.capabilities().card);
    println!("Driver: {}", device.capabilities().driver);

    let format = Format::new(1280, 720, FourCC::YUYV);
    let actual_format = device.set_format(&format).expect("Failed to set format");

    println!(
        "Format: {}x{} {:?}",
        actual_format.width, actual_format.height, actual_format.fourcc
    );

    let mut stream = device.create_stream(4).expect("Failed to create stream");

    loop {
        let frame = stream.next_frame().expect("Failed to capture frame");
        println!(
            "Frame {}: {} bytes, timestamp: {:?}",
            frame.metadata.sequence,
            frame.data.len(),
            frame.metadata.timestamp
        );
    }
}
