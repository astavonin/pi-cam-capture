//! High-level capture session API with guard-based streaming.
//!
//! This module provides a simplified interface for camera capture that handles
//! device initialization, format configuration, and stream management automatically.
//!
//! The streaming API uses a guard pattern (like `Mutex::lock()` → `MutexGuard`)
//! to ensure efficient buffer reuse and prevent common usage errors at compile time.
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
//! // Create streaming guard (starts streaming, allocates buffers)
//! let mut stream = session.streaming()?;
//!
//! // Capture frames efficiently - stream persists across iterations
//! for _ in 0..10 {
//!     let frame = stream.next_frame()?;
//!     println!("Frame {}: {} bytes", frame.metadata.sequence, frame.data.len());
//! }
//!
//! // Stream guard dropped here - stops streaming, releases buffers
//! # Ok(())
//! # }
//! ```

use crate::config::CaptureConfig;
use crate::device::V4L2Device;
use crate::error::Result;
use crate::traits::{CameraDevice, CaptureStream as CaptureStreamTrait, Format, Frame};

/// High-level capture session that manages device configuration.
///
/// `CaptureSession` provides a simple interface for camera capture by handling
/// all initialization and resource management automatically.
///
/// To capture frames, call [`streaming()`](Self::streaming) to create a
/// [`CaptureStream`] guard that manages the streaming lifecycle.
///
/// # Lifecycle
///
/// 1. **Create**: Opens device, sets format
/// 2. **Stream**: Call `streaming()` to create guard and allocate buffers
/// 3. **Capture**: Call `next_frame()` on the guard repeatedly
/// 4. **Cleanup**: Automatic when guard drops (stops streaming, releases buffers)
///
/// # Guard Pattern
///
/// Similar to `Mutex::lock()` → `MutexGuard`, the `streaming()` method returns
/// a guard that borrows the session. While the guard exists, the session cannot
/// be reconfigured, preventing invalid state.
pub struct CaptureSession<D: CameraDevice = V4L2Device> {
    device: D,
    config: CaptureConfig,
    actual_format: Format,
    actual_fps: Option<u32>,
}

/// Streaming guard that manages active capture stream.
///
/// This guard borrows from [`CaptureSession`] and ensures efficient buffer reuse.
/// When dropped, it automatically stops streaming and releases kernel resources.
///
/// Created by calling [`CaptureSession::streaming()`].
///
/// # Example
///
/// ```no_run
/// use pi_cam_capture::{CaptureConfig, CaptureSession};
///
/// # fn main() -> pi_cam_capture::Result<()> {
/// let mut session = CaptureSession::new(CaptureConfig::default())?;
///
/// // Create guard - starts streaming
/// let mut stream = session.streaming()?;
///
/// // Capture frames - efficient, reuses buffers
/// for _ in 0..100 {
///     let frame = stream.next_frame()?;
///     process(frame);
/// }
///
/// // Guard dropped here - stops streaming automatically
/// # Ok(())
/// # }
/// # fn process(frame: pi_cam_capture::Frame) {}
/// ```
pub struct CaptureStream<'a, D: CameraDevice + 'a> {
    stream: D::Stream<'a>,
    actual_fps: u32,
}

/// Convenience constructor for `V4L2Device` (the common production case).
impl CaptureSession {
    /// Create a new capture session with the specified configuration.
    ///
    /// This method:
    /// 1. Validates the configuration
    /// 2. Opens the V4L2 device
    /// 3. Sets the requested format
    ///
    /// Call [`streaming()`](Self::streaming) to create a guard and begin capturing frames.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid, the device cannot be opened,
    /// or the format cannot be set.
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
        config.validate()?;

        let mut device = V4L2Device::open(config.device_index())?;

        log::info!(
            "Opened device /dev/video{}: {}",
            config.device_index(),
            device.capabilities().card
        );

        let requested_format = Format::new(config.width(), config.height(), config.format());
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
}

impl<D: CameraDevice> CaptureSession<D> {
    /// Create a session from an already-opened device.
    ///
    /// Useful for testing with [`MockDevice`](crate::mock::MockDevice) or any
    /// custom `CameraDevice` implementation.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid or the format cannot be set.
    pub fn with_device(mut device: D, config: CaptureConfig) -> Result<Self> {
        config.validate()?;
        let requested_format = Format::new(config.width(), config.height(), config.format());
        let actual_format = device.set_format(&requested_format)?;
        Ok(Self {
            device,
            config,
            actual_format,
            actual_fps: None,
        })
    }

