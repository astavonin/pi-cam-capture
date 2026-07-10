//! Core traits and types for V4L2 camera abstraction.

use std::{fmt, time::Duration};

use crate::error::{FrameError, Result};

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
    /// NV12 pixel format (semi-planar 4:2:0).
    pub const NV12: Self = Self::new(b"NV12");
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
            [b'N', b'V', b'1', b'2'] => width,
            _ => 0, // MJPG and others: variable / driver-negotiated
        };
        let size = match fourcc.0 {
            [b'N', b'V', b'1', b'2'] => (stride * height * 3) / 2,
            _ => stride * height,
        };
        Self {
            width,
            height,
            fourcc,
            stride,
            size,
        }
    }
}

/// Driver-reported layout of a negotiated capture buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameLayout {
    /// Driver-returned frame width in pixels.
    pub width: u32,
    /// Driver-returned frame height in pixels.
    pub height: u32,
    /// Driver-returned pixel format.
    pub fourcc: FourCC,
    /// Driver-returned bytes per line.
    pub stride: u32,
    /// Driver-returned total buffer size in bytes.
    pub size: u32,
}

/// Field that failed `FrameLayout` validation.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutInvalidField {
    /// The driver-reported stride was below the required lower bound.
    Stride,
    /// The driver-reported size was below the required lower bound.
    Size,
}

/// Validation error for a driver-reported frame layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutValidationError {
    /// The first invalid field.
    pub field: LayoutInvalidField,
    /// The driver-reported value for the invalid field.
    pub value: u32,
}

impl fmt::Display for LayoutValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {:?}: {}", self.field, self.value)
    }
}

impl std::error::Error for LayoutValidationError {}

impl FrameLayout {
    /// Create a validated layout from driver-returned values.
    ///
    /// # Errors
    ///
    /// Returns `LayoutValidationError` if the stride or size is below the
    /// lower bound for the pixel format.
    pub fn new(
        width: u32,
        height: u32,
        fourcc: FourCC,
        stride: u32,
        size: u32,
    ) -> std::result::Result<Self, LayoutValidationError> {
        validate_layout(width, height, fourcc, stride, size)?;
        Ok(Self {
            width,
            height,
            fourcc,
            stride,
            size,
        })
    }

    /// Create a validated layout from a driver-returned `Format`.
    ///
    /// # Errors
    ///
    /// Returns `LayoutValidationError` if the format's stride or size fails
    /// the layout invariants.
    pub fn from_format(format: &Format) -> std::result::Result<Self, LayoutValidationError> {
        Self::new(
            format.width,
            format.height,
            format.fourcc,
            format.stride,
            format.size,
        )
    }
}

fn validate_layout(
    width: u32,
    height: u32,
    fourcc: FourCC,
    stride: u32,
    size: u32,
) -> std::result::Result<(), LayoutValidationError> {
    match bytes_per_row_factor(fourcc) {
        Some(bytes_per_pixel) => {
            let min_stride = u64::from(width) * u64::from(bytes_per_pixel);
            if stride == 0 || u64::from(stride) < min_stride {
                return Err(LayoutValidationError {
                    field: LayoutInvalidField::Stride,
                    value: stride,
                });
            }

            let min_size = lower_bound_size(fourcc, stride, height);
            if size == 0 || u64::from(size) < min_size {
                return Err(LayoutValidationError {
                    field: LayoutInvalidField::Size,
                    value: size,
                });
            }
        }
        None if fourcc == FourCC::MJPG => {
            if size == 0 {
                return Err(LayoutValidationError {
                    field: LayoutInvalidField::Size,
                    value: size,
                });
            }
        }
        None => {
            return Err(LayoutValidationError {
                field: LayoutInvalidField::Stride,
                value: stride,
            });
        }
    }

    Ok(())
}

const fn bytes_per_row_factor(fourcc: FourCC) -> Option<u32> {
    match fourcc.0 {
        [b'Y', b'U', b'Y', b'V'] => Some(2),
        [b'R', b'G', b'B', b'3'] => Some(3),
        [b'N', b'V', b'1', b'2'] => Some(1),
        _ => None,
    }
}

