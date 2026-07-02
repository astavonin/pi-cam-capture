//! Mock device implementation for testing without hardware.

use crate::error::Result;
use crate::traits::{
    BorrowedCaptureStream, CameraDevice, CaptureStream, DeviceCapabilities, Format, FourCC, Frame,
    FrameMetadata, FrameRef,
};

/// Mock device for testing without hardware.
pub struct MockDevice {
    capabilities: DeviceCapabilities,
    format: Format,
    /// Monotonically increasing frame counter shared across streams from this device.
    pub(crate) frame_count: u32,
    /// When `Some(n)`, the stream created by this device will inject a `CaptureFailed` error
    /// after `n` successful frames. Forwarded to `MockStream::error_after` on `create_stream`.
    error_after: Option<u32>,
}

impl Default for MockDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl MockDevice {
    /// Create a new mock device with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            capabilities: DeviceCapabilities {
                driver: "mock".to_owned(),
                card: "Mock Camera".to_owned(),
                bus_info: "mock:0".to_owned(),
                can_capture: true,
                can_stream: true,
            },
            format: Format::new(640, 480, FourCC::YUYV),
            frame_count: 0,
            error_after: None,
        }
    }

    /// Set the format for this mock device.
    #[must_use]
    pub const fn with_format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Set the capabilities for this mock device.
    #[must_use]
    pub fn with_capabilities(mut self, capabilities: DeviceCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Configure the stream created by this device to inject a `CaptureFailed` error after `n`
    /// successful frames. Useful for testing error propagation through `CaptureStream` guards.
    #[must_use]
    pub const fn with_error_after(mut self, n: u32) -> Self {
        self.error_after = Some(n);
        self
    }
}

impl CameraDevice for MockDevice {
    type Stream<'a> = MockStream<'a>;

    fn capabilities(&self) -> &DeviceCapabilities {
        &self.capabilities
    }

    fn format(&self) -> Result<Format> {
        Ok(self.format.clone())
    }

    fn set_format(&mut self, format: &Format) -> Result<Format> {
        self.format = format.clone();
        Ok(self.format.clone())
    }

    fn create_stream(&mut self, buffer_count: u32, fps: u32) -> Result<Self::Stream<'_>> {
        let error_after = self.error_after;
        Ok(MockStream {
            device: self,
            pattern: TestPattern::ColorBars,
            fps,
            buffer_count,
            current_frame: Vec::new(),
            error_after,
        })
    }
}

/// Test pattern types for mock frame generation.
#[derive(Debug, Clone, Copy)]
pub enum TestPattern {
    /// SMPTE color bars pattern.
    ColorBars,
    /// Horizontal gradient from dark to light.
    Gradient,
    /// Solid color with specified Y, U, V values.
    Solid(u8, u8, u8),
}

/// Mock capture stream for testing.
pub struct MockStream<'a> {
    device: &'a mut MockDevice,
    pattern: TestPattern,
    fps: u32,
    buffer_count: u32,
    current_frame: Vec<u8>,
    /// Injects a `CaptureFailed` error after this many successful frames. `None` means no error.
    error_after: Option<u32>,
}

impl MockStream<'_> {
    /// Set the test pattern for frame generation.
    #[must_use]
    pub const fn with_pattern(mut self, pattern: TestPattern) -> Self {
        self.pattern = pattern;
        self
    }

    /// Returns the fps this stream was created with.
    #[must_use]
    pub const fn fps(&self) -> u32 {
        self.fps
    }

    /// Returns the `buffer_count` this stream was created with.
    #[must_use]
    pub const fn buffer_count(&self) -> u32 {
        self.buffer_count
    }

    /// Returns the actual fps (same as requested — mock always honours the request).
    // same_name_method: intentional — this const inherent method avoids requiring callers
    // to import the CaptureStream trait for a simple FPS query.
    #[must_use]
    #[allow(clippy::same_name_method)]
    pub const fn actual_fps(&self) -> u32 {
        self.fps
    }

    /// Configures the stream to return a `CaptureFailed` error after `n` successful frames.
    ///
    /// Useful for testing error propagation through `CaptureStream` guards.
    #[must_use]
    pub const fn with_error_after(mut self, n: u32) -> Self {
        self.error_after = Some(n);
        self
    }

    /// Decrements the error counter and returns an error when it reaches zero.
    fn check_error(&mut self) -> Result<()> {
        if let Some(ref mut remaining) = self.error_after {
            if *remaining == 0 {
                return Err(
                    crate::error::StreamError::CaptureFailed("injected error".to_owned()).into(),
                );
            }
            *remaining -= 1;
        }
        Ok(())
    }

    fn next_metadata(&mut self, bytes_used: u32) -> FrameMetadata {
        let seq = self.device.frame_count;
        self.device.frame_count += 1;

        let millis_per_frame = if self.fps > 0 {
            1000 / u64::from(self.fps)
        } else {
            33 // fallback: ~30 fps
        };

        FrameMetadata {
            sequence: seq,
            timestamp: std::time::Duration::from_millis(u64::from(seq) * millis_per_frame),
            bytes_used,
        }
    }
}

