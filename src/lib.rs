//! Pi-Cam-Capture: A V4L2 camera capture library for Raspberry Pi
//!
//! This library provides trait-based abstractions over V4L2 camera operations,
//! enabling both production use with real hardware and testing with mock devices.
//!
//! # Quick Start
//!
//! ```no_run
//! use pi_cam_capture::{CaptureConfig, CaptureSession};
//!
//! # fn main() -> pi_cam_capture::Result<()> {
//! // Create and configure a capture session
//! let config = CaptureConfig::builder()
//!     .resolution(1280, 720)
//!     .fps(30)
//!     .build()?;
//!
//! let mut session = CaptureSession::new(config)?;
//!
//! // Capture frames
//! for _ in 0..10 {
//!     let frame = session.next_frame()?;
//!     println!("Frame {}: {} bytes", frame.metadata.sequence, frame.data.len());
//! }
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod device;
pub mod error;
pub mod session;
pub mod stream;
pub mod traits;
pub mod validation;

#[cfg(test)]
pub mod mock;

pub use config::{CaptureConfig, CaptureConfigBuilder};
pub use device::V4L2Device;
pub use error::{CaptureError, ConfigError, DeviceError, Result, StreamError};
pub use session::CaptureSession;
pub use stream::V4L2Stream;
pub use traits::{
    CameraDevice, CaptureStream, DeviceCapabilities, Format, FourCC, Frame, FrameMetadata,
};
pub use validation::{validate_color_bars, validate_frame_sequence, validate_gradient};