fn lower_bound_size(fourcc: FourCC, stride: u32, height: u32) -> u64 {
    let packed_size = u64::from(stride) * u64::from(height);
    if fourcc == FourCC::NV12 {
        (packed_size * 3) / 2
    } else {
        packed_size
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
    /// Negotiated buffer layout for this frame.
    pub layout: FrameLayout,
}

impl Frame {
    /// Construct a frame with its negotiated layout.
    #[must_use]
    pub const fn new(data: Vec<u8>, metadata: FrameMetadata, layout: FrameLayout) -> Self {
        Self {
            data,
            metadata,
            layout,
        }
    }

    /// Return this frame's negotiated layout.
    #[must_use]
    pub const fn layout(&self) -> &FrameLayout {
        &self.layout
    }

    /// Get RGB values for a pixel at the specified coordinates.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate (0-based)
    /// * `y` - Y coordinate (0-based)
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some((r, g, b)))` if the coordinates are valid,
    /// `Ok(None)` for out-of-bounds coordinates, or `FrameError` if the
    /// negotiated pixel format does not support direct pixel access.
    ///
    /// # Notes
    ///
    /// YUYV access converts the sampled YUV values to RGB. RGB3 access returns
    /// the three stored RGB bytes directly.
    pub fn pixel_at(
        &self,
        x: u32,
        y: u32,
    ) -> std::result::Result<Option<(u8, u8, u8)>, FrameError> {
        pixel_at_with_layout(self.data.as_slice(), &self.layout, x, y)
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
    /// Negotiated buffer layout for this frame.
    pub layout: FrameLayout,
}

impl FrameRef<'_> {
    /// Return this frame view's negotiated layout.
    #[must_use]
    pub const fn layout(&self) -> &FrameLayout {
        &self.layout
    }

    /// Get RGB values for a pixel at the specified coordinates.
    ///
    /// See [`Frame::pixel_at`] for format-specific behavior.
    pub fn pixel_at(
        &self,
        x: u32,
        y: u32,
    ) -> std::result::Result<Option<(u8, u8, u8)>, FrameError> {
        pixel_at_with_layout(self.data, &self.layout, x, y)
    }
}

fn pixel_at_with_layout(
    data: &[u8],
    layout: &FrameLayout,
    x: u32,
    y: u32,
) -> std::result::Result<Option<(u8, u8, u8)>, FrameError> {
    if layout.fourcc == FourCC::YUYV {
        return Ok(yuyv_pixel_at(data, layout, x, y));
    }

    if layout.fourcc == FourCC::RGB3 {
        return Ok(rgb3_pixel_at(data, layout, x, y));
    }

    Err(FrameError::UnsupportedPixelAccess(layout.fourcc))
}

fn yuyv_pixel_at(data: &[u8], layout: &FrameLayout, x: u32, y: u32) -> Option<(u8, u8, u8)> {
    if x >= layout.width || y >= layout.height {
        return None;
    }

    let pair_x = x & !1;
    let row_offset = (y as usize).checked_mul(layout.stride as usize)?;
    let pixel_offset = (pair_x as usize).checked_mul(2)?;
    let offset = row_offset.checked_add(pixel_offset)?;
    let chunk = data.get(offset..offset.checked_add(4)?)?;

    let y_val = if x % 2 == 0 {
        *chunk.first()?
    } else {
        *chunk.get(2)?
    };
    let u = *chunk.get(1)?;
    let v = *chunk.get(3)?;

    Some(yuv_to_rgb(y_val, u, v))
}

fn rgb3_pixel_at(data: &[u8], layout: &FrameLayout, x: u32, y: u32) -> Option<(u8, u8, u8)> {
    if x >= layout.width || y >= layout.height {
        return None;
    }

    let row_offset = (y as usize).checked_mul(layout.stride as usize)?;
    let pixel_offset = (x as usize).checked_mul(3)?;
    let offset = row_offset.checked_add(pixel_offset)?;
    let chunk = data.get(offset..offset.checked_add(3)?)?;

    Some((*chunk.first()?, *chunk.get(1)?, *chunk.get(2)?))
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
    use crate::error::FrameError;

    fn metadata(size: usize) -> FrameMetadata {
        FrameMetadata {
            sequence: 0,
            timestamp: std::time::Duration::ZERO,
            #[allow(clippy::cast_possible_truncation)] // frame size fits u32 in these tests
            bytes_used: size as u32,
        }
    }

    fn make_yuyv_frame(
        width: u32,
        height: u32,
        stride: u32,
        y0: u8,
        u: u8,
        y1: u8,
        v: u8,
    ) -> Frame {
        let size = (stride * height) as usize;
        let mut data = vec![0u8; size];
        for row in data.chunks_mut(stride as usize) {
            for chunk in row.chunks_exact_mut(4).take((width / 2) as usize) {
                chunk.copy_from_slice(&[y0, u, y1, v]);
            }
        }
        let layout = FrameLayout::new(width, height, FourCC::YUYV, stride, stride * height)
            .expect("valid YUYV layout");
        Frame {
            data,
            metadata: metadata(size),
            layout,
        }
    }

    #[test]
    fn test_fourcc_nv12_constant_matches_v4l_encoding() {
        let v4l_fourcc: v4l::FourCC = FourCC::NV12.into();
        let round_trip = FourCC::from(v4l_fourcc);
        assert_eq!(round_trip, FourCC::NV12);
        assert_eq!(round_trip.0, *b"NV12");
    }

    #[test]
    fn test_pixel_at_even_x() {
        let frame = make_yuyv_frame(4, 4, 8, 100, 128, 200, 128);
        let (r, g, b) = frame.pixel_at(0, 0).expect("supported").expect("in bounds");
        let expected = yuv_to_rgb(100, 128, 128);
        assert_eq!((r, g, b), expected);
    }

    #[test]
    fn test_pixel_at_odd_x() {
        let frame = make_yuyv_frame(4, 4, 8, 100, 128, 200, 128);
        let (r, g, b) = frame.pixel_at(1, 0).expect("supported").expect("in bounds");
        let expected = yuv_to_rgb(200, 128, 128);
        assert_eq!((r, g, b), expected);
    }

    #[test]
    fn test_pixel_at_last_pixel() {
        let frame = make_yuyv_frame(4, 4, 8, 100, 128, 200, 128);
        assert!(frame.pixel_at(3, 3).expect("supported").is_some());
    }

    #[test]
    fn test_pixel_at_out_of_bounds_x() {
        let frame = make_yuyv_frame(4, 4, 8, 100, 128, 200, 128);
        assert!(frame.pixel_at(4, 0).expect("supported").is_none());
    }

    #[test]
    fn test_pixel_at_out_of_bounds_y() {
        let frame = make_yuyv_frame(4, 4, 8, 100, 128, 200, 128);
        assert!(frame.pixel_at(0, 4).expect("supported").is_none());
    }

    #[test]
    fn test_pixel_at_overflow_coords() {
        let frame = make_yuyv_frame(4, 4, 8, 100, 128, 200, 128);
        assert!(frame
            .pixel_at(u32::MAX, u32::MAX)
            .expect("supported")
            .is_none());
    }

    #[test]
    fn test_pixel_at_empty_frame() {
        let layout = FrameLayout::new(4, 4, FourCC::YUYV, 8, 32).expect("valid layout");
        let frame = Frame {
            data: vec![],
            metadata: metadata(0),
            layout,
        };
        assert!(frame.pixel_at(0, 0).expect("supported").is_none());
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

    #[test]
    fn test_format_new_nv12_stride() {
        let f = Format::new(640, 480, FourCC::NV12);
        assert_eq!(f.stride, 640);
        assert_eq!(f.size, 460_800);
    }

    #[test]
    fn test_framelayout_stride_size_invariants_yuyv() {
        let layout = FrameLayout::new(640, 480, FourCC::YUYV, 1288, 618_240)
            .expect("valid padded YUYV layout");
        assert_eq!(layout.stride, 1288);
        assert_eq!(layout.size, 618_240);
    }

    #[test]
    fn test_framelayout_stride_size_invariants_nv12() {
        let layout = FrameLayout::new(640, 480, FourCC::NV12, 656, 472_320)
            .expect("valid padded NV12 layout");
        assert_eq!(layout.stride, 656);
        assert_eq!(layout.size, 472_320);
    }

    #[test]
    fn test_framelayout_stride_size_invariants_rgb3() {
        let layout = FrameLayout::new(640, 480, FourCC::RGB3, 1936, 929_280)
            .expect("valid padded RGB3 layout");
        assert_eq!(layout.stride, 1936);
        assert_eq!(layout.size, 929_280);
    }

    #[test]
    fn test_framelayout_nv12_odd_height_accepted_at_floor_divided_minimum() {
        let stride = 8;
        let size = (stride * 3 * 3) / 2;
        let layout = FrameLayout::new(4, 3, FourCC::NV12, stride, size)
            .expect("floor-divided minimum should be valid");
        assert_eq!(layout.size, size);
    }

    #[test]
    fn test_framelayout_nv12_odd_height_rejected_below_floor_divided_minimum() {
        let stride = 8;
        let size = (stride * 3 * 3) / 2 - 1;
        let err = FrameLayout::new(4, 3, FourCC::NV12, stride, size)
            .expect_err("size below lower bound should fail");
        assert_eq!(err.field, LayoutInvalidField::Size);
        assert_eq!(err.value, size);
    }

    #[test]
    fn test_pixel_at_uses_stride_not_width_for_row_offset() {
        let width = 4;
        let height = 3;
        let stride = width * 2 + 8;
        let mut frame = make_yuyv_frame(width, height, stride, 10, 128, 20, 128);

        for y in 0..height {
            let marker = match y {
                0 => 40,
                1 => 60,
                _ => 80,
            };
            let row_start = y as usize * stride as usize;
            frame.data[row_start] = marker;
            frame.data[row_start + 1] = 128;
            frame.data[row_start + 2] = marker;
            frame.data[row_start + 3] = 128;
        }

        let actual = frame.pixel_at(0, 2).expect("supported").expect("in bounds");
        assert_eq!(actual, yuv_to_rgb(80, 128, 128));
    }

    #[test]
    fn test_pixel_at_uses_stride_not_width_for_row_offset_frameref() {
        let width = 4u32;
        let height = 3u32;
        let stride = width * 2 + 8; // padded stride (16 vs natural 8)
        let size = stride * height;
        let layout = FrameLayout::new(width, height, FourCC::YUYV, stride, size)
            .expect("valid padded YUYV layout");

        let mut data = vec![0u8; size as usize];
        // Write per-row distinguishable YUYV pairs so a buggy y*width*bpp offset reads the
        // wrong marker: row 0→40, row 1→60, row 2→80.
        for y in 0..height {
            let marker: u8 = match y {
                0 => 40,
                1 => 60,
                _ => 80,
            };
            let row_start = y as usize * stride as usize;
            data[row_start] = marker;
            data[row_start + 1] = 128;
            data[row_start + 2] = marker;
            data[row_start + 3] = 128;
        }

        let frame_ref = FrameRef {
            data: &data,
            metadata: FrameMetadata {
                sequence: 0,
                timestamp: std::time::Duration::ZERO,
                bytes_used: size,
            },
            layout,
        };

        // A buggy y*width*bpp row offset would read row 1's marker (60) instead of row 2's (80).
        let actual = frame_ref.pixel_at(0, 2).expect("supported").expect("in bounds");
        assert_eq!(actual, yuv_to_rgb(80, 128, 128));
    }

    #[test]
    fn test_pixel_at_matches_existing_behaviour_when_stride_equals_width_times_bpp() {
        let frame = make_yuyv_frame(4, 4, 8, 100, 128, 200, 128);
        assert_eq!(
            frame.pixel_at(1, 0).expect("supported"),
            Some(yuv_to_rgb(200, 128, 128))
        );
    }

    #[test]
    fn test_pixel_at_rgb3_uses_stride_for_row_offset() {
        let width = 3;
        let height = 3;
        let stride = width * 3 + 5;
        let size = stride * height;
        let layout =
            FrameLayout::new(width, height, FourCC::RGB3, stride, size).expect("valid RGB3 layout");
        let mut data = vec![0u8; size as usize];

        for y in 0..height {
            let marker = match y {
                0 => 30,
                1 => 60,
                _ => 90,
            };
            let row_start = y as usize * stride as usize;
            data[row_start..row_start + 3].copy_from_slice(&[marker, marker + 1, marker + 2]);
        }

        let frame = Frame::new(data, metadata(size as usize), layout);
        let actual = frame.pixel_at(0, 2).expect("supported").expect("in bounds");
        assert_eq!(actual, (90, 91, 92));
    }

    #[test]
    fn test_pixel_at_rgb3_uses_stride_for_row_offset_frameref() {
        let width = 3;
        let height = 2;
        let stride = width * 3 + 7;
        let size = stride * height;
        let layout =
            FrameLayout::new(width, height, FourCC::RGB3, stride, size).expect("valid RGB3 layout");
        let mut data = vec![0u8; size as usize];
        let second_row = stride as usize;
        data[second_row..second_row + 3].copy_from_slice(&[77, 78, 79]);
        let frame_ref = FrameRef {
            data: &data,
            metadata: metadata(size as usize),
            layout,
        };

        let actual = frame_ref
            .pixel_at(0, 1)
            .expect("supported")
            .expect("in bounds");
        assert_eq!(actual, (77, 78, 79));
    }

    #[test]
    fn test_pixel_at_returns_unsupported_error_for_nv12() {
        let layout = FrameLayout::new(4, 4, FourCC::NV12, 4, 24).expect("valid NV12 layout");
        let frame = Frame::new(vec![0; 24], metadata(24), layout);
        assert_eq!(
            frame.pixel_at(0, 0).unwrap_err(),
            FrameError::UnsupportedPixelAccess(FourCC::NV12)
        );
    }

    #[test]
    fn test_pixel_at_returns_unsupported_error_for_mjpg() {
        let layout = FrameLayout::new(4, 4, FourCC::MJPG, 0, 128).expect("valid MJPG layout");
        let frame = Frame::new(vec![0; 128], metadata(128), layout);
        assert_eq!(
            frame.pixel_at(0, 0).unwrap_err(),
            FrameError::UnsupportedPixelAccess(FourCC::MJPG)
        );
    }

    #[test]
    fn test_pixel_at_returns_unsupported_error_for_nv12_frameref() {
        let layout = FrameLayout::new(4, 4, FourCC::NV12, 4, 24).expect("valid NV12 layout");
        let data = vec![0; 24];
        let frame_ref = FrameRef {
            data: &data,
            metadata: metadata(24),
            layout,
        };
        assert_eq!(
            frame_ref.pixel_at(0, 0).unwrap_err(),
            FrameError::UnsupportedPixelAccess(FourCC::NV12)
        );
    }

    #[test]
    fn test_pixel_at_returns_unsupported_error_for_mjpg_frameref() {
        let layout = FrameLayout::new(4, 4, FourCC::MJPG, 0, 128).expect("valid MJPG layout");
        let data = vec![0; 128];
        let frame_ref = FrameRef {
            data: &data,
            metadata: metadata(128),
            layout,
        };
        assert_eq!(
            frame_ref.pixel_at(0, 0).unwrap_err(),
            FrameError::UnsupportedPixelAccess(FourCC::MJPG)
        );
    }
}
