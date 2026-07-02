//! Core traits and types for V4L2 camera abstraction.

use std::time::Duration;

use crate::error::Result;

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
    /// Create a new format specification with stride and size computed from the pixel format.
    ///
    /// For MJPG, stride and size are set to 0 (variable; the driver fills them after negotiation).
    #[must_use]
    pub const fn new(width: u32, height: u32, fourcc: FourCC) -> Self {
        let stride = match fourcc.0 {
            [b'Y', b'U', b'Y', b'V'] => width * 2,
            [b'R', b'G', b'B', b'3'] => width * 3,
            _ => 0, // MJPG and others: variable / driver-negotiated
        };
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
        let pair_x = x & !1; // Round down to even x coordinate

        // Reject x coordinates that are out of the declared row width
        if pair_x >= width {
            return None;
        }

        // Widen to usize before multiplication to avoid u32 overflow on large frames
        let offset = (y as usize)
            .checked_mul(width as usize)?
            .checked_add(pair_x as usize)?
            .checked_mul(2)?;

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

        Some(yuv_to_rgb(y_val, u, v))
    }
}

/// A zero-copy view into a captured video frame.
///
/// Unlike [`Frame`], this type borrows the underlying mmap buffer directly,
/// avoiding a per-frame allocation. The borrow keeps the buffer locked until
/// this value is dropped, after which the next frame can be captured.
///
/// Obtain via [`CaptureStream::next_frame_ref`](crate::session::CaptureStream::next_frame_ref)
/// when the underlying stream supports borrowed frame access.
#[derive(Debug)]
pub struct FrameRef<'a> {
    /// Raw frame data borrowed from the mmap buffer.
    pub data: &'a [u8],
    /// Frame metadata.
    pub metadata: FrameMetadata,
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
pub(crate) fn yuv_to_rgb(y: u8, u: u8, v: u8) -> (u8, u8, u8) {
    // ITU-R BT.601 coefficients for full-range YCbCr → RGB
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

/// Abstraction over camera device operations.
pub trait CameraDevice {
    /// The stream type produced by [`create_stream`](Self::create_stream).
    type Stream<'a>: CaptureStream
    where
        Self: 'a;

    /// Returns the device capabilities reported by the driver.
    fn capabilities(&self) -> &DeviceCapabilities;

    /// Queries the current capture format from the driver.
    fn format(&self) -> Result<Format>;

    /// Requests a capture format from the driver.
    ///
    /// The driver may return a different format than requested.
    /// Always use the returned `Format` for subsequent operations.
    fn set_format(&mut self, format: &Format) -> Result<Format>;

    /// Creates a capture stream, allocating `buffer_count` mmap buffers and
    /// targeting `fps` frames per second.
    ///
    /// The returned stream borrows from `self` for its lifetime.
    fn create_stream(&mut self, buffer_count: u32, fps: u32) -> Result<Self::Stream<'_>>;
}

/// Abstraction over capture stream operations.
pub trait CaptureStream {
    /// Captures the next frame, blocking until one is available.
    fn next_frame(&mut self) -> Result<Frame>;

    /// Returns the actual FPS negotiated with the driver (or requested FPS for mock devices).
    fn actual_fps(&self) -> u32;
}

/// Capability for streams that can return a borrowed frame view.
///
/// The returned [`FrameRef`] borrows from `self`, so callers cannot request
/// another frame while the borrowed view is still alive.
pub trait BorrowedCaptureStream {
    /// Captures the next frame as a borrowed view of stream-owned storage.
    fn next_frame_ref(&mut self) -> Result<FrameRef<'_>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(width: u32, height: u32, y0: u8, u: u8, y1: u8, v: u8) -> Frame {
        // Fill with a single repeating YUYV pattern
        let size = (width * height * 2) as usize;
        let mut data = vec![0u8; size];
        for chunk in data.chunks_mut(4) {
            chunk[0] = y0;
            chunk[1] = u;
            chunk[2] = y1;
            chunk[3] = v;
        }
        Frame {
            data,
            metadata: FrameMetadata {
                sequence: 0,
                timestamp: std::time::Duration::ZERO,
                #[allow(clippy::cast_possible_truncation)] // frame size fits u32 in practice
                bytes_used: size as u32,
            },
        }
    }

    #[test]
    fn test_pixel_at_even_x() {
        let frame = make_frame(4, 4, 100, 128, 200, 128);
        let (r, g, b) = frame.pixel_at(0, 0, 4).expect("should be Some");
        let expected = yuv_to_rgb(100, 128, 128);
        assert_eq!((r, g, b), expected);
    }

    #[test]
    fn test_pixel_at_odd_x() {
        let frame = make_frame(4, 4, 100, 128, 200, 128);
        let (r, g, b) = frame.pixel_at(1, 0, 4).expect("should be Some");
        let expected = yuv_to_rgb(200, 128, 128);
        assert_eq!((r, g, b), expected);
    }

    #[test]
    fn test_pixel_at_last_pixel() {
        let frame = make_frame(4, 4, 100, 128, 200, 128);
        // x=3 (odd), y=3 — last pixel
        assert!(frame.pixel_at(3, 3, 4).is_some());
    }

    #[test]
    fn test_pixel_at_out_of_bounds_x() {
        let frame = make_frame(4, 4, 100, 128, 200, 128);
        assert!(frame.pixel_at(4, 0, 4).is_none());
    }

    #[test]
    fn test_pixel_at_out_of_bounds_y() {
        let frame = make_frame(4, 4, 100, 128, 200, 128);
        assert!(frame.pixel_at(0, 4, 4).is_none());
    }

    #[test]
    fn test_pixel_at_overflow_coords() {
        let frame = make_frame(4, 4, 100, 128, 200, 128);
        assert!(frame.pixel_at(u32::MAX, u32::MAX, u32::MAX).is_none());
    }

    #[test]
    fn test_pixel_at_empty_frame() {
        let frame = Frame {
            data: vec![],
            metadata: FrameMetadata {
                sequence: 0,
                timestamp: std::time::Duration::ZERO,
                bytes_used: 0,
            },
        };
        assert!(frame.pixel_at(0, 0, 4).is_none());
    }

    #[test]
    fn test_format_new_yuyv_stride() {
        let f = Format::new(640, 480, FourCC::YUYV);
        assert_eq!(f.stride, 1280);
        assert_eq!(f.size, 614_400);
    }

    #[test]
    fn test_format_new_rgb3_stride() {
        let f = Format::new(640, 480, FourCC::RGB3);
        assert_eq!(f.stride, 1920);
        assert_eq!(f.size, 921_600);
    }

    #[test]
    fn test_format_new_mjpg_stride_zero() {
        let f = Format::new(640, 480, FourCC::MJPG);
        assert_eq!(f.stride, 0);
        assert_eq!(f.size, 0);
    }
}
