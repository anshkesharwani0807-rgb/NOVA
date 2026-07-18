use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEventPayload {
    ActionExecuted {
        action: String,
        success: bool,
        detail: String,
        duration_ms: u64,
    },
    ActionBlocked {
        action: String,
        reason: String,
    },
    ActionFailed {
        action: String,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEvent {
    pub id: Uuid,
    pub correlation_id: Uuid,
    pub timestamp: DateTime<Local>,
    pub payload: InputEventPayload,
}

impl InputEvent {
    pub fn new(correlation_id: Uuid, payload: InputEventPayload) -> Self {
        Self {
            id: Uuid::new_v4(),
            correlation_id,
            timestamp: Local::now(),
            payload,
        }
    }

    pub fn action_name(&self) -> &'static str {
        match self.payload {
            InputEventPayload::ActionExecuted { .. } => "input.action_executed",
            InputEventPayload::ActionBlocked { .. } => "input.action_blocked",
            InputEventPayload::ActionFailed { .. } => "input.action_failed",
        }
    }

    pub fn description(&self) -> String {
        match &self.payload {
            InputEventPayload::ActionExecuted { action, success, detail, duration_ms } => {
                format!("Input {action}: {detail} (success={success}, {duration_ms}ms)")
            }
            InputEventPayload::ActionBlocked { action, reason } => {
                format!("Input {action} blocked: {reason}")
            }
            InputEventPayload::ActionFailed { action, error } => {
                format!("Input {action} failed: {error}")
            }
        }
    }
}
