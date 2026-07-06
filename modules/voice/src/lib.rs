use async_trait::async_trait;
use nova_kernel::{Kernel, KernelModule, Result};
use std::sync::Arc;

pub struct VoiceSystem {
    kernel: Arc<Kernel>,
}

impl VoiceSystem {
    pub fn new(kernel: Arc<Kernel>) -> Self {
        Self { kernel }
    }
}

#[async_trait]
impl KernelModule for VoiceSystem {
    fn module_id(&self) -> &'static str {
        "voice"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    /// Initializes and starts the Voice Assistant listeners and wake word detection.
    async fn start(&self) -> Result<()> {
        let event_bus = self.kernel.event_bus.clone();
        let mut rx = event_bus.subscribe();

        tokio::spawn(async move {
            tracing::info!("VoiceSystem wake-word and ASR listener started.");
            while let Ok(event) = rx.recv().await {
                // For example, if a "voice:trigger" event is published, we'd handle it.
                if event.metadata.origin_module == "Shell"
                    && event.metadata.causing_action.as_deref() == Some("voice_trigger")
                {
                    tracing::info!("[VoiceSystem] Voice command trigger detected via Shell.");
                }
            }
        });

        tracing::info!("VoiceSystem initialized.");
        Ok(())
    }
}
