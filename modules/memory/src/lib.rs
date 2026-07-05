use nova_kernel::{Kernel, Result};
use std::sync::Arc;

pub struct MemoryEngine {
    kernel: Arc<Kernel>,
}

impl MemoryEngine {
    pub fn new(kernel: Arc<Kernel>) -> Self {
        Self { kernel }
    }

    /// Initializes and starts the Memory Engine background loop
    pub async fn start(&self) -> Result<()> {
        let event_bus = self.kernel.event_bus.clone();
        let mut rx = event_bus.subscribe();

        tokio::spawn(async move {
            tracing::info!("MemoryEngine background event listener started.");
            while let Ok(event) = rx.recv().await {
                // Here the MemoryEngine would process incoming capture events (e.g. photos, audio)
                // and store them in the local encrypted database.
                if event.metadata.origin_module != "MemoryEngine" {
                    tracing::debug!(
                        "[MemoryEngine] Logged observation of event ID {} from {}",
                        event.metadata.id,
                        event.metadata.origin_module
                    );
                }
            }
        });

        tracing::info!("MemoryEngine initialized.");
        Ok(())
    }
}
