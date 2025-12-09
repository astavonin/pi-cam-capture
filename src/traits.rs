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

impl Frame {
    /// Get RGB values for a pixel at the specified coordinates.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate (0-based)
    /// * `y` - Y coordinate (0-based)
    /// * `width` - Frame width in pixels
    ///
    /// # Returns
    ///
    /// Returns `Some((r, g, b))` if the coordinates are valid, `None` otherwise.
    ///
    /// # Notes
    ///
    /// This method assumes YUYV format (2 bytes per pixel). For odd x coordinates,
    /// it uses the Y value from the next pixel pair with the shared U/V values.
    #[must_use]
    pub fn pixel_at(&self, x: u32, y: u32, width: u32) -> Option<(u8, u8, u8)> {
        // YUYV format: [Y0 U Y1 V] repeats
        // Each pair of pixels shares U and V values

        // Calculate the byte offset for this pixel
        let pair_x = x & !1; // Round down to even x coordinate
        let offset = ((y * width + pair_x) * 2) as usize;

        // Check bounds - need 4 bytes starting at offset
        if offset + 3 >= self.data.len() {
            return None;
        }

        // Extract YUYV values using safe indexing
        let y_val = if x % 2 == 0 {
            *self.data.get(offset)? // Y0
        } else {
            *self.data.get(offset + 2)? // Y1
        };
        let u = *self.data.get(offset + 1)?;
        let v = *self.data.get(offset + 3)?;

        // Convert YUV to RGB
        Some(yuv_to_rgb(y_val, u, v))
    }
}

/// Convert YUV values to RGB.
///
/// Uses the ITU-R BT.601 conversion formula.
///
/// # Arguments
///
/// * `y` - Luminance value (16-235 for studio range)
/// * `u` - Blue-difference chroma value (16-240)
/// * `v` - Red-difference chroma value (16-240)
///
/// # Returns
///
/// RGB tuple with values clamped to 0-255 range.
#[must_use]
#[allow(clippy::many_single_char_names)]
fn yuv_to_rgb(y: u8, u: u8, v: u8) -> (u8, u8, u8) {
    // ITU-R BT.601 conversion
    let y_f = f32::from(y);
    let u_f = f32::from(u) - 128.0;
    let v_f = f32::from(v) - 128.0;

    let r = 1.402f32.mul_add(v_f, y_f);
    let g = 0.714_14f32.mul_add(-v_f, 0.344_14f32.mul_add(-u_f, y_f));
    let b = 1.772f32.mul_add(u_f, y_f);

    let clamp = |val: f32| -> u8 {
        if val < 0.0 {
            0
        } else if val > 255.0 {
            255
        } else {
            #[allow(clippy::cast_possible_truncation)]
            #[allow(clippy::cast_sign_loss)]
            {
                val as u8
            }
        }
    };

    (clamp(r), clamp(g), clamp(b))
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
