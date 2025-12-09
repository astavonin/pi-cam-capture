//! Core traits and types for V4L2 camera abstraction.

use std::time::Duration;

/// Pixel format representation (e.g., YUYV, MJPG, RGB3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FourCC(pub [u8; 4]);

impl FourCC {
    /// Create a new `FourCC` from a 4-byte array.
    #[must_use]
    pub const fn new(code: &[u8; 4]) -> Self {
        Self(*code)
    }

    /// YUYV pixel format (4:2:2 packed).
    pub const YUYV: Self = Self::new(b"YUYV");
    /// MJPEG pixel format (Motion JPEG).
    pub const MJPG: Self = Self::new(b"MJPG");
    /// RGB3 pixel format (24-bit RGB).
    pub const RGB3: Self = Self::new(b"RGB3");
}

impl From<v4l::FourCC> for FourCC {
    fn from(fourcc: v4l::FourCC) -> Self {
        Self(fourcc.repr)
    }
}

impl From<FourCC> for v4l::FourCC {
    fn from(fourcc: FourCC) -> Self {
        Self::new(&fourcc.0)
    }
}

/// Video format specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Format {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Pixel format.
    pub fourcc: FourCC,
    /// Bytes per line (stride).
    pub stride: u32,
    /// Total frame size in bytes.
    pub size: u32,
}

impl Format {
    /// Create a new format specification.
    #[must_use]
    pub const fn new(width: u32, height: u32, fourcc: FourCC) -> Self {
        let stride = width * 2; // YUYV is 2 bytes per pixel
        let size = stride * height;
        Self {
            width,
            height,
            fourcc,
            stride,
            size,
        }
    }
}

/// Device capability flags.
#[derive(Debug, Clone, Default)]
pub struct DeviceCapabilities {
    /// Driver name.
    pub driver: String,
    /// Card/device name.
    pub card: String,
    /// Bus information.
    pub bus_info: String,
    /// Whether the device can capture video.
    pub can_capture: bool,
    /// Whether the device supports streaming.
    pub can_stream: bool,
}

/// Metadata for a captured frame.
#[derive(Debug, Clone)]
pub struct FrameMetadata {
    /// Frame sequence number.
    pub sequence: u32,
    /// Capture timestamp.
    pub timestamp: Duration,
    /// Actual bytes used in the frame buffer.
    pub bytes_used: u32,
}

/// A captured video frame.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Raw frame data.
    pub data: Vec<u8>,
    /// Frame metadata.
    pub metadata: FrameMetadata,
}

/// Error type for camera operations.
#[derive(Debug)]
pub enum CameraError {
    /// Device with given index was not found.
    DeviceNotFound(u32),
    /// Failed to open device.
    DeviceOpenFailed(String),
    /// Requested format is not supported.
    FormatNotSupported(Format),
    /// Error during streaming operation.
    StreamError(String),
    /// Operation timed out.
    Timeout,
    /// I/O error.
    Io(std::io::Error),
}

impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceNotFound(idx) => write!(f, "Device {idx} not found"),
            Self::DeviceOpenFailed(msg) => write!(f, "Failed to open device: {msg}"),
            Self::FormatNotSupported(fmt) => write!(f, "Format not supported: {fmt:?}"),
            Self::StreamError(msg) => write!(f, "Stream error: {msg}"),
            Self::Timeout => write!(f, "Operation timed out"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl std::error::Error for CameraError {}

impl From<std::io::Error> for CameraError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Result type for camera operations.
pub type Result<T> = std::result::Result<T, CameraError>;

/// Abstraction over camera device operations.
pub trait CameraDevice {
    /// The stream type returned by `create_stream`.
    type Stream<'a>: CaptureStream
    where
        Self: 'a;

    /// Get device capabilities.
    fn capabilities(&self) -> &DeviceCapabilities;

    /// Get current format.
    fn format(&self) -> Result<Format>;

    /// Set capture format. Returns the actual format set by the driver.
    fn set_format(&mut self, format: &Format) -> Result<Format>;

    /// Create a capture stream with the specified number of buffers.
    fn create_stream(&mut self, buffer_count: u32) -> Result<Self::Stream<'_>>;
}

/// Abstraction over capture stream operations.
pub trait CaptureStream {
    /// Capture the next frame from the stream.
    fn next_frame(&mut self) -> Result<Frame>;
}
