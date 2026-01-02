//! High-level capture session API.
//!
//! This module provides a simplified interface for camera capture that handles
//! device initialization, format configuration, and stream management automatically.
//!
//! # Example
//!
//! ```no_run
//! use pi_cam_capture::{CaptureConfig, CaptureSession};
//!
//! # fn main() -> pi_cam_capture::Result<()> {
//! // Create configuration
//! let config = CaptureConfig::builder()
//!     .resolution(1280, 720)
//!     .fps(30)
//!     .build()?;
//!
//! // Create session (opens device, sets format)
//! let mut session = CaptureSession::new(config)?;
//!
//! // Start streaming
//! session.start_stream()?;
//!
//! // Capture frames
//! for _ in 0..10 {
//!     let frame = session.next_frame()?;
//!     println!("Frame {}: {} bytes", frame.metadata.sequence, frame.data.len());
//! }
//!
//! // Session automatically cleaned up on drop
//! # Ok(())
//! # }
//! ```

use crate::config::CaptureConfig;
use crate::device::V4L2Device;
use crate::error::Result;
use crate::traits::{CameraDevice, CaptureStream, Format, Frame};

/// High-level capture session that manages device and streaming.
///
/// `CaptureSession` provides a simple interface for camera capture by handling
/// all initialization and resource management automatically.
///
/// # Lifecycle
///
/// 1. Create: Opens device, sets format
/// 2. Start: Creates streaming buffers
/// 3. Capture: Call `next_frame()` repeatedly
/// 4. Cleanup: Automatic on drop
pub struct CaptureSession {
    device: V4L2Device,
    config: CaptureConfig,
    actual_format: Format,
    actual_fps: Option<u32>,
}

impl CaptureSession {
    /// Create a new capture session with the specified configuration.
    ///
    /// This method:
    /// 1. Opens the V4L2 device
    /// 2. Sets the requested format
    ///
    /// Call `start_stream()` to begin capturing frames.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pi_cam_capture::{CaptureConfig, CaptureSession};
    ///
    /// # fn main() -> pi_cam_capture::Result<()> {
    /// let config = CaptureConfig::default();
    /// let session = CaptureSession::new(config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: CaptureConfig) -> Result<Self> {
        // Open the device
        let mut device = V4L2Device::open(config.device_index)?;

        log::info!(
            "Opened device /dev/video{}: {}",
            config.device_index,
            device.capabilities().card
        );

        // Set the format
        let requested_format = Format::new(config.width, config.height, config.format);
        let actual_format = device.set_format(&requested_format)?;

        log::info!(
            "Set format: {}x{} {:?}",
            actual_format.width,
            actual_format.height,
            actual_format.fourcc
        );

        Ok(Self {
            device,
            config,
            actual_format,
            actual_fps: None,
        })
    }

    /// Start streaming and capture frames.
    ///
    /// This creates a stream with the configured buffer count and FPS.
    /// After calling this, you can use `next_frame()` to capture frames.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pi_cam_capture::{CaptureConfig, CaptureSession};
    ///
    /// # fn main() -> pi_cam_capture::Result<()> {
    /// let mut session = CaptureSession::new(CaptureConfig::default())?;
    /// session.start_stream()?;
    /// let frame = session.next_frame()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn start_stream(&mut self) -> Result<()> {
        let stream = self.device.create_stream(self.config.buffer_count, self.config.fps)?;
        self.actual_fps = Some(stream.actual_fps());

        log::info!(
            "Created stream with {} buffers at {} FPS",
            self.config.buffer_count,
            stream.actual_fps()
        );

        // Drop the stream - it's already started, we'll create new ones as needed
        Ok(())
    }

    /// Capture the next frame from the camera.
    ///
    /// This blocks until a frame is available or an error occurs.
    /// You must call `start_stream()` first.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pi_cam_capture::{CaptureConfig, CaptureSession};
    ///
    /// # fn main() -> pi_cam_capture::Result<()> {
    /// let mut session = CaptureSession::new(CaptureConfig::default())?;
    /// session.start_stream()?;
    ///
    /// let frame = session.next_frame()?;
    /// println!("Frame {}: {} bytes", frame.metadata.sequence, frame.data.len());
    /// # Ok(())
    /// # }
    /// ```
    pub fn next_frame(&mut self) -> Result<Frame> {
        // Create a temporary stream for this capture
        let mut stream = self.device.create_stream(self.config.buffer_count, self.config.fps)?;
        stream.next_frame()
    }

    /// Get the capture configuration.
    #[must_use]
    pub const fn config(&self) -> &CaptureConfig {
        &self.config
    }

    /// Get the actual format set by the driver.
    #[must_use]
    pub const fn actual_format(&self) -> &Format {
        &self.actual_format
    }

    /// Get the actual FPS set by the driver.
    #[must_use]
    pub fn actual_fps(&self) -> u32 {
        self.actual_fps.unwrap_or(self.config.fps)
    }

    /// Get device capabilities.
    #[must_use]
    pub fn capabilities(&self) -> &crate::traits::DeviceCapabilities {
        self.device.capabilities()
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        log::debug!("Closing capture session");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_with_default_config() {
        let config = CaptureConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_accessors() {
        let config = CaptureConfig::builder()
            .resolution(640, 480)
            .fps(60)
            .build()
            .unwrap();

        assert_eq!(config.width, 640);
        assert_eq!(config.height, 480);
        assert_eq!(config.fps, 60);
    }
}
