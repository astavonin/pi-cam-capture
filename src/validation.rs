//! Frame validation utilities for test pattern verification.
//!
//! This module provides functions to validate that captured frames contain
//! expected test patterns. Useful for integration testing with virtual cameras.

use crate::traits::{CameraError, Format, Frame, Result};

/// Expected RGB values for SMPTE color bars (8 bars).
///
/// These are the RGB values resulting from converting the YUV values
/// used by the mock device's color bar pattern.
///
/// Colors in order: White, Yellow, Cyan, Green, Magenta, Red, Blue, Black
const SMPTE_COLOR_BARS: [(u8, u8, u8); 8] = [
    (235, 235, 235), // White
    (235, 235, 11),  // Yellow
    (12, 236, 237),  // Cyan
    (13, 237, 13),   // Green
    (237, 13, 237),  // Magenta
    (238, 14, 13),   // Red
    (15, 15, 239),   // Blue
    (16, 16, 16),    // Black
];

/// Tolerance for RGB color matching (accounts for YUV->RGB conversion errors).
const COLOR_TOLERANCE: i32 = 15;

/// Validates that a frame contains the SMPTE color bar pattern.
///
/// This function checks 8 vertical stripes at their center positions,
/// verifying that each stripe contains the expected color with a tolerance
/// for YUV-to-RGB conversion inaccuracies.
///
/// # Arguments
///
/// * `frame` - The frame to validate
/// * `format` - The frame format (contains width and height)
///
/// # Returns
///
/// * `Ok(())` if the color bars are valid
/// * `Err(CameraError::StreamError)` if validation fails
///
/// # Errors
///
/// Returns `StreamError` if:
/// - The frame dimensions don't match the format
/// - Any color bar doesn't match the expected color within tolerance
pub fn validate_color_bars(frame: &Frame, format: &Format) -> Result<()> {
    let width = format.width;
    let height = format.height;
    let bar_width = width / 8;
    let center_y = height / 2;

    for (bar_idx, expected_rgb) in SMPTE_COLOR_BARS.iter().enumerate() {
        // Sample the center of each bar
        #[allow(clippy::cast_possible_truncation)]
        let sample_x = (bar_idx as u32 * bar_width) + (bar_width / 2);

        let actual_rgb = frame.pixel_at(sample_x, center_y, width).ok_or_else(|| {
            CameraError::StreamError(format!(
                "Failed to get pixel at ({sample_x}, {center_y})"
            ))
        })?;

        if !colors_match(actual_rgb, *expected_rgb, COLOR_TOLERANCE) {
            return Err(CameraError::StreamError(format!(
                "Color bar {bar_idx} mismatch at ({sample_x}, {center_y}): \
                 expected RGB{expected_rgb:?}, got RGB{actual_rgb:?}"
            )));
        }
    }

    Ok(())
}

