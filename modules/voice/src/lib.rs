//! # NOVA Voice System (`nova_voice`, Milestone 7)
//!
//! An offline-first, modular voice subsystem. The media pipeline
//! (`capture → VAD → wake-word → ASR → AI Runtime → TTS → speaker`) is fully provider-driven
//! (see [`provider`]) so on-device engines (Whisper.cpp, Vosk, Sherpa-ONNX, Coqui, Piper,
//! Silero, Porcupine) and future cloud providers plug in behind traits without touching the
//! orchestration. A deterministic offline [`mock`] stack is the default for tests and the demo.
//!
//! The voice module talks to the AI Runtime **only** through the Event Bus
//! (`ai:inference` request), never touching memory/search directly (BRAIN §3, ADR-0004).
//! All activity is narrated on the Event Bus and the Activity Trail (Principle 5).

pub mod events;
pub mod mock;
pub mod pipeline;
pub mod provider;
pub mod session;
pub mod types;

pub use events::{
    AI_REQUEST_STARTED, LISTENING_STARTED, LISTENING_STOPPED, SPEECH_RECOGNITION_FAILED,
    SPEECH_RECOGNIZED, TTS_FINISHED, TTS_STARTED, VOICE_INTERRUPTED, VOICE_RESPONSE_FINISHED,
    VOICE_RESPONSE_STARTED, WAKE_WORD_DETECTED,
};
pub use mock::{default_voice_stack, MockAudioCapture, VoiceStack};
pub use pipeline::VoicePipeline;
pub use provider::*;
pub use session::{VoiceSessionCounters, VoiceSessionManager, VoiceSessionSnapshot};
pub use types::*;

use async_trait::async_trait;
use nova_kernel::{
    ErrorCategory, HealthStatus, Kernel, KernelModule, ModuleHealth, NovaError, Result,
};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// The voice module, managed by the kernel as a `KernelModule`.
///
/// `VoiceSystem` is the thin lifecycle shell: it builds the offline [`VoicePipeline`] and
/// [`VoiceSessionManager`] in [`KernelModule::initialize`], starts them in
/// [`KernelModule::start`], and tears them down in [`KernelModule::stop`]/`shutdown`.
pub struct VoiceSystem {
    kernel: Arc<Kernel>,
    pipeline: RwLock<Option<Arc<VoicePipeline>>>,
    session: RwLock<Option<Arc<VoiceSessionManager>>>,
    task: RwLock<Option<JoinHandle<()>>>,
}

impl VoiceSystem {
    pub fn new(kernel: Arc<Kernel>) -> Self {
        Self {
            kernel,
            pipeline: RwLock::new(None),
            session: RwLock::new(None),
            task: RwLock::new(None),
        }
    }

    /// Access the live pipeline (for tests / composition root tweaks).
    pub fn pipeline(&self) -> Option<Arc<VoicePipeline>> {
        self.pipeline.read().clone()
    }

    /// Access the session manager (for stats / UI).
    pub fn session_manager(&self) -> Option<Arc<VoiceSessionManager>> {
        self.session.read().clone()
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

    /// The AI Runtime must be up (and its `ai:inference` handler registered) before voice.
    fn dependencies(&self) -> Vec<&'static str> {
        vec!["ai"]
    }

    async fn initialize(&self) -> Result<()> {
        let stack = default_voice_stack();
        let cfg = VoiceConfig::default();
        let pipeline = VoicePipeline::new(self.kernel.clone(), stack, cfg);
        let session = VoiceSessionManager::new(self.kernel.clone(), pipeline.clone());
        *self.pipeline.write() = Some(pipeline);
        *self.session.write() = Some(session);
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        let pipeline = self.pipeline.read().clone().ok_or_else(|| {
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_VOICE_NOT_INIT",
                "voice pipeline was not initialized",
            )
        })?;
        let session = self.session.read().clone().ok_or_else(|| {
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_VOICE_NOT_INIT",
                "voice session manager was not initialized",
            )
        })?;

        session.start_session();
        session.start()?;

        let p = pipeline.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = p.run().await {
                tracing::warn!("[voice] pipeline stopped with error: {e}");
            }
        });
        *self.task.write() = Some(handle);

        tracing::info!("[voice] VoiceSystem started (offline pipeline running).");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let pipeline = self.pipeline.read().clone();
        if let Some(p) = pipeline {
            p.stop().await;
        }
        if let Some(h) = self.task.write().take() {
            h.abort();
        }
        let session = self.session.read().clone();
        if let Some(s) = session {
            s.end_session();
        }
        tracing::info!("[voice] VoiceSystem stopped.");
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        self.stop().await
    }

    fn health(&self) -> ModuleHealth {
        match self.pipeline.read().as_ref() {
            Some(p) if p.is_running() => ModuleHealth {
                status: HealthStatus::Healthy,
                detail: "offline voice pipeline running".to_string(),
            },
            Some(_) => ModuleHealth::degraded("voice pipeline not running"),
            None => ModuleHealth::unhealthy("voice not initialized"),
        }
    }
}
