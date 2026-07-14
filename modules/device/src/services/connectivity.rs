use super::DeviceService;
use crate::events::{DeviceEvent, DeviceEventPayload};
use crate::providers::DeviceProvider;
use async_trait::async_trait;
use nova_kernel::{EventMetadata, NovaEvent, Result};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

pub struct ConnectivityMonitor {
    provider: Arc<dyn DeviceProvider>,
    kernel_event_bus: Arc<nova_kernel::event_bus::EventBus>,
    running: AtomicBool,
    last_online: RwLock<Option<bool>>,
}

impl ConnectivityMonitor {
    pub fn new(provider: Arc<dyn DeviceProvider>, kernel: &nova_kernel::Kernel) -> Self {
        Self {
            provider,
            kernel_event_bus: kernel.event_bus.clone(),
            running: AtomicBool::new(false),
            last_online: RwLock::new(None),
        }
    }

    pub async fn check_and_notify(&self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        if let Ok((online, net_type)) = self.provider.get_connectivity_status().await {
            let mut last = self.last_online.write();
            if last.is_none_or(|l| l != online) {
                let event = DeviceEvent::new(
                    Uuid::new_v4(),
                    DeviceEventPayload::ConnectivityChanged {
                        online,
                        network_type: net_type,
                    },
                );
                let meta = EventMetadata::new("device", Some(event.action_name().to_string()));
                let _ = self.kernel_event_bus.publish(NovaEvent {
                    metadata: meta,
                    payload: std::sync::Arc::new(event),
                });
                *last = Some(online);
            }
        }
    }
}

#[async_trait]
impl DeviceService for ConnectivityMonitor {
    fn name(&self) -> &'static str {
        "connectivity_monitor"
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