/// Validates that a frame contains a horizontal gradient pattern.
///
/// This function samples a horizontal line at the center of the frame and
/// verifies that the luminance increases monotonically from left to right.
/// It also checks that there is a significant overall luminance change
/// across the frame (not a solid color).
///
/// # Arguments
///
/// * `frame` - The frame to validate
/// * `format` - The frame format (contains width and height)
///
/// # Returns
///
/// * `Ok(())` if the gradient is valid
/// * `Err(CameraError::StreamError)` if validation fails
///
/// # Errors
///
/// Returns `StreamError` if:
/// - The frame dimensions don't match the format
/// - The luminance doesn't increase monotonically
/// - The total luminance change is too small (solid color)
pub fn validate_gradient(frame: &Frame, format: &Format) -> Result<()> {
    let width = format.width;
    let height = format.height;
    let center_y = height / 2;

    // Sample every 10 pixels to check for monotonic increase
    let sample_step = 10u32;
    let mut first_luminance: Option<f32> = None;
    let mut prev_luminance: Option<f32> = None;
    let mut last_luminance: Option<f32> = None;

    for x in (0..width).step_by(sample_step as usize) {
        let (r, g, b) = frame.pixel_at(x, center_y, width).ok_or_else(|| {
            CameraError::StreamError(format!("Failed to get pixel at ({x}, {center_y})"))
        })?;

        // Calculate luminance (Y' in Rec. 601)
        let luminance = 0.114f32.mul_add(
            f32::from(b),
            0.587f32.mul_add(f32::from(g), 0.299 * f32::from(r)),
        );

        if first_luminance.is_none() {
            first_luminance = Some(luminance);
        }

        if let Some(prev) = prev_luminance {
            if luminance < prev - 1.0 {
                // Allow small decreases due to rounding
                return Err(CameraError::StreamError(format!(
                    "Gradient not monotonically increasing at x={x}: \
                     luminance {luminance} < previous {prev}"
                )));
            }
        }

        prev_luminance = Some(luminance);
        last_luminance = Some(luminance);
    }

    // Check that there's a significant luminance change across the frame
    if let (Some(first), Some(last)) = (first_luminance, last_luminance) {
        let luminance_change = last - first;
        if luminance_change < 50.0 {
            return Err(CameraError::StreamError(format!(
                "Insufficient luminance change for gradient: {luminance_change} \
                 (expected at least 50.0)"
            )));
        }
    }

    Ok(())
}

/// Validates that a sequence of frames has incrementing sequence numbers.
///
/// This function checks that frame sequence numbers increment by 1 with no gaps.
///
/// # Arguments
///
/// * `frames` - The frames to validate
///
/// # Returns
///
/// * `Ok(())` if the sequence is valid
/// * `Err(CameraError::StreamError)` if validation fails
///
/// # Errors
///
/// Returns `StreamError` if:
/// - The frames slice is empty
/// - Any sequence number doesn't increment by exactly 1 from the previous
pub fn validate_frame_sequence(frames: &[Frame]) -> Result<()> {
    if frames.is_empty() {
        return Err(CameraError::StreamError(
            "Cannot validate empty frame sequence".to_owned(),
        ));
    }

    for i in 1..frames.len() {
        let prev_frame = frames.get(i - 1).ok_or_else(|| {
            CameraError::StreamError(format!("Failed to get frame at index {}", i - 1))
        })?;
        let curr_frame = frames.get(i).ok_or_else(|| {
            CameraError::StreamError(format!("Failed to get frame at index {i}"))
        })?;

        let prev_seq = prev_frame.metadata.sequence;
        let curr_seq = curr_frame.metadata.sequence;

        if curr_seq != prev_seq + 1 {
            return Err(CameraError::StreamError(format!(
                "Frame sequence gap at index {i}: expected {}, got {curr_seq}",
                prev_seq + 1
            )));
        }
    }

    Ok(())
}

