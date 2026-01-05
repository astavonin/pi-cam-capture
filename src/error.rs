//! Error types for camera operations.

use thiserror::Error;

/// Top-level error type for all camera operations.
#[derive(Error, Debug)]
pub enum CaptureError {
    /// Device-related error.
    #[error("Device error: {0}")]
    Device(#[from] DeviceError),

    /// Configuration-related error.
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Stream-related error.
    #[error("Stream error: {0}")]
    Stream(#[from] StreamError),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Device operation errors.
#[derive(Error, Debug)]
pub enum DeviceError {
    /// Device not found.
    #[error("Device /dev/video{0} not found")]
    NotFound(u32),

    /// Permission denied.
    #[error("Permission denied accessing device")]
    PermissionDenied,

    /// Device is busy.
    #[error("Device is busy")]
    Busy,

    /// Failed to open device.
    #[error("Failed to open device: {0}")]
    OpenFailed(String),

    /// Failed to query device capabilities.
    #[error("Failed to query device capabilities: {0}")]
    QueryCapsFailed(String),
}

/// Configuration errors.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Unsupported pixel format.
    #[error("Unsupported pixel format: {0:?}")]
    UnsupportedFormat(String),

    /// Unsupported resolution.
    #[error("Unsupported resolution: {width}x{height}")]
    UnsupportedResolution {
        /// Frame width.
        width: u32,
        /// Frame height.
        height: u32,
    },

    /// Unsupported FPS.
    #[error("Unsupported FPS: {0}")]
    UnsupportedFps(u32),

    /// Invalid buffer count.
    #[error("Invalid buffer count: {0} (must be 2-8)")]
    InvalidBufferCount(u32),
}

/// Stream operation errors.
#[derive(Error, Debug)]
pub enum StreamError {
    /// Failed to allocate buffers.
    #[error("Failed to allocate buffers")]
    BufferAllocationFailed,

    /// Stream operation timed out.
    #[error("Stream operation timed out")]
    Timeout,

    /// Failed to start stream.
    #[error("Failed to start stream: {0}")]
    StartFailed(String),

    /// Failed to capture frame.
    #[error("Failed to capture frame: {0}")]
    CaptureFailed(String),

    /// Failed to set format.
    #[error("Failed to set format: {0}")]
    SetFormatFailed(String),

    /// Failed to get format.
    #[error("Failed to get format: {0}")]
    GetFormatFailed(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for camera operations.
pub type Result<T> = std::result::Result<T, CaptureError>;
