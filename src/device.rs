//! V4L2 device implementation using the v4l crate.

use v4l::video::Capture;
use v4l::Device;

use crate::error::{DeviceError, Result, StreamError};
use crate::stream::V4L2Stream;
use crate::traits::{CameraDevice, DeviceCapabilities, Format, FourCC};


/// V4L2 device implementation wrapping the v4l crate.
pub struct V4L2Device {
    device: Device,
    capabilities: DeviceCapabilities,
}

impl V4L2Device {
    /// Open a V4L2 device by index (e.g., 0 for /dev/video0).
    pub fn open(index: u32) -> Result<Self> {
        let device = Device::new(index as usize)
            .map_err(|err| DeviceError::OpenFailed(err.to_string()))?;

        let caps = device
            .query_caps()
            .map_err(|err| DeviceError::QueryCapsFailed(err.to_string()))?;

        let capabilities = DeviceCapabilities {
            driver: caps.driver,
            card: caps.card,
            bus_info: caps.bus,
            can_capture: caps.capabilities.contains(v4l::capability::Flags::VIDEO_CAPTURE),
            can_stream: caps.capabilities.contains(v4l::capability::Flags::STREAMING),
        };

        Ok(Self {
            device,
            capabilities,
        })
    }
}

impl CameraDevice for V4L2Device {
    type Stream<'a> = V4L2Stream<'a>;

    fn capabilities(&self) -> &DeviceCapabilities {
        &self.capabilities
    }

    fn format(&self) -> Result<Format> {
        let fmt = self
            .device
            .format()
            .map_err(|err| StreamError::GetFormatFailed(err.to_string()))?;

        Ok(Format {
            width: fmt.width,
            height: fmt.height,
            fourcc: FourCC::from(fmt.fourcc),
            stride: fmt.stride,
            size: fmt.size,
        })
    }

    fn set_format(&mut self, format: &Format) -> Result<Format> {
        let mut fmt = self
            .device
            .format()
            .map_err(|err| StreamError::GetFormatFailed(err.to_string()))?;

        fmt.width = format.width;
        fmt.height = format.height;
        fmt.fourcc = format.fourcc.into();

        let fmt = self
            .device
            .set_format(&fmt)
            .map_err(|err| StreamError::SetFormatFailed(err.to_string()))?;

        Ok(Format {
            width: fmt.width,
            height: fmt.height,
            fourcc: FourCC::from(fmt.fourcc),
            stride: fmt.stride,
            size: fmt.size,
        })
    }

    fn create_stream(&mut self, buffer_count: u32, fps: u32) -> Result<Self::Stream<'_>> {
        V4L2Stream::new(&self.device, buffer_count, fps)
    }
}
