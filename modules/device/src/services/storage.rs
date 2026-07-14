use super::DeviceService;
use crate::events::{DeviceEvent, DeviceEventPayload};
use crate::providers::DeviceProvider;
use async_trait::async_trait;
use nova_kernel::{EventMetadata, NovaEvent, Result};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

pub struct StorageMonitor {
    provider: Arc<dyn DeviceProvider>,
    kernel_event_bus: Arc<nova_kernel::event_bus::EventBus>,
    running: AtomicBool,
    last_free: RwLock<Option<u64>>,
}

impl StorageMonitor {
    pub fn new(provider: Arc<dyn DeviceProvider>, kernel: &nova_kernel::Kernel) -> Self {
        Self {
            provider,
            kernel_event_bus: kernel.event_bus.clone(),
            running: AtomicBool::new(false),
            last_free: RwLock::new(None),
        }
    }

    pub async fn check_and_notify(&self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        if let Ok(info) = self.provider.get_storage_info().await {
            let mut last = self.last_free.write();
            let threshold_low = 500_000_000;
            let threshold_critical = 100_000_000;
            if info.free_bytes <= threshold_critical {
                self.publish(DeviceEventPayload::StorageCritical {
                    free_bytes: info.free_bytes,
                });
            } else if info.free_bytes <= threshold_low && last.is_none_or(|l| l > threshold_low) {
                self.publish(DeviceEventPayload::StorageLow {
                    free_bytes: info.free_bytes,
                });
            }
            *last = Some(info.free_bytes);
        }
    }

    fn publish(&self, payload: DeviceEventPayload) {
        let event = DeviceEvent::new(Uuid::new_v4(), payload);
        let meta = EventMetadata::new("device", Some(event.action_name().to_string()));
        let _ = self.kernel_event_bus.publish(NovaEvent {
            metadata: meta,
            payload: std::sync::Arc::new(event),
        });
    }
}

#[async_trait]
impl DeviceService for StorageMonitor {
    fn name(&self) -> &'static str {
        "storage_monitor"
    }

    async fn start(&self) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
