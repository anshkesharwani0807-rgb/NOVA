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

pub struct NotificationMonitor {
    provider: Arc<dyn DeviceProvider>,
    kernel_event_bus: Arc<nova_kernel::event_bus::EventBus>,
    config: Arc<RwLock<DeviceConfig>>,
    running: AtomicBool,
    known_notifications: RwLock<Vec<String>>,
}

impl NotificationMonitor {
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
            known_notifications: RwLock::new(vec![]),
        }
    }

    pub async fn check_and_notify(&self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        if let Ok(notifications) = self.provider.get_notifications().await {
            let cfg = self.config.read();
            for notification in &notifications {
                if cfg.notification_filter_enabled
                    && cfg
                        .notification_filter_patterns
                        .iter()
                        .any(|p| notification.package_name.contains(p))
                {
                    continue;
                }
                let mut known = self.known_notifications.write();
                let key = format!("{}:{}", notification.package_name, notification.title);
                if !known.contains(&key) {
                    let event = DeviceEvent::new(
                        Uuid::new_v4(),
                        DeviceEventPayload::NotificationPosted {
                            package: notification.package_name.clone(),
                            title: notification.title.clone(),
                        },
                    );
                    let meta = EventMetadata::new("device", Some(event.action_name().to_string()));
                    let _ = self.kernel_event_bus.publish(NovaEvent {
                        metadata: meta,
                        payload: std::sync::Arc::new(event),
                    });
                    known.push(key);
                }
            }
        }
    }
}

#[async_trait]
impl DeviceService for NotificationMonitor {
    fn name(&self) -> &'static str {
        "notification_monitor"
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
