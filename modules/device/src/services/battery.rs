use super::DeviceService;
use crate::config::DeviceConfig;
use crate::events::{DeviceEvent, DeviceEventPayload};
use crate::providers::DeviceProvider;
use async_trait::async_trait;
use nova_kernel::{EventMetadata, NovaEvent, Result};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

pub struct BatteryMonitor {
    provider: Arc<dyn DeviceProvider>,
    kernel_event_bus: Arc<nova_kernel::event_bus::EventBus>,
    config: Arc<RwLock<DeviceConfig>>,
    running: AtomicBool,
    last_was_low: RwLock<bool>,
}

impl BatteryMonitor {
    pub fn new(
        provider: Arc<dyn DeviceProvider>,
        kernel: &nova_kernel::Kernel,
        config: Arc<RwLock<DeviceConfig>>,
    ) -> Self {
        Self {
            provider,
            kernel_event_bus: kernel.event_bus.clone(),
            config,
            running: AtomicBool::new(false),
            last_was_low: RwLock::new(false),
        }
    }

    pub async fn check_and_notify(&self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        if let Ok(status) = self.provider.get_battery_status().await {
            let cfg = self.config.read();
            if status.level <= cfg.battery_critical_threshold_pct {
                self.publish(DeviceEventPayload::StorageCritical {
                    free_bytes: status.level as u64,
                });
            } else if status.level <= cfg.battery_low_threshold_pct {
                let mut low = self.last_was_low.write();
                if !*low {
                    self.publish(DeviceEventPayload::BatteryLow {
                        level: status.level,
                    });
                    *low = true;
                }
            } else {
                *self.last_was_low.write() = false;
            }
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
impl DeviceService for BatteryMonitor {
    fn name(&self) -> &'static str {
        "battery_monitor"
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
