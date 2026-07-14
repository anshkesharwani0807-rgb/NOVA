mod config;
mod error;
mod events;
mod manager;
mod pairing;
mod protocol;
mod transport;

pub use config::*;
pub use error::*;
pub use events::*;
pub use manager::*;
pub use pairing::*;
pub use protocol::*;

use async_trait::async_trait;
use nova_kernel::module::KernelModule;
use nova_kernel::NovaError;
use std::sync::Arc;

pub struct DeviceSync {
    inner: Arc<SyncInner>,
}

struct SyncInner {
    manager: parking_lot::Mutex<SyncManager>,
    config: parking_lot::RwLock<SyncConfig>,
}

impl DeviceSync {
    pub fn new(_kernel: Arc<nova_kernel::Kernel>) -> Self {
        Self {
            inner: Arc::new(SyncInner {
                manager: parking_lot::Mutex::new(SyncManager::new()),
                config: parking_lot::RwLock::new(SyncConfig::default()),
            }),
        }
    }

    pub fn manager(&self) -> &parking_lot::Mutex<SyncManager> {
        &self.inner.manager
    }

    pub fn config(&self) -> &parking_lot::RwLock<SyncConfig> {
        &self.inner.config
    }
}

#[async_trait]
impl KernelModule for DeviceSync {
    fn module_id(&self) -> &'static str {
        "sync"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn dependencies(&self) -> Vec<&'static str> {
        vec!["kernel"]
    }

    async fn initialize(&self) -> std::result::Result<(), NovaError> {
        tracing::info!("DeviceSync initialized");
        Ok(())
    }

    async fn start(&self) -> std::result::Result<(), NovaError> {
        tracing::info!("DeviceSync started");
        Ok(())
    }

    async fn stop(&self) -> std::result::Result<(), NovaError> {
        tracing::info!("DeviceSync stopped");
        Ok(())
    }

    async fn shutdown(&self) -> std::result::Result<(), NovaError> {
        tracing::info!("DeviceSync shutdown");
        Ok(())
    }

    fn health(&self) -> nova_kernel::module::ModuleHealth {
        nova_kernel::module::ModuleHealth::healthy()
    }
}
