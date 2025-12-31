//! Configuration types for camera capture sessions.
//!
//! # Example
//!
//! ```
//! use pi_cam_capture::{CaptureConfig, FourCC};
//!
//! // Using the builder pattern
//! let config = CaptureConfig::builder()
//!     .device(0)
//!     .resolution(1920, 1080)
//!     .format(FourCC::YUYV)
//!     .fps(30)
//!     .buffer_count(4)
//!     .build()
//!     .unwrap();
//!
//! // Or use the default configuration
//! let default_config = CaptureConfig::default();
//! assert_eq!(default_config.width, 1920);
//! assert_eq!(default_config.height, 1080);
//! assert_eq!(default_config.fps, 30);
//! ```

use crate::error::{ConfigError, Result};
use crate::traits::FourCC;

/// Configuration for a camera capture session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureConfig {
    /// Device index (e.g., 0 for /dev/video0).
    pub device_index: u32,

    /// Frame width in pixels.
    pub width: u32,

    /// Frame height in pixels.
    pub height: u32,

    /// Pixel format.
    pub format: FourCC,

    /// Target frames per second.
    pub fps: u32,

    /// Number of mmap buffers (2-8 recommended).
    pub buffer_count: u32,
}

impl CaptureConfig {
    /// Create a new builder for `CaptureConfig`.
    #[must_use]
    pub fn builder() -> CaptureConfigBuilder {
        CaptureConfigBuilder::new()
    }

    /// Validate the configuration parameters.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if any parameter is outside valid ranges:
    /// - Buffer count must be 2-8
    /// - FPS must be 1-120
    /// - Resolution must be 160x120 to 4096x4096
    pub fn validate(&self) -> Result<()> {
        // Buffer count must be 2-8 (V4L2 limitation)
        if !(2..=8).contains(&self.buffer_count) {
            return Err(ConfigError::InvalidBufferCount(self.buffer_count).into());
        }

        // Reasonable FPS range
        if !(1..=120).contains(&self.fps) {
            return Err(ConfigError::UnsupportedFps(self.fps).into());
        }

        // Reasonable resolution range
        if self.width < 160 || self.width > 4096 {
            return Err(ConfigError::UnsupportedResolution {
                width: self.width,
                height: self.height,
            }
            .into());
        }

        if self.height < 120 || self.height > 4096 {
            return Err(ConfigError::UnsupportedResolution {
                width: self.width,
                height: self.height,
            }
            .into());
        }

        Ok(())
    }
}

impl Default for CaptureConfig {
    /// Default configuration: 1920x1080 YUYV at 30 FPS with 4 buffers on /dev/video0.
    fn default() -> Self {
        Self {
            device_index: 0,
            width: 1920,
            height: 1080,
            format: FourCC::YUYV,
            fps: 30,
            buffer_count: 4,
        }
    }
}

/// Builder for `CaptureConfig`.
#[derive(Debug)]
pub struct CaptureConfigBuilder {
    config: CaptureConfig,
}

impl CaptureConfigBuilder {
    /// Create a new builder with default values.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: CaptureConfig::default(),
        }
    }

    /// Set the device index.
    #[must_use]
    pub const fn device(mut self, index: u32) -> Self {
        self.config.device_index = index;
        self
    }

    /// Set the resolution.
    #[must_use]
    pub const fn resolution(mut self, width: u32, height: u32) -> Self {
        self.config.width = width;
        self.config.height = height;
        self
    }

    /// Set the pixel format.
    #[must_use]
    pub const fn format(mut self, format: FourCC) -> Self {
        self.config.format = format;
        self
    }

    /// Set the target FPS.
    #[must_use]
    pub const fn fps(mut self, fps: u32) -> Self {
        self.config.fps = fps;
        self
    }

    /// Set the number of buffers.
    #[must_use]
    pub const fn buffer_count(mut self, count: u32) -> Self {
        self.config.buffer_count = count;
        self
    }

    /// Build and validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` if validation fails.
    pub fn build(self) -> Result<CaptureConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

impl Default for CaptureConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = CaptureConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_builder_default() {
        let config = CaptureConfig::builder().build().unwrap();
        assert_eq!(config, CaptureConfig::default());
    }

    #[test]
    fn test_builder_custom() {
        let config = CaptureConfig::builder()
            .device(1)
            .resolution(1280, 720)
            .fps(60)
            .buffer_count(8)
            .build()
            .unwrap();

        assert_eq!(config.device_index, 1);
        assert_eq!(config.width, 1280);
        assert_eq!(config.height, 720);
        assert_eq!(config.fps, 60);
        assert_eq!(config.buffer_count, 8);
    }

    #[test]
    fn test_invalid_buffer_count_too_low() {
        let mut config = CaptureConfig::default();
        config.buffer_count = 1;

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::CaptureError::Config(ConfigError::InvalidBufferCount(1))
        ));
    }

    #[test]
    fn test_invalid_buffer_count_too_high() {
        let mut config = CaptureConfig::default();
        config.buffer_count = 9;

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::CaptureError::Config(ConfigError::InvalidBufferCount(9))
        ));
    }

    #[test]
    fn test_invalid_fps_zero() {
        let mut config = CaptureConfig::default();
        config.fps = 0;

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::CaptureError::Config(ConfigError::UnsupportedFps(0))
        ));
    }

    #[test]
    fn test_invalid_fps_too_high() {
        let mut config = CaptureConfig::default();
        config.fps = 121;

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::CaptureError::Config(ConfigError::UnsupportedFps(121))
        ));
    }

    #[test]
    fn test_invalid_resolution_width_too_small() {
        let mut config = CaptureConfig::default();
        config.width = 159;

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::CaptureError::Config(ConfigError::UnsupportedResolution { .. })
        ));
    }

    #[test]
    fn test_invalid_resolution_width_too_large() {
        let mut config = CaptureConfig::default();
        config.width = 4097;

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::CaptureError::Config(ConfigError::UnsupportedResolution { .. })
        ));
    }

    #[test]
    fn test_invalid_resolution_height_too_small() {
        let mut config = CaptureConfig::default();
        config.height = 119;

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::CaptureError::Config(ConfigError::UnsupportedResolution { .. })
        ));
    }

    #[test]
    fn test_invalid_resolution_height_too_large() {
        let mut config = CaptureConfig::default();
        config.height = 4097;

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::CaptureError::Config(ConfigError::UnsupportedResolution { .. })
        ));
    }

    #[test]
    fn test_valid_edge_cases() {
        // Minimum valid values
        let config = CaptureConfig::builder()
            .resolution(160, 120)
            .fps(1)
            .buffer_count(2)
            .build();
        assert!(config.is_ok());

        // Maximum valid values
        let config = CaptureConfig::builder()
            .resolution(4096, 4096)
            .fps(120)
            .buffer_count(8)
            .build();
        assert!(config.is_ok());
    }

    #[test]
    fn test_builder_fails_on_invalid() {
        let result = CaptureConfig::builder().buffer_count(1).build();
        assert!(result.is_err());
    }
}
