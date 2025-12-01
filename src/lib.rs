pub mod device;
pub mod traits;

#[cfg(test)]
pub mod mock;

pub use device::V4L2Device;
pub use traits::{
    CameraDevice, CaptureStream, DeviceCapabilities, Format, FourCC, Frame, FrameMetadata,
};
