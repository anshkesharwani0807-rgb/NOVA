//! Screen capture traits and platform implementations

use crate::{CapturedFrame, ScreenCaptureConfig, ScreenResult};
use async_trait::async_trait;

/// Screen capture trait
#[async_trait]
pub trait ScreenCapture: Send + Sync {
    fn id(&self) -> &str;

    async fn start_capture(&mut self, config: ScreenCaptureConfig) -> ScreenResult<()>;
    async fn stop_capture(&mut self) -> ScreenResult<()>;
    async fn capture_frame(&mut self) -> ScreenResult<CapturedFrame>;
    async fn start_stream(
        &mut self,
        tx: tokio::sync::mpsc::Sender<CapturedFrame>,
    ) -> ScreenResult<()>;
    fn is_capturing(&self) -> bool;
}

/// Platform-agnostic screen capture factory
pub struct ScreenCaptureFactory;

impl ScreenCaptureFactory {
    pub fn create() -> ScreenResult<Box<dyn ScreenCapture>> {
        #[cfg(target_os = "windows")]
        {
            Ok(Box::new(windows::WindowsScreenCapture::new()?))
        }
        #[cfg(target_os = "android")]
        {
            Ok(Box::new(android::AndroidScreenCapture::new()?))
        }
        #[cfg(not(any(target_os = "windows", target_os = "android")))]
        {
            Err(ScreenError::UnsupportedPlatform)
        }
    }
}

/// Platform-specific modules - MUST BE PUBLIC for lib.rs to access them
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "android")]
pub mod android;

// Stub implementation for unsupported platforms
#[cfg(not(any(target_os = "windows", target_os = "android")))]
mod stub {
    use super::*;
    use crate::ScreenError;
    use std::sync::Arc;

    pub struct StubScreenCapture;

    #[async_trait]
    impl super::ScreenCapture for StubScreenCapture {
        fn id(&self) -> &str {
            "stub"
        }

        async fn start_capture(&mut self, _config: ScreenCaptureConfig) -> ScreenResult<()> {
            Err(ScreenError::UnsupportedPlatform)
        }

        async fn stop_capture(&mut self) -> ScreenResult<()> {
            Ok(())
        }

        async fn capture_frame(&mut self) -> ScreenResult<CapturedFrame> {
            Err(ScreenError::UnsupportedPlatform)
        }

        async fn start_stream(
            &mut self,
            _tx: tokio::sync::mpsc::Sender<CapturedFrame>,
        ) -> ScreenResult<()> {
            Err(ScreenError::UnsupportedPlatform)
        }

        fn is_capturing(&self) -> bool {
            false
        }
    }

    pub fn create() -> ScreenResult<Arc<dyn super::ScreenCapture>> {
        Ok(Arc::new(StubScreenCapture))
    }
}
