use crate::error::{ErrorCategory, NovaError, Result};
use chrono::{DateTime, Local};
use parking_lot::RwLock;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventMetadata {
    pub id: Uuid,
    pub correlation_id: Uuid,
    pub timestamp: DateTime<Local>,
    pub origin_module: String,
    pub causing_action: Option<String>,
}

impl EventMetadata {
    pub fn new(origin_module: &str, causing_action: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            correlation_id: Uuid::new_v4(),
            timestamp: Local::now(),
            origin_module: origin_module.to_string(),
            causing_action,
        }
    }

    pub fn child_of(
        parent: &EventMetadata,
        origin_module: &str,
        causing_action: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            correlation_id: parent.correlation_id,
            timestamp: Local::now(),
            origin_module: origin_module.to_string(),
            causing_action,
        }
    }
}

/// Generic wrapped event carrying metadata and actual payload
#[derive(Clone)]
pub struct NovaEvent {
    pub metadata: EventMetadata,
    pub payload: Arc<dyn Any + Send + Sync>,
}

impl std::fmt::Debug for NovaEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NovaEvent")
            .field("metadata", &self.metadata)
            .field("payload_type", &self.payload.type_id())
            .finish()
    }
}

#[derive(Debug)]
pub struct NovaRequest {
    pub name: String,
    pub metadata: EventMetadata,
    pub payload: Arc<dyn Any + Send + Sync>,
    pub response_tx: oneshot::Sender<Result<NovaResponse>>,
}

#[derive(Clone)]
pub struct NovaResponse {
    pub metadata: EventMetadata,
    pub payload: Arc<dyn Any + Send + Sync>,
}

impl std::fmt::Debug for NovaResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NovaResponse")
            .field("metadata", &self.metadata)
            .field("payload_type", &self.payload.type_id())
            .finish()
    }
}

pub struct EventBus {
    event_tx: broadcast::Sender<NovaEvent>,
    request_channels: RwLock<HashMap<String, mpsc::Sender<NovaRequest>>>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (event_tx, _) = broadcast::channel(capacity);
        Self {
            event_tx,
            request_channels: RwLock::new(HashMap::new()),
        }
    }

    /// Publish an event to all subscribers (Pub/Sub)
    pub fn publish(&self, event: NovaEvent) -> Result<usize> {
        // Log the activity to diagnostic logs and activity trail
        let module = event.metadata.origin_module.clone();
        let action = event
            .metadata
            .causing_action
            .clone()
            .unwrap_or_else(|| "publish_event".to_string());

        crate::logger::log_activity(
            &module,
            &action,
            &format!("Published event (ID: {})", event.metadata.id),
            Some(event.metadata.correlation_id),
        );

        match self.event_tx.send(event) {
            Ok(subscribers) => Ok(subscribers),
            Err(_) => Ok(0), // 0 active subscribers
        }
    }

    /// Subscribe to all published events
    pub fn subscribe(&self) -> broadcast::Receiver<NovaEvent> {
        self.event_tx.subscribe()
    }

    /// Register a module as the exclusive handler for a Request name
    pub fn register_request_handler(
        &self,
        name: &str,
        buffer_size: usize,
    ) -> Result<mpsc::Receiver<NovaRequest>> {
        let mut channels = self.request_channels.write();
        if channels.contains_key(name) {
            return Err(NovaError::new(
                ErrorCategory::Kernel,
                "ERR_EVENTBUS_HANDLER_EXISTS",
                &format!("A handler for request '{}' is already registered", name),
            ));
        }

        let (tx, rx) = mpsc::channel(buffer_size);
        channels.insert(name.to_string(), tx);
        Ok(rx)
    }

    /// Send a request and wait for a response (Request/Response)
    pub async fn request(
        &self,
        name: &str,
        metadata: EventMetadata,
        payload: Arc<dyn Any + Send + Sync>,
    ) -> Result<NovaResponse> {
        let sender = {
            let channels = self.request_channels.read();
            channels.get(name).cloned()
        };

        let tx = sender.ok_or_else(|| {
            NovaError::new(
                ErrorCategory::Kernel,
                "ERR_EVENTBUS_NO_HANDLER",
                &format!("No handler registered for request '{}'", name),
            )
        })?;

        let (response_tx, response_rx) = oneshot::channel();
        let req = NovaRequest {
            name: name.to_string(),
            metadata: metadata.clone(),
            payload,
            response_tx,
        };

        crate::logger::log_activity(
            &metadata.origin_module,
            &format!("request:{}", name),
            &format!("Sent request '{}'", name),
            Some(metadata.correlation_id),
        );

        tx.send(req).await.map_err(|_| {
            NovaError::new(
                ErrorCategory::Kernel,
                "ERR_EVENTBUS_SEND_FAIL",
                &format!("Failed to send request '{}' to handler queue", name),
            )
        })?;

        match response_rx.await {
            Ok(res) => res,
            Err(_) => Err(NovaError::new(
                ErrorCategory::Kernel,
                "ERR_EVENTBUS_RESP_TIMEOUT",
                &format!(
                    "Request '{}' handler dropped response channel without replying",
                    name
                ),
            )),
        }
    }
}
