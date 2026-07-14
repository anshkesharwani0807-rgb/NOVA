//! Voice event definitions and publishing (Milestone 7).
//!
//! The voice module narrates its work on the kernel Event Bus and the user-facing
//! Activity Trail (Principle 5 — transparency). Every event carries a correlation id so a
//! whole voice turn (wake → ASR → AI → TTS) can be traced end to end.

use nova_kernel::{EventMetadata, Kernel, NovaEvent};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;
use uuid::Uuid;

pub const WAKE_WORD_DETECTED: &str = "voice.wake_word_detected";
pub const LISTENING_STARTED: &str = "voice.listening_started";
pub const LISTENING_STOPPED: &str = "voice.listening_stopped";
pub const SPEECH_RECOGNIZED: &str = "voice.speech_recognized";
pub const SPEECH_RECOGNITION_FAILED: &str = "voice.speech_recognition_failed";
pub const AI_REQUEST_STARTED: &str = "voice.ai_request_started";
pub const VOICE_RESPONSE_STARTED: &str = "voice.response_started";
pub const VOICE_RESPONSE_FINISHED: &str = "voice.response_finished";
pub const TTS_STARTED: &str = "voice.tts_started";
pub const TTS_FINISHED: &str = "voice.tts_finished";
pub const VOICE_INTERRUPTED: &str = "voice.interrupted";

/// Payload published on the event bus for every voice event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceEvent {
    pub kind: String,
    pub detail: String,
}

/// Publish a voice event to the bus and mirror it to the Activity Trail.
pub fn publish(kernel: &Kernel, kind: &str, correlation_id: Uuid, detail: impl Into<String>) {
    let detail = detail.into();
    let mut metadata = EventMetadata::new("voice", Some(kind.to_string()));
    metadata.correlation_id = correlation_id;

    let payload: Arc<dyn Any + Send + Sync> = Arc::new(VoiceEvent {
        kind: kind.to_string(),
        detail: detail.clone(),
    });
    let _ = kernel.event_bus.publish(NovaEvent { metadata, payload });

    nova_kernel::log_activity("voice", kind, &detail, Some(correlation_id));
}
