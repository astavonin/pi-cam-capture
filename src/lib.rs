//! Pi-Cam-Capture: A V4L2 camera capture library for Raspberry Pi
//!
//! This library provides trait-based abstractions over V4L2 camera operations,
//! enabling both production use with real hardware and testing with mock devices.

pub mod device;
pub mod traits;

#[cfg(test)]
pub mod mock;

pub use device::V4L2Device;
pub use traits::{
    CameraDevice, CaptureStream, DeviceCapabilities, Format, FourCC, Frame, FrameMetadata,
};