    /// Create a streaming guard to capture frames efficiently.
    ///
    /// This method:
    /// 1. Allocates mmap buffers
    /// 2. Starts V4L2 streaming
    /// 3. Returns a guard that borrows this session
    ///
    /// The guard persists the stream across multiple `next_frame()` calls,
    /// making capture efficient. When dropped, it automatically stops streaming
    /// and releases buffers.
    ///
    /// # Guard Behavior
    ///
    /// While the guard exists, the session is mutably borrowed and cannot be
    /// reconfigured. This prevents errors like changing format mid-stream.
    ///
    /// # Errors
    ///
    /// Returns an error if buffer allocation or stream start fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pi_cam_capture::{CaptureConfig, CaptureSession};
    ///
    /// # fn main() -> pi_cam_capture::Result<()> {
    /// let mut session = CaptureSession::new(CaptureConfig::default())?;
    ///
    /// // Create guard - starts streaming
    /// let mut stream = session.streaming()?;
    ///
    /// // Capture frames - stream persists, efficient
    /// for _ in 0..10 {
    ///     let frame = stream.next_frame()?;
    ///     println!("Frame: {} bytes", frame.data.len());
    /// }
    ///
    /// // Guard dropped - stops streaming
    /// # Ok(())
    /// # }
    /// ```
    pub fn streaming(&mut self) -> Result<CaptureStream<'_, D>> {
        let stream = self
            .device
            .create_stream(self.config.buffer_count(), self.config.fps())?;
        let actual_fps = stream.actual_fps();

        log::info!(
            "Created stream with {} buffers at {} FPS",
            self.config.buffer_count(),
            actual_fps
        );

        self.actual_fps = Some(actual_fps);

        Ok(CaptureStream { stream, actual_fps })
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

    /// Returns the FPS negotiated by the driver after [`streaming()`](Self::streaming) is called.
    ///
    /// Returns `None` if streaming has not been started yet.
    #[must_use]
    pub const fn actual_fps(&self) -> Option<u32> {
        self.actual_fps
    }

    /// Returns the target FPS from the configuration (may differ from driver-negotiated FPS).
    #[must_use]
    pub const fn target_fps(&self) -> u32 {
        self.config.fps
    }

    /// Get device capabilities.
    #[must_use]
    pub fn capabilities(&self) -> &crate::traits::DeviceCapabilities {
        self.device.capabilities()
    }
}

impl<'a, D: CameraDevice + 'a> CaptureStream<'a, D> {
    /// Capture the next frame from the camera.
    ///
    /// This blocks until a frame is available or an error occurs.
    /// The stream efficiently reuses the same mmap buffers across calls.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame cannot be captured.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pi_cam_capture::{CaptureConfig, CaptureSession};
    ///
    /// # fn main() -> pi_cam_capture::Result<()> {
    /// let mut session = CaptureSession::new(CaptureConfig::default())?;
    /// let mut stream = session.streaming()?;
    ///
    /// let frame = stream.next_frame()?;
    /// println!("Frame {}: {} bytes", frame.metadata.sequence, frame.data.len());
    /// # Ok(())
    /// # }
    /// ```
    // Inherent method retained for ergonomics; delegates to the CaptureStream trait impl below.
    // same_name_method: intentional — callers benefit from method-call syntax without a trait import.
    #[allow(clippy::same_name_method)]
    pub fn next_frame(&mut self) -> Result<Frame> {
        self.stream.next_frame()
    }

    /// Get the actual FPS set by the driver for this stream.
    // same_name_method: intentional — this const inherent method is more efficient than
    // the trait's non-const version and avoids requiring callers to import the trait.
    #[must_use]
    #[allow(clippy::same_name_method)]
    pub const fn actual_fps(&self) -> u32 {
        self.actual_fps
    }
}

impl<'a, D: CameraDevice + 'a> CaptureStreamTrait for CaptureStream<'a, D> {
    fn actual_fps(&self) -> u32 {
        self.actual_fps
    }

    fn next_frame(&mut self) -> Result<Frame> {
        self.stream.next_frame()
    }
}

impl<'a, D: CameraDevice + 'a> Drop for CaptureStream<'a, D> {
    fn drop(&mut self) {
        log::info!("Stopping capture stream (actual_fps={})", self.actual_fps);
    }
}

