pub mod capture;
pub mod config;
pub mod engine;
pub mod error;
pub mod events;
pub mod grounding;
pub mod input;
#[cfg(target_os = "android")]
pub mod jni_bridge;
pub mod ocr;
pub mod permission;
pub mod traits;
pub mod types;
pub mod ui_tree;

pub use types::*;

pub use capture::{ScreenCapture, ScreenCaptureFactory};
pub use config::ScreenConfig;
pub use engine::{ScreenAnalysis, ScreenEngine};
pub use error::{ScreenError, ScreenResult};
pub use events::{ScreenEvent, ScreenEventPayload};
pub use input::{InputActionExt, ScreenInputAction, ScreenInputBridge, ScreenInputTarget};
pub use permission::{ScreenCapability, ScreenPermissionManager};
pub use traits::{OCREngine, UITreeExtractor, VisualGrounding};

use async_trait::async_trait;
use nova_kernel::{
    log_activity, ErrorCategory, EventMetadata, Kernel, KernelModule, ModuleHealth, NovaError,
    NovaEvent, Result,
};
use parking_lot::RwLock;
use std::sync::Arc;

pub struct ScreenSystem {
    #[allow(dead_code)]
    kernel: Arc<Kernel>,
    engine: Arc<RwLock<ScreenEngine>>,
    permissions: Arc<ScreenPermissionManager>,
    config: Arc<RwLock<ScreenConfig>>,
    audit: Arc<RwLock<Vec<ScreenEvent>>>,
}

impl ScreenSystem {
    pub fn new(kernel: Arc<Kernel>) -> Self {
        let config = ScreenConfig::default();
        let permissions = Arc::new(ScreenPermissionManager::new());

        let engine = ScreenEngine::new(config.clone(), permissions.clone())
            .expect("Failed to create ScreenEngine");

        Self {
            kernel,
            engine: Arc::new(RwLock::new(engine)),
            permissions,
            config: Arc::new(RwLock::new(config)),
            audit: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn engine(&self) -> &Arc<RwLock<ScreenEngine>> {
        &self.engine
    }

    pub fn permissions(&self) -> &Arc<ScreenPermissionManager> {
        &self.permissions
    }

    pub fn config(&self) -> &Arc<RwLock<ScreenConfig>> {
        &self.config
    }

    pub fn audit_log(&self) -> Vec<ScreenEvent> {
        self.audit.read().clone()
    }

    pub fn update_config(&self, new_config: ScreenConfig) {
        *self.engine.write().config.write() = new_config.clone();
        *self.config.write() = new_config;
    }

    #[allow(dead_code)]
    fn publish_event(&self, event: ScreenEvent, causing_action: Option<String>) {
        let action = event.action_name().to_string();
        let desc = event.description();
        log_activity("screen", &action, &desc, Some(event.correlation_id));
        let meta = EventMetadata::new("screen", causing_action);
        let _ = self.kernel.event_bus.publish(NovaEvent {
            metadata: meta,
            payload: std::sync::Arc::new(event.clone()),
        });
        let mut log = self.audit.write();
        log.push(event);
        if log.len() > 1000 {
            log.remove(0);
        }
    }
}

#[async_trait]
impl KernelModule for ScreenSystem {
    fn module_id(&self) -> &'static str {
        "screen"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn dependencies(&self) -> Vec<&'static str> {
        vec!["input"]
    }

    async fn initialize(&self) -> Result<()> {
        tracing::info!("Screen module initializing");
        let engine = self.engine.read();
        let id = engine.capture.id().to_string();
        tracing::info!("Screen capture backend: {id}");
        Ok(())
    }

    #[allow(clippy::await_holding_lock)]
    async fn start(&self) -> Result<()> {
        tracing::info!("Screen module starting capture");
        let capture_cfg = {
            let cfg = self.config.read();
            crate::types::ScreenCaptureConfig {
                target_fps: cfg.target_fps,
                region: None,
                include_cursor: cfg.include_cursor,
                downscale_factor: cfg.downscale_factor,
            }
        };
        let mut engine = self.engine.write();
        engine
            .capture
            .start_capture(capture_cfg)
            .await
            .map_err(|e| {
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_SCREEN_CAPTURE_START",
                    &e.to_string(),
                )
            })?;
        drop(engine);
        tracing::info!("Screen module started");
        Ok(())
    }

    #[allow(clippy::await_holding_lock)]
    async fn stop(&self) -> Result<()> {
        tracing::info!("Screen module stopping capture");
        let mut engine = self.engine.write();
        let result = engine.capture.stop_capture().await;
        drop(engine);
        result.map_err(|e| {
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_SCREEN_CAPTURE_STOP",
                &e.to_string(),
            )
        })
    }

    #[allow(clippy::await_holding_lock)]
    async fn shutdown(&self) -> Result<()> {
        tracing::info!("Screen module shutdown");
        let mut engine = self.engine.write();
        if engine.capture.is_capturing() {
            engine.capture.stop_capture().await.ok();
        }
        drop(engine);
        Ok(())
    }

    fn health(&self) -> ModuleHealth {
        let engine = self.engine.read();
        if engine.capture.is_capturing() {
            ModuleHealth::healthy()
        } else {
            ModuleHealth::degraded("Screen capture not running")
        }
    }
}
