//! Stream management and FPS control.

use v4l::buffer::Type;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream as V4lCaptureStream;
use v4l::video::Capture;
use v4l::Device;

use crate::error::{Result, StreamError};
use crate::traits::{CaptureStream, Frame, FrameMetadata};
use std::time::Duration;

/// V4L2 capture stream wrapping mmap-based streaming with FPS control.
pub struct V4L2Stream<'a> {
    stream: Stream<'a>,
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
        // Create the mmap stream
        let stream = Stream::with_buffers(device, Type::VideoCapture, buffer_count)
            .map_err(|err| StreamError::StartFailed(err.to_string()))?;

        // Set FPS via V4L2 stream parameters
        let actual_fps = Self::set_fps(device, target_fps)?;

        // Log if the driver negotiated a different FPS
        if actual_fps == target_fps {
            log::info!("Stream configured for {actual_fps} FPS");
        } else {
            log::warn!("Requested {target_fps} FPS, but driver set {actual_fps} FPS");
        }

        Ok(Self { stream, actual_fps })
    }

    /// Get the actual FPS set by the driver.
    #[must_use]
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
        } else {
            actual_params.interval.denominator / actual_params.interval.numerator
        };

        let numerator = actual_params.interval.numerator;
        let denominator = actual_params.interval.denominator;
        log::debug!("Driver set interval = {numerator}/{denominator} ({actual_fps} FPS)");

        Ok(actual_fps)
    }
}

impl CaptureStream for V4L2Stream<'_> {
    fn next_frame(&mut self) -> Result<Frame> {
        let (buf, meta) = self
            .stream
            .next()
            .map_err(|err| StreamError::CaptureFailed(err.to_string()))?;

        // Safe conversions: V4L2 timestamps are always non-negative in practice
        #[allow(clippy::cast_sign_loss)]
        let secs = meta.timestamp.sec.max(0) as u64;
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let nanos = (meta.timestamp.usec.max(0) as u32).saturating_mul(1000);

        Ok(Frame {
            data: buf.to_vec(),
            metadata: FrameMetadata {
                sequence: meta.sequence,
                timestamp: Duration::new(secs, nanos),
                bytes_used: meta.bytesused,
            },
        })
    }
}
