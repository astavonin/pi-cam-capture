//! V4L2 device implementation using the v4l crate.

use v4l::buffer::Type;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream as V4lCaptureStream;
use v4l::video::Capture;
use v4l::Device;

use crate::traits::{
    CameraDevice, CameraError, CaptureStream, DeviceCapabilities, Format, FourCC, Frame,
    FrameMetadata, Result,
};
use std::time::Duration;

/// V4L2 device implementation wrapping the v4l crate.
pub struct V4L2Device {
    device: Device,
    capabilities: DeviceCapabilities,
}

impl V4L2Device {
    /// Open a V4L2 device by index (e.g., 0 for /dev/video0).
    pub fn open(index: u32) -> Result<Self> {
        let device = Device::new(index as usize)
            .map_err(|err| CameraError::DeviceOpenFailed(err.to_string()))?;

        let caps = device
            .query_caps()
            .map_err(|err| CameraError::DeviceOpenFailed(err.to_string()))?;

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
            .map_err(|err| CameraError::StreamError(err.to_string()))?;

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
            .map_err(|err| CameraError::StreamError(err.to_string()))?;

        fmt.width = format.width;
        fmt.height = format.height;
        fmt.fourcc = format.fourcc.into();

        let fmt = self
            .device
            .set_format(&fmt)
            .map_err(|err| CameraError::StreamError(err.to_string()))?;

        Ok(Format {
            width: fmt.width,
            height: fmt.height,
            fourcc: FourCC::from(fmt.fourcc),
            stride: fmt.stride,
            size: fmt.size,
        })
    }

    fn create_stream(&mut self, buffer_count: u32) -> Result<Self::Stream<'_>> {
        let stream = Stream::with_buffers(&self.device, Type::VideoCapture, buffer_count)
            .map_err(|err| CameraError::StreamError(err.to_string()))?;

        Ok(V4L2Stream { stream })
    }
}

/// V4L2 capture stream wrapping mmap-based streaming.
pub struct V4L2Stream<'a> {
    stream: Stream<'a>,
}

impl CaptureStream for V4L2Stream<'_> {
    fn next_frame(&mut self) -> Result<Frame> {
        let (buf, meta) = self
            .stream
            .next()
            .map_err(|err| CameraError::StreamError(err.to_string()))?;

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
