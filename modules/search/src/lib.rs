use nova_kernel::{EventMetadata, Kernel, NovaResponse, Result};
use std::sync::Arc;

pub struct UniversalSearch {
    kernel: Arc<Kernel>,
}

impl UniversalSearch {
    pub fn new(kernel: Arc<Kernel>) -> Self {
        Self { kernel }
    }

    /// Initializes and starts the Universal Search request listener
    pub async fn start(&self) -> Result<()> {
        let mut rx = self
            .kernel
            .event_bus
            .register_request_handler("search:query", 64)?;

        tokio::spawn(async move {
            tracing::info!("UniversalSearch request handler started.");
            while let Some(req) = rx.recv().await {
                tracing::debug!(
                    "[UniversalSearch] Processing query request: {:?}",
                    req.metadata
                );

                let res_meta = EventMetadata::child_of(
                    &req.metadata,
                    "UniversalSearch",
                    Some("search_query_response".to_string()),
                );

                // Skeleton returns a mock result
                let payload: Arc<String> =
                    Arc::new("Skeleton Mode: Indexing empty. No documents searched.".to_string());
                let response = NovaResponse {
                    metadata: res_meta,
                    payload,
                };

                let _ = req.response_tx.send(Ok(response));
            }
        });

        tracing::info!("UniversalSearch initialized.");
        Ok(())
    }
}
