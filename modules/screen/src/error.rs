//! Screen capture error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ScreenError {
    #[error("Capture failed: {0}")]
    CaptureFailed(String),

    #[error("Not initialized")]
    NotInitialized,

    #[error("Already capturing")]
    AlreadyCapturing,

    #[error("Not capturing")]
    NotCapturing,

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Platform error: {0}")]
    PlatformError(String),

    #[error("OCR failed: {0}")]
    OCRFailed(String),

    #[error("UI tree extraction failed: {0}")]
    UITreeExtractionFailed(String),

    #[error("Visual grounding failed: {0}")]
    GroundingFailed(String),

    #[error("Invalid region: {0}")]
    InvalidRegion(String),

    #[error("Unsupported operation: {0}")]
    Unsupported(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Windows API error: {0}")]
    #[cfg(target_os = "windows")]
    WindowsError(#[from] windows::core::Error),

    #[error("JNI error: {0}")]
    #[cfg(target_os = "android")]
    JniError(#[from] jni::errors::Error),

    #[error("Unsupported platform")]
    UnsupportedPlatform,
}

pub type ScreenResult<T> = Result<T, ScreenError>;

impl From<ScreenError> for nova_kernel::NovaError {
    fn from(e: ScreenError) -> Self {
        nova_kernel::NovaError::new(
            nova_kernel::ErrorCategory::Internal,
            "ERR_SCREEN",
            &e.to_string(),
        )
    }
}