impl CaptureStream for MockStream<'_> {
    fn actual_fps(&self) -> u32 {
        self.fps
    }

    fn next_frame(&mut self) -> Result<Frame> {
        self.check_error()?;
        let format = &self.device.format;
        let data = generate_test_frame(format, self.pattern);
        // Use actual data length so bytes_used is correct even for MJPG (where format.size == 0)
        #[allow(clippy::cast_possible_truncation)] // frame sizes fit in u32 in practice
        let bytes_used = data.len() as u32;
        let metadata = self.next_metadata(bytes_used);

        Ok(Frame { data, metadata })
    }
}

impl BorrowedCaptureStream for MockStream<'_> {
    fn next_frame_ref(&mut self) -> Result<FrameRef<'_>> {
        self.check_error()?;
        let format = &self.device.format;
        write_test_frame(&mut self.current_frame, format, self.pattern);
        // Use actual buffer length so bytes_used is correct even for MJPG (where format.size == 0)
        #[allow(clippy::cast_possible_truncation)] // frame sizes fit in u32 in practice
        let bytes_used = self.current_frame.len() as u32;
        let metadata = self.next_metadata(bytes_used);

        Ok(FrameRef {
            data: &self.current_frame,
            metadata,
        })
    }
}

/// Generate test frame data based on pattern.
pub(crate) fn generate_test_frame(format: &Format, pattern: TestPattern) -> Vec<u8> {
    let size = (format.width * format.height * 2) as usize; // YUYV = 2 bytes/pixel
    let mut data = vec![0u8; size];
    fill_test_frame(&mut data, format, pattern);
    data
}

fn write_test_frame(data: &mut Vec<u8>, format: &Format, pattern: TestPattern) {
    let size = (format.width * format.height * 2) as usize; // YUYV = 2 bytes/pixel
    data.clear();
    data.resize(size, 0);
    fill_test_frame(data, format, pattern);
}

fn fill_test_frame(data: &mut [u8], format: &Format, pattern: TestPattern) {
    match pattern {
        TestPattern::ColorBars => {
            generate_color_bars(data, format.width, format.height);
        }
        TestPattern::Gradient => {
            generate_gradient(data, format.width, format.height);
        }
        TestPattern::Solid(y, u, v) => {
            generate_solid(data, y, u, v);
        }
    }
}

/// Write a YUYV pixel pair `[Y0, U, Y1, V]` at `offset` in `data`, if in bounds.
fn write_yuyv(data: &mut [u8], offset: usize, y0: u8, u: u8, y1: u8, v: u8) {
    if let Some(chunk) = data.get_mut(offset..offset + 4) {
        chunk.copy_from_slice(&[y0, u, y1, v]);
    }
}

/// Generate YUYV color bars pattern.
fn generate_color_bars(data: &mut [u8], width: u32, height: u32) {
    // SMPTE EG 1-1990 100% color bar YUV values (YUYV packed)
    // 8 color bars: White, Yellow, Cyan, Green, Magenta, Red, Blue, Black
    let bars: [(u8, u8, u8); 8] = [
        (235, 128, 128), // White
        (210, 16, 146),  // Yellow
        (170, 166, 16),  // Cyan
        (145, 54, 34),   // Green
        (106, 202, 222), // Magenta
        (81, 90, 240),   // Red
        (41, 240, 110),  // Blue
        (16, 128, 128),  // Black
    ];

    // Guard: width < 8 maps all pixels to bar 0 instead of dividing by zero
    let bar_width = (width / 8).max(1);

    for y in 0..height {
        for x in (0..width).step_by(2) {
            let bar_idx = (x / bar_width).min(7) as usize;
            // bars has exactly 8 elements; bar_idx is clamped to 0..=7
            let (y_val, u_val, v_val) = bars.get(bar_idx).copied().unwrap_or((16, 128, 128));
            let offset = ((y * width + x) * 2) as usize;
            write_yuyv(data, offset, y_val, u_val, y_val, v_val);
        }
    }
}

