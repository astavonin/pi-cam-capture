//! Mock device implementation for testing without hardware.

use crate::traits::{
    CameraDevice, CaptureStream, DeviceCapabilities, Format, FourCC, Frame, FrameMetadata, Result,
};
use std::time::Duration;

/// Mock device for testing without hardware.
pub struct MockDevice {
    capabilities: DeviceCapabilities,
    format: Format,
    frame_count: u32,
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
        }
    }

    /// Set the format for this mock device.
    #[must_use]
    pub fn with_format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Set the capabilities for this mock device.
    #[must_use]
    pub fn with_capabilities(mut self, capabilities: DeviceCapabilities) -> Self {
        self.capabilities = capabilities;
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

    fn create_stream(&mut self, _buffer_count: u32) -> Result<Self::Stream<'_>> {
        Ok(MockStream {
            device: self,
            pattern: TestPattern::ColorBars,
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
}

impl MockStream<'_> {
    /// Set the test pattern for frame generation.
    #[must_use]
    pub fn with_pattern(mut self, pattern: TestPattern) -> Self {
        self.pattern = pattern;
        self
    }
}

impl CaptureStream for MockStream<'_> {
    fn next_frame(&mut self) -> Result<Frame> {
        let format = &self.device.format;
        let data = generate_test_frame(format, self.pattern);

        let seq = self.device.frame_count;
        self.device.frame_count += 1;

        Ok(Frame {
            data,
            metadata: FrameMetadata {
                sequence: seq,
                timestamp: Duration::from_millis(u64::from(seq) * 33), // ~30fps
                bytes_used: format.size,
            },
        })
    }
}

/// Generate test frame data based on pattern.
fn generate_test_frame(format: &Format, pattern: TestPattern) -> Vec<u8> {
    let size = (format.width * format.height * 2) as usize; // YUYV = 2 bytes/pixel
    let mut data = vec![0u8; size];

    match pattern {
        TestPattern::ColorBars => {
            generate_color_bars(&mut data, format.width, format.height);
        }
        TestPattern::Gradient => {
            generate_gradient(&mut data, format.width, format.height);
        }
        TestPattern::Solid(y, u, v) => {
            generate_solid(&mut data, y, u, v);
        }
    }

    data
}

/// Generate YUYV color bars pattern.
fn generate_color_bars(data: &mut [u8], width: u32, height: u32) {
    // 8 color bars: White, Yellow, Cyan, Green, Magenta, Red, Blue, Black
    // YUYV values for each bar
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

    let bar_width = width / 8;

    for y in 0..height {
        for x in (0..width).step_by(2) {
            let bar_idx = (x / bar_width).min(7) as usize;
            let (y_val, u_val, v_val) = bars[bar_idx];

            let offset = ((y * width + x) * 2) as usize;
            if offset + 3 < data.len() {
                data[offset] = y_val;     // Y0
                data[offset + 1] = u_val; // U
                data[offset + 2] = y_val; // Y1
                data[offset + 3] = v_val; // V
            }
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

            if offset + 3 < data.len() {
                data[offset] = y_val;     // Y0
                data[offset + 1] = 128;   // U (neutral)
                data[offset + 2] = y_val; // Y1
                data[offset + 3] = 128;   // V (neutral)
            }
        }
    }
}

/// Generate solid color YUYV frame.
fn generate_solid(data: &mut [u8], y: u8, u: u8, v: u8) {
    for i in (0..data.len()).step_by(4) {
        if i + 3 < data.len() {
            data[i] = y;     // Y0
            data[i + 1] = u; // U
            data[i + 2] = y; // Y1
            data[i + 3] = v; // V
        }
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
        let actual = device.set_format(&new_format).expect("set_format should succeed");
        assert_eq!(actual.width, 1280);
        assert_eq!(actual.height, 720);
    }

    #[test]
    fn test_mock_stream_capture() {
        let mut device = MockDevice::new();
        let mut stream = device.create_stream(4).expect("create_stream should succeed");

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
}
