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
use crate::traits::{
    BorrowedCaptureStream, CameraDevice, CaptureStream as CaptureStreamTrait, Format, Frame,
    FrameRef,
};
use std::time::{Duration, Instant};

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
    /// Target interval between consecutive frames for schedule-based pacing.
    target_frame_interval: Duration,
    /// Absolute deadline for the next frame call; `None` before the first call.
    /// Advances by exactly one `target_frame_interval` per call regardless of
    /// actual dequeue duration, maintaining a fixed cadence under varying load.
    next_deadline: Option<Instant>,
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

        Ok(CaptureStream {
            stream,
            actual_fps,
            target_frame_interval: frame_interval(actual_fps),
            next_deadline: None,
        })
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
    /// May also block to enforce the configured frame rate; see `target_frame_interval`.
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
        self.pace_next_frame();
        self.stream.next_frame()
    }

    // Schedule-based pacer: advances next_deadline by one fixed interval per call,
    // independent of how long the kernel dequeue took. When the deadline is in the past
    // (slow producer), the pacer fires immediately and catches up on the next call
    // rather than accumulating debt. This maintains accurate long-run mean FPS
    // at the cost of transient burst-then-skip under sustained overload.
    fn pace_next_frame(&mut self) {
        let now = Instant::now();
        if let Some(deadline) = self.next_deadline {
            if now < deadline {
                std::thread::sleep(deadline - now);
            }
            // Advance deadline by one interval regardless of actual sleep duration, so the
            // pacer maintains a fixed cadence even when kernel dequeue time consumes the gap.
            self.next_deadline = Some(deadline + self.target_frame_interval);
        } else {
            self.next_deadline = Some(now + self.target_frame_interval);
        }
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

impl<'a, D> CaptureStream<'a, D>
where
    D: CameraDevice + 'a,
    D::Stream<'a>: BorrowedCaptureStream,
{
    /// Capture the next frame as a zero-copy borrowed view.
    ///
    /// The returned [`FrameRef`] borrows from this streaming guard, so another
    /// frame cannot be captured until the returned value is dropped.
    /// May also block to enforce the configured frame rate; see `target_frame_interval`.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame cannot be captured.
    // same_name_method: intentional — this inherent method (which applies pacing) coexists with
    // the BorrowedCaptureStream trait impl; the trait impl delegates to this via UFCS.
    #[allow(clippy::same_name_method)]
    pub fn next_frame_ref(&mut self) -> Result<FrameRef<'_>> {
        self.pace_next_frame();
        self.stream.next_frame_ref()
    }
}

impl<'a, D: CameraDevice + 'a> CaptureStreamTrait for CaptureStream<'a, D> {
    fn actual_fps(&self) -> u32 {
        self.actual_fps
    }

    // Rust method resolution picks the inherent next_frame() (which applies pacing) over this
    // trait method. Removing the inherent method would silently create infinite recursion here.
    fn next_frame(&mut self) -> Result<Frame> {
        self.next_frame()
    }
}

// same_name_method: intentional — the inherent next_frame_ref() (which applies pacing) coexists
// with this trait impl. UFCS routes to the inherent method, satisfying the trait without
// duplicating the pacing logic.
#[allow(clippy::same_name_method)]
impl<'a, D> BorrowedCaptureStream for CaptureStream<'a, D>
where
    D: CameraDevice + 'a,
    D::Stream<'a>: BorrowedCaptureStream,
{
    fn next_frame_ref(&mut self) -> Result<FrameRef<'_>> {
        // Delegates to the inherent method, which applies schedule-based pacing before
        // forwarding to the inner stream.
        CaptureStream::next_frame_ref(self)
    }
}

