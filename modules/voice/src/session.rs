//! Voice session management (Milestone 7).
//!
//! `VoiceSessionManager` tracks a live "awake" listening session and watches the voice event
//! stream to maintain live statistics (wake words, commands, responses, interruptions,
//! failures). It does not own media processing — that lives in [`crate::pipeline`].

use crate::events::*;
use crate::pipeline::VoicePipeline;
use nova_kernel::{Kernel, Result};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

/// Rolling counters for the current voice session.
#[derive(Clone, Copy, Debug, Default)]
pub struct VoiceSessionCounters {
    pub wake_words: u64,
    pub commands_recognized: u64,
    pub responses_spoken: u64,
    pub interruptions: u64,
    pub recognition_failures: u64,
}

/// A point-in-time view of the voice session for diagnostics/UI.
#[derive(Clone, Copy, Debug)]
pub struct VoiceSessionSnapshot {
    pub session_id: Uuid,
    pub active: bool,
    pub elapsed_ms: u64,
    pub counters: VoiceSessionCounters,
}

/// One "awake" period of the assistant.
struct VoiceSession {
    id: Uuid,
    started: Instant,
    counters: VoiceSessionCounters,
}

/// Coordinates activation and live statistics for the voice module.
pub struct VoiceSessionManager {
    kernel: Arc<Kernel>,
    current: RwLock<Option<VoiceSession>>,
}

impl VoiceSessionManager {
    pub fn new(kernel: Arc<Kernel>, _pipeline: Arc<VoicePipeline>) -> Arc<Self> {
        Arc::new(Self {
            kernel,
            current: RwLock::new(None),
        })
    }

    /// Begin a new listening session (statistics tracking). Pipeline activation is owned by
    /// the pipeline itself (wake word for always-on, PTT trigger otherwise).
    pub fn start_session(&self) {
        *self.current.write() = Some(VoiceSession {
            id: Uuid::new_v4(),
            started: Instant::now(),
            counters: VoiceSessionCounters::default(),
        });
    }

    /// End the current session (clears stats tracking).
    pub fn end_session(&self) {
        self.current.write().take();
    }

    /// Subscribe to the event bus and maintain live counters. Call once from the module's
    /// `start()`.
    pub fn start(self: Arc<Self>) -> Result<()> {
        let mut rx = self.kernel.event_bus.subscribe();
        let me = self.clone();
        tokio::spawn(async move {
            while let Ok(ev) = rx.recv().await {
                if let Some(kind) = ev.metadata.causing_action.as_deref() {
                    if kind.starts_with("voice.") {
                        me.record(kind);
                    }
                }
            }
        });
        Ok(())
    }

    fn record(&self, kind: &str) {
        if let Some(session) = self.current.write().as_mut() {
            let c = &mut session.counters;
            match kind {
                WAKE_WORD_DETECTED => c.wake_words += 1,
                SPEECH_RECOGNIZED => c.commands_recognized += 1,
                VOICE_RESPONSE_FINISHED => c.responses_spoken += 1,
                VOICE_INTERRUPTED => c.interruptions += 1,
                SPEECH_RECOGNITION_FAILED => c.recognition_failures += 1,
                _ => {}
            }
        }
    }

    /// Snapshot of the current session for diagnostics / UI.
    pub fn snapshot(&self) -> VoiceSessionSnapshot {
        match self.current.read().as_ref() {
            Some(s) => VoiceSessionSnapshot {
                session_id: s.id,
                active: true,
                elapsed_ms: s.started.elapsed().as_millis() as u64,
                counters: s.counters,
            },
            None => VoiceSessionSnapshot {
                session_id: Uuid::nil(),
                active: false,
                elapsed_ms: 0,
                counters: VoiceSessionCounters::default(),
            },
        }
    }
}
