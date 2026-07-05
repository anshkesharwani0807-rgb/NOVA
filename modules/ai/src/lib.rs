use nova_kernel::{EventMetadata, Kernel, NovaResponse, Result};
use std::sync::Arc;

pub struct AIEngine {
    kernel: Arc<Kernel>,
}

impl AIEngine {
    pub fn new(kernel: Arc<Kernel>) -> Self {
        Self { kernel }
    }

    /// Starts the AI Engine request handler for local inference
    pub async fn start(&self) -> Result<()> {
        let mut rx = self
            .kernel
            .event_bus
            .register_request_handler("ai:inference", 64)?;

        tokio::spawn(async move {
            tracing::info!("AIEngine request handler started.");
            while let Some(req) = rx.recv().await {
                tracing::debug!(
                    "[AIEngine] Processing inference request: {:?}",
                    req.metadata
                );

                let res_meta = EventMetadata::child_of(
                    &req.metadata,
                    "AIEngine",
                    Some("inference_response".to_string()),
                );

                // Skeleton returns a mock response
                let payload: Arc<String> =
                    Arc::new("Skeleton Mode: On-device models uninitialized.".to_string());
                let response = NovaResponse {
                    metadata: res_meta,
                    payload,
                };

                let _ = req.response_tx.send(Ok(response));
            }
        });

        tracing::info!("AIEngine initialized.");
        Ok(())
    }
}
