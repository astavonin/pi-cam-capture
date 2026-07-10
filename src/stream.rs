//! Stream management and FPS control.

use v4l::buffer::Type;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream as V4lCaptureStream;
use v4l::video::Capture;
use v4l::Device;

use crate::error::{Result, StreamError};
use crate::traits::{
    BorrowedCaptureStream, CaptureStream, Format, FourCC, Frame, FrameLayout, FrameMetadata,
    FrameRef,
};
use std::time::Duration;

/// V4L2 capture stream wrapping mmap-based streaming with FPS control.
pub struct V4L2Stream<'a> {
    stream: Stream<'a>,
    layout: FrameLayout,
    actual_fps: u32,
}

impl<'a> V4L2Stream<'a> {
    /// Create a new stream with the specified buffer count and FPS.
    ///
    /// # Arguments
    ///
    /// * `device` - The V4L2 device to create the stream for
    /// * `buffer_count` - Number of mmap buffers (2-8 recommended)
    /// * `target_fps` - Desired frames per second
    ///
    /// # Errors
    ///
    /// Returns `StreamError` if stream creation or FPS setting fails.
    pub fn new(device: &'a Device, buffer_count: u32, target_fps: u32) -> Result<Self> {
        // Set FPS before allocating mmap buffers; some drivers ignore S_PARM after REQBUFS.
        let actual_fps = Self::set_fps(device, target_fps)?;

        let fmt = device
            .format()
            .map_err(|err| StreamError::GetFormatFailed(err.to_string()))?;
        let layout = FrameLayout::from_format(&Format {
            width: fmt.width,
            height: fmt.height,
            fourcc: FourCC::from(fmt.fourcc),
            stride: fmt.stride,
            size: fmt.size,
        })
        .map_err(|err| StreamError::StartFailed(format!("Invalid negotiated layout: {err}")))?;

        // Create the mmap stream
        let stream = Stream::with_buffers(device, Type::VideoCapture, buffer_count)
            .map_err(|err| StreamError::StartFailed(err.to_string()))?;

        // Log if the driver negotiated a different FPS
        if actual_fps == target_fps {
            log::info!("Stream configured for {actual_fps} FPS");
        } else {
            log::warn!("Requested {target_fps} FPS, but driver set {actual_fps} FPS");
        }

        Ok(Self {
            stream,
            layout,
            actual_fps,
        })
    }

    /// Get the actual FPS set by the driver.
    // same_name_method: intentional — this const inherent method is more efficient than the
    // trait's non-const version and avoids requiring callers to import the trait.
    #[must_use]
    #[allow(clippy::same_name_method)]
    pub const fn actual_fps(&self) -> u32 {
        self.actual_fps
    }

    /// Set the stream FPS via V4L2 streaming parameters.
    ///
    /// # Arguments
    ///
    /// * `device` - The V4L2 device
    /// * `target_fps` - Desired frames per second
    ///
    /// # Returns
    ///
    /// The actual FPS set by the driver (may differ from target).
    ///
    /// # Errors
    ///
    /// Returns `StreamError` if setting parameters fails.
    fn set_fps(device: &Device, target_fps: u32) -> Result<u32> {
        // Get current stream parameters
        let mut params = device
            .params()
            .map_err(|err| StreamError::StartFailed(format!("Failed to get params: {err}")))?;

        // Set desired frame interval (1/fps seconds per frame)
        // V4L2 uses a fraction: timeperframe = numerator/denominator
        // For FPS, we want 1/fps, so numerator=1, denominator=fps
        params.interval = v4l::Fraction::new(1, target_fps);

        log::debug!("Setting stream parameters: interval = 1/{target_fps} ({target_fps} FPS)");

        // Apply to driver
        let actual_params = device
            .set_params(&params)
            .map_err(|err| StreamError::StartFailed(format!("Failed to set params: {err}")))?;

        // Calculate actual FPS from returned interval
        // actual_fps = denominator/numerator
        let actual_fps = if actual_params.interval.numerator == 0 {
            log::warn!("Driver returned invalid interval (numerator=0), using target FPS");
            target_fps
        } else if actual_params.interval.denominator == 0 {
            log::warn!("Driver returned invalid interval (denominator=0), using target FPS");
            target_fps
        } else {
            actual_params.interval.denominator / actual_params.interval.numerator
        };

        let numerator = actual_params.interval.numerator;
        let denominator = actual_params.interval.denominator;
        log::debug!("Driver set interval = {numerator}/{denominator} ({actual_fps} FPS)");

        Ok(actual_fps)
    }