/// Generate YUYV horizontal gradient pattern.
fn generate_gradient(data: &mut [u8], width: u32, height: u32) {
    for y in 0..height {
        for x in (0..width).step_by(2) {
            #[allow(clippy::cast_possible_truncation)]
            let y_val = ((x * 255) / width) as u8;
            let offset = ((y * width + x) * 2) as usize;
            write_yuyv(data, offset, y_val, 128, y_val, 128);
        }
    }
}

/// Generate solid color YUYV frame.
fn generate_solid(data: &mut [u8], y: u8, u: u8, v: u8) {
    for chunk in data.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[y, u, y, v]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_device_creation() {
        let device = MockDevice::new();
        assert_eq!(device.capabilities().driver, "mock");
        assert!(device.capabilities().can_capture);
        assert!(device.capabilities().can_stream);
    }

    #[test]
    fn test_mock_device_format() {
        let mut device = MockDevice::new();
        let format = device.format().expect("format should succeed");
        assert_eq!(format.width, 640);
        assert_eq!(format.height, 480);

        let new_format = Format::new(1280, 720, FourCC::YUYV);
        let actual = device
            .set_format(&new_format)
            .expect("set_format should succeed");
        assert_eq!(actual.width, 1280);
        assert_eq!(actual.height, 720);
    }

    #[test]
    fn test_mock_stream_capture() {
        let mut device = MockDevice::new();
        let mut stream = device
            .create_stream(4, 30)
            .expect("create_stream should succeed");

        let frame1 = stream.next_frame().expect("next_frame should succeed");
        assert_eq!(frame1.metadata.sequence, 0);
        assert!(!frame1.data.is_empty());

        let frame2 = stream.next_frame().expect("next_frame should succeed");
        assert_eq!(frame2.metadata.sequence, 1);
    }

    #[test]
    fn test_color_bars_pattern() {
        let format = Format::new(640, 480, FourCC::YUYV);
        let data = generate_test_frame(&format, TestPattern::ColorBars);

        // Check frame size
        assert_eq!(data.len(), (640 * 480 * 2) as usize);

        // First bar should be white (Y=235)
        assert_eq!(data[0], 235);
    }

    #[test]
    fn test_gradient_pattern() {
        let format = Format::new(640, 480, FourCC::YUYV);
        let data = generate_test_frame(&format, TestPattern::Gradient);

        // Left edge should be dark
        assert!(data[0] < 10);

        // Right edge should be bright (check last row, last pixel)
        let last_row_start = (479 * 640 * 2) as usize;
        let last_pixel_y = data[last_row_start + 638 * 2];
        assert!(last_pixel_y > 200);
    }

    #[test]
    fn test_solid_pattern() {
        let format = Format::new(64, 64, FourCC::YUYV);
        let data = generate_test_frame(&format, TestPattern::Solid(128, 64, 192));

        // All Y values should be 128
        assert_eq!(data[0], 128);
        assert_eq!(data[2], 128);

        // U should be 64, V should be 192
        assert_eq!(data[1], 64);
        assert_eq!(data[3], 192);
    }

    #[test]
    fn test_mock_stream_fps_timestamp() {
        let mut device = MockDevice::new();
        let mut stream = device.create_stream(4, 60).expect("create_stream failed");
        let f1 = stream.next_frame().expect("next_frame failed");
        let f2 = stream.next_frame().expect("next_frame failed");
        // 60 FPS → ~16ms per frame
        let delta = f2.metadata.timestamp - f1.metadata.timestamp;
        assert_eq!(delta.as_millis(), 1000 / 60);
    }

    #[test]
    fn test_mock_stream_buffer_count_accessible() {
        let mut device = MockDevice::new();
        let stream = device.create_stream(8, 30).expect("create_stream failed");
        assert_eq!(stream.buffer_count(), 8);
        assert_eq!(stream.fps(), 30);
    }

    #[test]
    fn test_mock_stream_borrowed_capture_reuses_scratch_buffer() {
        let mut device = MockDevice::new();
        let expected_size = device.format.size;
        let mut stream = device.create_stream(4, 30).expect("create_stream failed");

        let first_ptr = {
            let frame = stream.next_frame_ref().expect("next_frame_ref failed");
            assert_eq!(frame.metadata.sequence, 0);
            assert_eq!(frame.metadata.bytes_used, expected_size);
            assert_eq!(frame.data.len(), expected_size as usize);
            frame.data.as_ptr()
        };

        let second_ptr = {
            let frame = stream
                .next_frame_ref()
                .expect("second next_frame_ref failed");
            assert_eq!(frame.metadata.sequence, 1);
            assert_eq!(frame.metadata.bytes_used, expected_size);
            frame.data.as_ptr()
        };

        assert_eq!(first_ptr, second_ptr);
    }

    #[test]
    fn test_color_bars_narrow_frame() {
        // width=4 < 8 — must not panic (bar_width would be 0 without the guard)
        let format = Format::new(4, 4, FourCC::YUYV);
        let data = generate_test_frame(&format, TestPattern::ColorBars);
        assert_eq!(data.len(), (4 * 4 * 2) as usize);
    }

    // Error injection via with_error_after propagates through next_frame
    #[test]
    fn test_mock_stream_error_injection_next_frame() {
        let mut device = MockDevice::new();
        let mut stream = device
            .create_stream(4, 30)
            .expect("create_stream failed")
            .with_error_after(1);

        let _ = stream.next_frame().expect("first frame should succeed");
        let result = stream.next_frame();
        assert!(
            matches!(
                result.unwrap_err(),
                crate::error::CaptureError::Stream(crate::error::StreamError::CaptureFailed(ref msg))
                if msg == "injected error"
            ),
            "expected StreamError::CaptureFailed(\"injected error\")"
        );
    }

    // Error injection via with_error_after propagates through next_frame_ref
    #[test]
    fn test_mock_stream_error_injection_next_frame_ref() {
        let mut device = MockDevice::new();
        let mut stream = device
            .create_stream(4, 30)
            .expect("create_stream failed")
            .with_error_after(1);

        {
            let _frame = stream.next_frame_ref().expect("first frame should succeed");
        }
        let result = stream.next_frame_ref();
        assert!(
            matches!(
                result.unwrap_err(),
                crate::error::CaptureError::Stream(crate::error::StreamError::CaptureFailed(ref msg))
                if msg == "injected error"
            ),
            "expected StreamError::CaptureFailed(\"injected error\")"
        );
    }

    // bytes_used reflects actual frame data length, not format.size (which is 0 for MJPG)
    #[test]
    fn test_mock_stream_bytes_used_reflects_actual_data_length() {
        let mut device = MockDevice::new().with_format(Format::new(64, 64, FourCC::YUYV));
        let mut stream = device.create_stream(4, 30).expect("create_stream failed");

        let frame = stream.next_frame().expect("next_frame failed");
        assert_eq!(
            frame.metadata.bytes_used as usize,
            frame.data.len(),
            "bytes_used must match actual data length"
        );
        assert!(frame.metadata.bytes_used > 0);
    }

    // MJPG format has format.size == 0; bytes_used must still reflect actual data length
    #[test]
    fn test_mock_stream_bytes_used_nonzero_for_mjpg_format() {
        let mut device = MockDevice::new().with_format(Format::new(64, 64, FourCC::MJPG));
        let mut stream = device.create_stream(4, 30).expect("create_stream failed");

        let frame = stream.next_frame().expect("next_frame failed");
        assert!(
            frame.metadata.bytes_used > 0,
            "bytes_used must be positive even for MJPG format"
        );

        {
            let frame_ref = stream.next_frame_ref().expect("next_frame_ref failed");
            assert!(
                frame_ref.metadata.bytes_used > 0,
                "borrowed bytes_used must be positive for MJPG"
            );
        }
    }
}
