//! AI Runtime event definitions and publishing (Milestone 6).
//!
//! The runtime narrates its work on the kernel Event Bus and the user-facing Activity Trail
//! (Principle 5: transparency). Every event carries the request's correlation id so a whole
//! inference — context build, tool calls, response — can be traced end to end.

use nova_kernel::{EventMetadata, Kernel, NovaEvent};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;
use uuid::Uuid;

pub const AI_REQUEST_STARTED: &str = "ai.request_started";
pub const AI_REQUEST_FINISHED: &str = "ai.request_finished";
pub const AI_RESPONSE_GENERATED: &str = "ai.response_generated";
pub const AI_INFERENCE_FAILED: &str = "ai.inference_failed";
pub const CONTEXT_BUILT: &str = "ai.context_built";
pub const TOOL_INVOKED: &str = "ai.tool_invoked";

/// Payload published on the event bus for every AI runtime event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiEvent {
    pub kind: String,
    pub detail: String,
}

/// Publish an AI event to the event bus and mirror it to the Activity Trail.
pub fn publish(kernel: &Kernel, kind: &str, correlation_id: Uuid, detail: impl Into<String>) {
    let detail = detail.into();
    let mut metadata = EventMetadata::new("ai", Some(kind.to_string()));
    metadata.correlation_id = correlation_id;

    let payload: Arc<dyn Any + Send + Sync> = Arc::new(AiEvent {
        kind: kind.to_string(),
        detail: detail.clone(),
    });
    let _ = kernel.event_bus.publish(NovaEvent { metadata, payload });

    nova_kernel::log_activity("ai", kind, &detail, Some(correlation_id));
}
