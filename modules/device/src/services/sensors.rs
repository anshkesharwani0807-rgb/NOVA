use super::DeviceService;
use crate::events::{DeviceEvent, DeviceEventPayload};
use crate::providers::DeviceProvider;
use async_trait::async_trait;
use nova_kernel::{EventMetadata, NovaEvent, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

pub struct SensorMonitor {
    provider: Arc<dyn DeviceProvider>,
    kernel_event_bus: Arc<nova_kernel::event_bus::EventBus>,
    running: AtomicBool,
}

impl SensorMonitor {
    pub fn new(provider: Arc<dyn DeviceProvider>, kernel: &nova_kernel::Kernel) -> Self {
        Self {
            provider,
            kernel_event_bus: kernel.event_bus.clone(),
            running: AtomicBool::new(false),
        }
    }

    pub async fn check_and_notify(&self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        if let Ok(readings) = self.provider.get_sensor_readings().await {
            for reading in readings {
                let event = DeviceEvent::new(
                    Uuid::new_v4(),
                    DeviceEventPayload::SensorUpdated {
                        sensor_type: reading.sensor_type,
                        values: reading.values,
                    },
                );
                let meta = EventMetadata::new("device", Some(event.action_name().to_string()));
                let _ = self.kernel_event_bus.publish(NovaEvent {
                    metadata: meta,
                    payload: std::sync::Arc::new(event),
                });
            }
        }
    }
}

#[async_trait]
impl DeviceService for SensorMonitor {
    fn name(&self) -> &'static str {
        "sensor_monitor"
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
