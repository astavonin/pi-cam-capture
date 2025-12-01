use std::time::Duration;

/// Pixel format representation (e.g., YUYV, MJPG, RGB3)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FourCC(pub [u8; 4]);

impl FourCC {
    pub const fn new(code: &[u8; 4]) -> Self {
        Self(*code)
    }

    pub const YUYV: Self = Self::new(b"YUYV");
    pub const MJPG: Self = Self::new(b"MJPG");
    pub const RGB3: Self = Self::new(b"RGB3");
}

impl From<v4l::FourCC> for FourCC {
    fn from(fourcc: v4l::FourCC) -> Self {
        Self(fourcc.repr)
    }
}

impl From<FourCC> for v4l::FourCC {
    fn from(fourcc: FourCC) -> Self {
        v4l::FourCC::new(&fourcc.0)
    }
}

/// Video format specification
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Format {
    pub width: u32,
    pub height: u32,
    pub fourcc: FourCC,
    pub stride: u32,
    pub size: u32,
}

impl Format {
    pub fn new(width: u32, height: u32, fourcc: FourCC) -> Self {
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

/// Device capability flags
#[derive(Debug, Clone, Default)]
pub struct DeviceCapabilities {
    pub driver: String,
    pub card: String,
    pub bus_info: String,
    pub can_capture: bool,
    pub can_stream: bool,
}

/// Metadata for a captured frame
#[derive(Debug, Clone)]
pub struct FrameMetadata {
    pub sequence: u32,
    pub timestamp: Duration,
    pub bytes_used: u32,
}

/// A captured video frame
#[derive(Debug, Clone)]
pub struct Frame {
    pub data: Vec<u8>,
    pub metadata: FrameMetadata,
}

/// Error type for camera operations
#[derive(Debug)]
pub enum CameraError {
    DeviceNotFound(u32),
    DeviceOpenFailed(String),
    FormatNotSupported(Format),
    StreamError(String),
    Timeout,
    Io(std::io::Error),
}

impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceNotFound(idx) => write!(f, "Device {} not found", idx),
            Self::DeviceOpenFailed(msg) => write!(f, "Failed to open device: {}", msg),
            Self::FormatNotSupported(fmt) => write!(f, "Format not supported: {:?}", fmt),
            Self::StreamError(msg) => write!(f, "Stream error: {}", msg),
            Self::Timeout => write!(f, "Operation timed out"),
            Self::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for CameraError {}

impl From<std::io::Error> for CameraError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, CameraError>;

/// Abstraction over camera device operations
pub trait CameraDevice {
    type Stream<'a>: CaptureStream
    where
        Self: 'a;

    fn capabilities(&self) -> &DeviceCapabilities;
    fn format(&self) -> Result<Format>;
    fn set_format(&mut self, format: &Format) -> Result<Format>;
    fn create_stream(&mut self, buffer_count: u32) -> Result<Self::Stream<'_>>;
}

/// Abstraction over capture stream operations
pub trait CaptureStream {
    fn next_frame(&mut self) -> Result<Frame>;
}
