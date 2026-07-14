use super::DeviceService;
use crate::events::{DeviceEvent, DeviceEventPayload};
use crate::providers::DeviceProvider;
use async_trait::async_trait;
use nova_kernel::{EventMetadata, NovaEvent, Result};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

pub struct ClipboardMonitor {
    provider: Arc<dyn DeviceProvider>,
    kernel_event_bus: Arc<nova_kernel::event_bus::EventBus>,
    running: AtomicBool,
    last_content: RwLock<String>,
}

impl ClipboardMonitor {
    pub fn new(provider: Arc<dyn DeviceProvider>, kernel: &nova_kernel::Kernel) -> Self {
        Self {
            provider,
            kernel_event_bus: kernel.event_bus.clone(),
            running: AtomicBool::new(false),
            last_content: RwLock::new(String::new()),
        }
    }
}

#[async_trait]
impl DeviceService for ClipboardMonitor {
    fn name(&self) -> &'static str {
        "clipboard_monitor"
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

impl ClipboardMonitor {
    pub async fn check_and_notify(&self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        if let Ok(entry) = self.provider.read_clipboard().await {
            let mut last = self.last_content.write();
            if *last != entry.content {
                let event = DeviceEvent::new(
                    Uuid::new_v4(),
                    DeviceEventPayload::ClipboardRead {
                        content_len: entry.content.len(),
                    },
                );
                let meta = EventMetadata::new("device", Some(event.action_name().to_string()));
                let _ = self.kernel_event_bus.publish(NovaEvent {
                    metadata: meta,
                    payload: std::sync::Arc::new(event),
                });
                *last = entry.content;
            }
        }
    }
}