    /// Parse a V4L2 timestamp into a `Duration`, rejecting malformed `usec` values.
    ///
    /// # Errors
    ///
    /// Returns `StreamError::CaptureFailed` if `usec` is outside `[0, 1_000_000)`.
    pub(crate) fn parse_timestamp(sec: i64, usec: i64) -> Result<Duration> {
        if !(0..1_000_000).contains(&usec) {
            return Err(StreamError::CaptureFailed(format!(
                "Driver returned invalid timestamp usec={usec}"
            ))
            .into());
        }
        // sec.max(0) guards against a driver returning a negative second — treat as zero
        #[allow(clippy::cast_sign_loss)] // guarded by .max(0)
        let secs = sec.max(0) as u64;
        // usec in [0, 1_000_000) fits in u32 and has no fractional part when * 1000
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // range checked above
        let nanos = usec as u32 * 1000;
        Ok(Duration::new(secs, nanos))
    }
}

impl CaptureStream for V4L2Stream<'_> {
    fn actual_fps(&self) -> u32 {
        self.actual_fps
    }

    fn next_frame(&mut self) -> Result<Frame> {
        let (buf, meta) = self
            .stream
            .next()
            .map_err(|err| StreamError::CaptureFailed(err.to_string()))?;

        let timestamp = Self::parse_timestamp(meta.timestamp.sec, meta.timestamp.usec)?;

        Ok(Frame {
            data: buf.to_vec(),
            metadata: FrameMetadata {
                sequence: meta.sequence,
                timestamp,
                bytes_used: meta.bytesused,
            },
            layout: self.layout,
        })
    }
}

impl BorrowedCaptureStream for V4L2Stream<'_> {
    fn next_frame_ref(&mut self) -> Result<FrameRef<'_>> {
        let (buf, meta) = self
            .stream
            .next()
            .map_err(|err| StreamError::CaptureFailed(err.to_string()))?;

        let timestamp = Self::parse_timestamp(meta.timestamp.sec, meta.timestamp.usec)?;

        Ok(FrameRef {
            data: buf,
            metadata: FrameMetadata {
                sequence: meta.sequence,
                timestamp,
                bytes_used: meta.bytesused,
            },
            layout: self.layout,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamp_valid() {
        let ts = V4L2Stream::parse_timestamp(1, 500_000).expect("valid timestamp");
        assert_eq!(ts, Duration::new(1, 500_000_000));
    }

    #[test]
    fn test_parse_timestamp_usec_at_boundary() {
        assert!(V4L2Stream::parse_timestamp(0, 999_999).is_ok());
        let err = V4L2Stream::parse_timestamp(0, 1_000_000).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("usec=1000000"),
            "expected usec value in error message: {err_str}"
        );
        let err2 = V4L2Stream::parse_timestamp(0, -1).unwrap_err();
        let err_str2 = err2.to_string();
        assert!(
            err_str2.contains("usec=-1"),
            "expected usec value in error message: {err_str2}"
        );
    }

    #[test]
    fn test_parse_timestamp_negative_sec_clamped() {
        let ts = V4L2Stream::parse_timestamp(-5, 0).expect("valid timestamp");
        assert_eq!(ts, Duration::ZERO);
    }
}