impl<'a, D: CameraDevice + 'a> Drop for CaptureStream<'a, D> {
    fn drop(&mut self) {
        log::debug!(
            "CaptureStream guard dropping — inner stream cleanup follows (actual_fps={})",
            self.actual_fps
        );
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

fn frame_interval(fps: u32) -> Duration {
    Duration::from_secs_f64(1.0 / f64::from(fps.max(1)))
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
    fn test_guard_capture_borrowed_frame() {
        let device = MockDevice::new();
        // fps=120 → ~8.3ms interval; assert ≥7ms to confirm pacing fires between consecutive calls
        // (7ms ≈ 84% of one interval, tolerating scheduler jitter without masking a halved cadence)
        let config = CaptureConfig {
            fps: 120,
            ..test_config()
        };
        let mut session = CaptureSession::with_device(device, config).expect("with_device failed");
        let mut stream = session.streaming().expect("streaming failed");

        let t0 = Instant::now();
        {
            let frame = stream.next_frame_ref().expect("next_frame_ref failed");
            assert_eq!(frame.metadata.sequence, 0);
            assert_eq!(frame.data.len(), (640 * 480 * 2) as usize);
        }

        {
            let _frame = stream
                .next_frame_ref()
                .expect("second next_frame_ref failed");
        }
        // Two calls at 120fps should take at least one interval (~8.3ms); assert ≥7ms (half an
        // interval below the expected 8.3ms gives headroom for jitter without masking a halved cadence).
        assert!(
            t0.elapsed() >= Duration::from_millis(7),
            "expected pacing delay ≥7ms between consecutive next_frame_ref calls"
        );
        assert!(
            t0.elapsed() < Duration::from_millis(100),
            "pacing delay unexpectedly long: {:?}",
            t0.elapsed()
        );

        let frame = stream.next_frame().expect("next_frame failed");
        assert_eq!(frame.metadata.sequence, 2);
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

    // Verifies that wall-clock time between three consecutive frames meets at least two intervals.
    // Note: both schedule-based and offset-based pacers would satisfy this assertion; a test that
    // distinguishes the two designs would require injecting artificial dequeue latency.
    #[test]
    fn test_pacer_fires_between_consecutive_frames() {
        let device = MockDevice::new();
        // fps=100 → 10ms interval; capture 3 frames and assert wall-clock ≥ 2 intervals (20ms)
        let config = CaptureConfig {
            fps: 100,
            ..test_config()
        };
        let mut session = CaptureSession::with_device(device, config).expect("with_device failed");
        let mut stream = session.streaming().expect("streaming failed");

        let before = Instant::now();
        let _f1 = stream.next_frame().expect("frame 1 failed");
        let _f2 = stream.next_frame().expect("frame 2 failed");
        let _f3 = stream.next_frame().expect("frame 3 failed");
        let elapsed = before.elapsed();

        assert!(
            elapsed >= Duration::from_millis(18),
            "expected ≥ 2 intervals (20ms) for 3 frames at 100fps, got {elapsed:?}"
        );
    }

    // frame_interval produces correct durations for common rates
    #[test]
    fn test_frame_interval_common_rates() {
        // 1/30 s ≈ 33_333_333 ns (floor of exact value 33_333_333.3…)
        let interval_30 = frame_interval(30);
        assert!(
            interval_30.as_nanos() >= 33_333_333 && interval_30.as_nanos() <= 33_333_334,
            "30fps interval should be ~33.33ms, got {interval_30:?}"
        );
        // 1/60 s ≈ 16_666_666 ns (floor of exact value 16_666_666.6…)
        let interval_60 = frame_interval(60);
        assert!(
            interval_60.as_nanos() >= 16_666_666 && interval_60.as_nanos() <= 16_666_667,
            "60fps interval should be ~16.67ms, got {interval_60:?}"
        );
    }

    // fps=0 is clamped to 1 Hz to avoid division by zero
    #[test]
    fn test_frame_interval_zero_fps_falls_back_to_one_hz() {
        assert_eq!(frame_interval(0), Duration::from_secs(1));
    }

    // fps=1 produces a one-second interval
    #[test]
    fn test_frame_interval_one_fps() {
        assert_eq!(frame_interval(1), Duration::from_secs(1));
    }

    // First call to next_frame must not sleep the full interval (no deadline set yet)
    #[test]
    fn test_pacer_first_call_does_not_sleep() {
        let device = MockDevice::new();
        // fps=1 → 1s interval; first call must return quickly, not sleep 1 second
        let config = CaptureConfig {
            fps: 1,
            ..test_config()
        };
        let mut session = CaptureSession::with_device(device, config).expect("with_device failed");
        let mut stream = session.streaming().expect("streaming failed");
        let before = Instant::now();
        let _ = stream.next_frame().expect("first call failed");
        assert!(
            before.elapsed() < Duration::from_millis(100),
            "first call slept unexpectedly"
        );
    }

    // CaptureStream implements BorrowedCaptureStream and works in generic context
    #[test]
    fn test_capture_stream_implements_borrowed_capture_stream_trait() {
        fn use_borrowed<S: BorrowedCaptureStream>(stream: &mut S) -> crate::error::Result<usize> {
            let frame = stream.next_frame_ref()?;
            Ok(frame.data.len())
        }

        let device = MockDevice::new();
        let config = CaptureConfig {
            fps: 120,
            ..test_config()
        };
        let mut session = CaptureSession::with_device(device, config).expect("with_device failed");
        let mut stream = session.streaming().expect("streaming failed");
        let len = use_borrowed(&mut stream).expect("trait usage failed");
        assert!(len > 0);
    }

    // CaptureStream guard propagates stream errors from next_frame through the call chain
    #[test]
    fn test_guard_next_frame_propagates_stream_error() {
        let device = MockDevice::new().with_error_after(1);
        let config = CaptureConfig {
            fps: 120,
            ..test_config()
        };
        let mut session = CaptureSession::with_device(device, config).expect("with_device failed");
        let mut stream = session.streaming().expect("streaming failed");
        let _first = stream.next_frame().expect("first frame should succeed");
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

    // CaptureStream guard propagates stream errors from next_frame_ref through the call chain
    #[test]
    fn test_guard_next_frame_ref_propagates_stream_error() {
        let device = MockDevice::new().with_error_after(1);
        let config = CaptureConfig {
            fps: 120,
            ..test_config()
        };
        let mut session = CaptureSession::with_device(device, config).expect("with_device failed");
        let mut stream = session.streaming().expect("streaming failed");
        {
            let _first = stream.next_frame_ref().expect("first frame should succeed");
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
}