/// Helper function to check if two RGB colors match within a tolerance.
///
/// # Arguments
///
/// * `actual` - The actual RGB color
/// * `expected` - The expected RGB color
/// * `tolerance` - Maximum allowed difference per channel
///
/// # Returns
///
/// `true` if all three channels are within tolerance, `false` otherwise
fn colors_match(actual: (u8, u8, u8), expected: (u8, u8, u8), tolerance: i32) -> bool {
    let (ar, ag, ab) = actual;
    let (er, eg, eb) = expected;

    let r_diff = i32::from(ar).abs_diff(i32::from(er));
    let g_diff = i32::from(ag).abs_diff(i32::from(eg));
    let b_diff = i32::from(ab).abs_diff(i32::from(eb));

    #[allow(clippy::cast_sign_loss)]
    let tol = tolerance as u32;

    r_diff <= tol && g_diff <= tol && b_diff <= tol
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockDevice, TestPattern};
    use crate::traits::{CameraDevice, CaptureStream, FourCC};

    #[test]
    fn test_validate_color_bars_success() {
        let mut device = MockDevice::new();
        let format = Format::new(640, 480, FourCC::YUYV);
        device.set_format(&format).expect("set_format failed");

        let stream = device.create_stream(1).expect("create_stream failed");
        let mut stream = stream.with_pattern(TestPattern::ColorBars);
        let frame = stream.next_frame().expect("next_frame failed");

        let result = validate_color_bars(&frame, &format);
        assert!(
            result.is_ok(),
            "Color bars validation should succeed: {result:?}"
        );
    }

    #[test]
    fn test_validate_color_bars_wrong_pattern() {
        let mut device = MockDevice::new();
        let format = Format::new(640, 480, FourCC::YUYV);
        device.set_format(&format).expect("set_format failed");

        let stream = device.create_stream(1).expect("create_stream failed");
        let mut stream = stream.with_pattern(TestPattern::Gradient);
        let frame = stream.next_frame().expect("next_frame failed");

        let result = validate_color_bars(&frame, &format);
        assert!(
            result.is_err(),
            "Color bars validation should fail for gradient pattern"
        );
    }

    #[test]
    fn test_validate_gradient_success() {
        let mut device = MockDevice::new();
        let format = Format::new(640, 480, FourCC::YUYV);
        device.set_format(&format).expect("set_format failed");

        let stream = device.create_stream(1).expect("create_stream failed");
        let mut stream = stream.with_pattern(TestPattern::Gradient);
        let frame = stream.next_frame().expect("next_frame failed");

        let result = validate_gradient(&frame, &format);
        assert!(
            result.is_ok(),
            "Gradient validation should succeed: {result:?}"
        );
    }

    #[test]
    fn test_validate_gradient_wrong_pattern() {
        let mut device = MockDevice::new();
        let format = Format::new(640, 480, FourCC::YUYV);
        device.set_format(&format).expect("set_format failed");

        let stream = device.create_stream(1).expect("create_stream failed");
        let mut stream = stream.with_pattern(TestPattern::Solid(128, 128, 128));
        let frame = stream.next_frame().expect("next_frame failed");

        let result = validate_gradient(&frame, &format);
        assert!(
            result.is_err(),
            "Gradient validation should fail for solid pattern"
        );
    }

    #[test]
    fn test_validate_frame_sequence_success() {
        let mut device = MockDevice::new();
        let mut stream = device.create_stream(1).expect("create_stream failed");

        let frames: Vec<Frame> = (0..5)
            .map(|_| stream.next_frame().expect("next_frame failed"))
            .collect();

        let result = validate_frame_sequence(&frames);
        assert!(
            result.is_ok(),
            "Frame sequence validation should succeed: {result:?}"
        );
    }

    #[test]
    fn test_validate_frame_sequence_empty() {
        let frames: Vec<Frame> = vec![];
        let result = validate_frame_sequence(&frames);
        assert!(
            result.is_err(),
            "Frame sequence validation should fail for empty sequence"
        );
    }

    #[test]
    fn test_validate_frame_sequence_with_gap() {
        let mut device = MockDevice::new();
        let mut stream = device.create_stream(1).expect("create_stream failed");

        let mut frames = vec![
            stream.next_frame().expect("next_frame failed"),
            stream.next_frame().expect("next_frame failed"),
        ];

        // Skip a frame to create a gap
        let _ = stream.next_frame().expect("next_frame failed");

        frames.push(stream.next_frame().expect("next_frame failed"));

        let result = validate_frame_sequence(&frames);
        assert!(
            result.is_err(),
            "Frame sequence validation should fail with gap"
        );
    }

    #[test]
    fn test_colors_match_exact() {
        assert!(colors_match((100, 150, 200), (100, 150, 200), 10));
    }

    #[test]
    fn test_colors_match_within_tolerance() {
        assert!(colors_match((100, 150, 200), (105, 155, 205), 10));
    }

    #[test]
    fn test_colors_match_outside_tolerance() {
        assert!(!colors_match((100, 150, 200), (120, 150, 200), 10));
    }
}