impl<D: CameraDevice> Drop for CaptureSession<D> {
    fn drop(&mut self) {
        log::info!(
            "Closing capture session (device_index={}, actual_fps={:?})",
            self.config.device_index(),
            self.actual_fps
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockDevice;
    use crate::traits::CaptureStream as CaptureStreamTrait;

    // Helper: build a valid config without opening a real device
    fn test_config() -> CaptureConfig {
        CaptureConfig {
            device_index: 0,
            width: 640,
            height: 480,
            format: crate::traits::FourCC::YUYV,
            fps: 30,
            buffer_count: 4,
        }
    }

    #[test]
    fn test_with_device_validates_config() {
        let device = MockDevice::new();
        let bad_config = CaptureConfig {
            buffer_count: 0,
            ..test_config()
        };
        let result = CaptureSession::with_device(device, bad_config);
        assert!(result.is_err());
        assert!(matches!(
            result.err().expect("result was Ok"),
            crate::error::CaptureError::Config(crate::error::ConfigError::InvalidBufferCount(0))
        ));
    }

    #[test]
    fn test_actual_fps_none_before_streaming() {
        let device = MockDevice::new();
        let session =
            CaptureSession::with_device(device, test_config()).expect("with_device failed");
        assert!(session.actual_fps().is_none());
    }

    #[test]
    fn test_actual_fps_some_after_streaming() {
        let device = MockDevice::new();
        let mut session =
            CaptureSession::with_device(device, test_config()).expect("with_device failed");
        {
            // Drop the stream before reading back from session to satisfy borrow checker
            let _stream = session.streaming().expect("streaming failed");
        }
        assert!(session.actual_fps().is_some());
    }

    #[test]
    fn test_guard_capture_and_drop() {
        let device = MockDevice::new();
        let mut session =
            CaptureSession::with_device(device, test_config()).expect("with_device failed");

        // First streaming guard
        {
            let mut stream = session.streaming().expect("streaming failed");
            let f1 = stream.next_frame().expect("next_frame failed");
            let f2 = stream.next_frame().expect("next_frame failed");
            assert_eq!(f1.metadata.sequence, 0);
            assert_eq!(f2.metadata.sequence, 1);
        } // guard dropped here

        // Can create a second streaming guard after first is dropped
        {
            let mut stream = session.streaming().expect("second streaming failed");
            let f = stream.next_frame().expect("next_frame failed");
            // MockDevice frame_count continues from where it left off
            assert_eq!(f.metadata.sequence, 2);
        }
    }

    #[test]
    fn test_capture_stream_implements_trait() {
        let device = MockDevice::new();
        let mut session =
            CaptureSession::with_device(device, test_config()).expect("with_device failed");
        let mut stream = session.streaming().expect("streaming failed");

        // CaptureStream trait must be usable via the guard
        let frame = <CaptureStream<'_, MockDevice> as CaptureStreamTrait>::next_frame(&mut stream)
            .expect("trait next_frame failed");
        assert!(!frame.data.is_empty());
    }

    #[test]
    fn test_target_fps_vs_actual_fps() {
        let device = MockDevice::new();
        let config = CaptureConfig {
            fps: 60,
            ..test_config()
        };
        let mut session = CaptureSession::with_device(device, config).expect("with_device failed");

        assert_eq!(session.target_fps(), 60);
        assert!(session.actual_fps().is_none());

        {
            // Drop the stream before reading back from session to satisfy borrow checker
            let _stream = session.streaming().expect("streaming failed");
        }
        assert!(session.actual_fps().is_some());
    }

    #[test]
    #[allow(clippy::panic)] // intentional panic to exercise Drop on unwind
    fn test_guard_drop_on_panic() {
        // Verify the guard's Drop fires even during an unwinding panic.
        let result = std::panic::catch_unwind(|| {
            let device = MockDevice::new();
            let mut session = CaptureSession::with_device(
                device,
                CaptureConfig {
                    device_index: 0,
                    width: 640,
                    height: 480,
                    format: crate::traits::FourCC::YUYV,
                    fps: 30,
                    buffer_count: 4,
                },
            )
            .expect("with_device failed");
            let _stream = session.streaming().expect("streaming failed");
            panic!("intentional panic to test Drop");
        });
        // Drop should have fired without secondary panic
        assert!(result.is_err());
    }
}
