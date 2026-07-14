pub mod battery;
pub mod clipboard;
pub mod connectivity;
pub mod notifications;
pub mod sensors;
pub mod storage;

use async_trait::async_trait;
use nova_kernel::Result;
use std::sync::Arc;

#[async_trait]
pub trait DeviceService: Send + Sync {
    fn name(&self) -> &'static str;
    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    fn is_running(&self) -> bool;
}

pub struct ServiceRegistry {
    services: Vec<Arc<dyn DeviceService>>,
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self { services: vec![] }
    }

    pub fn register(&mut self, service: Arc<dyn DeviceService>) {
        self.services.push(service);
    }

    pub async fn start_all(&self) -> Result<()> {
        for service in &self.services {
            service.start().await?;
            tracing::info!("Device service '{}' started", service.name());
        }
        Ok(())
    }

    pub async fn stop_all(&self) -> Result<()> {
        for service in self.services.iter().rev() {
            service.stop().await?;
            tracing::info!("Device service '{}' stopped", service.name());
        }
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.services.len()
    }
}
