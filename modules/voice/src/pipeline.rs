//! Voice pipeline orchestration for the NOVA Voice module (Milestone 7).
//!
//! Pipeline (offline-first, streaming, cancellable):
//! ```text
//! Microphone → VAD → Wake Word → ASR → AI Runtime → Tool Calls → TTS → Speaker
//! ```
//!
//! The pipeline runs on a background capture loop. Wake-word and command segments are
//! bounded by the VAD. After a wake word, speech segments are transcribed (streaming),
//! forwarded to the AI Runtime over the Event Bus (`ai:inference` request), and the reply
//! is synthesised and played. A new speech onset while a response is active triggers
//! barge-in (cancellation + `VoiceInterrupted`).

use nova_kernel::{ErrorCategory, Kernel, NovaError, Result};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::events::*;
use crate::mock::VoiceStack;
use crate::provider::{
    AsrProvider, AudioCaptureProvider, AudioOutputProvider, Cancellation, NoiseFilterProvider,
    TtsProvider, VadProvider, WakeWordProvider,
};
use crate::types::{AudioFrame, ListeningMode, SpeechState, VoiceConfig, VoicePermissionState};

/// A response currently being produced (AI + TTS), held so barge-in can cancel it.
struct ActiveResponse {
    cancellation: Cancellation,
    handle: tokio::task::JoinHandle<()>,
}

/// The voice pipeline: owns providers, config, and run state. Cheap to clone via `Arc`.
pub struct VoicePipeline {
    kernel: Arc<Kernel>,
    capture: RwLock<Arc<dyn AudioCaptureProvider>>,
    output: RwLock<Arc<dyn AudioOutputProvider>>,
    vad: RwLock<Arc<dyn VadProvider>>,
    wake: RwLock<Arc<dyn WakeWordProvider>>,
    asr: RwLock<Arc<dyn AsrProvider>>,
    tts: RwLock<Arc<dyn TtsProvider>>,
    noise: RwLock<Option<Arc<dyn NoiseFilterProvider>>>,
    config: RwLock<VoiceConfig>,
    running: AtomicBool,
    /// Whether we are past the wake word and currently accepting commands.
    awakened: AtomicBool,
    active: RwLock<Option<ActiveResponse>>,
}

impl VoicePipeline {
    pub fn new(kernel: Arc<Kernel>, stack: VoiceStack, config: VoiceConfig) -> Arc<Self> {
        Arc::new(Self {
            kernel,
            capture: RwLock::new(stack.capture),
            output: RwLock::new(stack.output),
            vad: RwLock::new(stack.vad),
            wake: RwLock::new(stack.wake),
            asr: RwLock::new(stack.asr),
            tts: RwLock::new(stack.tts),
            noise: RwLock::new(Some(stack.noise)),
            config: RwLock::new(config),
            running: AtomicBool::new(false),
            awakened: AtomicBool::new(false),
            active: RwLock::new(None),
        })
    }

    // ── Provider switching (no architecture change; swap at runtime) ──────────

    pub fn set_capture_provider(&self, p: Arc<dyn AudioCaptureProvider>) {
        *self.capture.write() = p;
    }
    pub fn set_output_provider(&self, p: Arc<dyn AudioOutputProvider>) {
        *self.output.write() = p;
    }
    pub fn set_vad_provider(&self, p: Arc<dyn VadProvider>) {
        *self.vad.write() = p;
    }
    pub fn set_wake_provider(&self, p: Arc<dyn WakeWordProvider>) {
        *self.wake.write() = p;
    }
    pub fn set_asr_provider(&self, p: Arc<dyn AsrProvider>) {
        *self.asr.write() = p;
    }
    pub fn set_tts_provider(&self, p: Arc<dyn TtsProvider>) {
        *self.tts.write() = p;
    }
    pub fn set_noise_filter(&self, p: Option<Arc<dyn NoiseFilterProvider>>) {
        *self.noise.write() = p;
    }

    pub fn config(&self) -> VoiceConfig {
        self.config.read().clone()
    }
    pub fn set_config(&self, cfg: VoiceConfig) {
        *self.config.write() = cfg;
    }

    /// Explicitly activate command listening (push-to-talk / always-on after wake).
    pub fn activate(&self) {
        self.awakened.store(true, Ordering::SeqCst);
    }
    pub fn deactivate(&self) {
        self.awakened.store(false, Ordering::SeqCst);
    }

    /// Cancel any in-flight response and publish an interruption event.
    fn cancel_active(&self) {
        let active = self.active.write().take();
        if let Some(a) = active {
            a.cancellation.cancel();
            a.handle.abort();
            publish(
                &self.kernel,
                VOICE_INTERRUPTED,
                Uuid::new_v4(),
                "barge-in cancelled the active response",
            );
        }
    }

    /// Main capture loop. Spawns nothing itself; runs until the source is exhausted or
    /// [`VoicePipeline::stop`] is called.
    pub async fn run(self: Arc<Self>) -> Result<()> {
        let capture = self.capture.read().clone();
        let perm = capture.request_permission().await?;
        if perm != VoicePermissionState::Granted {
            publish(
                &self.kernel,
                SPEECH_RECOGNITION_FAILED,
                Uuid::new_v4(),
                "microphone permission denied",
            );
            return Err(NovaError::new(
                ErrorCategory::Internal,
                "ERR_VOICE_NO_PERMISSION",
                "microphone permission was not granted",
            ));
        }

        capture.start_capture().await?;
        self.running.store(true, Ordering::SeqCst);

        let mut in_speech = false;
        let mut segment: Vec<AudioFrame> = Vec::new();

        loop {
            if !self.running.load(Ordering::SeqCst) {
                break;
            }
            let frame = match capture.next_frame().await? {
                Some(f) => f,
                None => break, // scripted / device source ended
            };

            let frame = if self.config.read().noise_filter_enabled {
                if let Some(n) = self.noise.read().clone() {
                    n.filter(&frame)
                } else {
                    frame
                }
            } else {
                frame
            };

            let vad = self.vad.read().clone();
            match vad.classify(&frame) {
                SpeechState::Speech => {
                    if !in_speech {
                        in_speech = true;
                        segment.clear();
                        // Barge-in: a new utterance cancels any active response.
                        self.cancel_active();
                        publish(
                            &self.kernel,
                            LISTENING_STARTED,
                            Uuid::new_v4(),
                            "speech onset",
                        );
                    }
                    segment.push(frame);
                }
                SpeechState::Silence => {
                    if in_speech {
                        in_speech = false;
                        publish(
                            &self.kernel,
                            LISTENING_STOPPED,
                            Uuid::new_v4(),
                            "speech offset",
                        );
                        let seg = std::mem::take(&mut segment);
                        self.clone().process_segment(seg).await;
                    }
                }
            }
        }

        let _ = capture.stop_capture().await;
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Handle one completed speech segment (bounded by VAD offsets).
    async fn process_segment(self: Arc<Self>, frames: Vec<AudioFrame>) {
        if !self.awakened.load(Ordering::SeqCst) {
            // Pre-wake: only listen for the wake word; ignore other speech.
            if self.wake.read().clone().process_segment(&frames) {
                self.awakened.store(true, Ordering::SeqCst);
                publish(
                    &self.kernel,
                    WAKE_WORD_DETECTED,
                    Uuid::new_v4(),
                    "wake word detected",
                );
            }
            return;
        }

        // Post-wake: stream transcription, then hand the command to the AI Runtime.
        let asr = self.asr.read().clone();
        let (partial_tx, mut partial_rx) = mpsc::unbounded_channel();
        let cancel = Cancellation::new();
        let asr_handle = {
            let frames = frames.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move { asr.transcribe_stream(&frames, &partial_tx, &cancel).await })
        };

        let final_text = {
            while let Some(p) = partial_rx.recv().await {
                publish(
                    &self.kernel,
                    SPEECH_RECOGNIZED,
                    Uuid::new_v4(),
                    format!("partial: {}", p.text),
                );
            }
            match asr_handle.await {
                Ok(Ok(text)) => text,
                Ok(Err(e)) => {
                    publish(
                        &self.kernel,
                        SPEECH_RECOGNITION_FAILED,
                        Uuid::new_v4(),
                        format!("ASR error: {e}"),
                    );
                    return;
                }
                Err(_) => {
                    publish(
                        &self.kernel,
                        SPEECH_RECOGNITION_FAILED,
                        Uuid::new_v4(),
                        "ASR task panicked",
                    );
                    return;
                }
            }
        };

        if final_text.trim().is_empty() {
            return;
        }

        // Spawn the AI + TTS turn so the capture loop stays free for barge-in.
        let cmd_cancel = cancel.clone();
        let me = self.clone();
        let handle = tokio::spawn(async move { me.handle_command(final_text, cmd_cancel).await });
        *self.active.write() = Some(ActiveResponse {
            cancellation: cancel,
            handle,
        });
    }

    /// Run one full voice turn: ASR text → AI Runtime → TTS playback.
    async fn handle_command(self: Arc<Self>, text: String, cancel: Cancellation) {
        let cid = Uuid::new_v4();
        publish(
            &self.kernel,
            AI_REQUEST_STARTED,
            cid,
            format!("command: {text}"),
        );

        let meta = nova_kernel::EventMetadata::new("voice", Some("ai_request".to_string()));
        let payload: Arc<String> = Arc::new(text.clone());
        let reply = match self
            .kernel
            .event_bus
            .request("ai:inference", meta, payload)
            .await
        {
            Ok(resp) => resp
                .payload
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_default(),
            Err(e) => {
                publish(
                    &self.kernel,
                    SPEECH_RECOGNITION_FAILED,
                    cid,
                    format!("AI request failed: {e}"),
                );
                return;
            }
        };

        if reply.starts_with("inference error") {
            publish(
                &self.kernel,
                SPEECH_RECOGNITION_FAILED,
                cid,
                "AI runtime returned an error",
            );
            return;
        }

        publish(
            &self.kernel,
            VOICE_RESPONSE_STARTED,
            cid,
            format!("reply: {reply}"),
        );

        self.clone().synthesize_and_play(reply, &cancel).await;

        publish(
            &self.kernel,
            VOICE_RESPONSE_FINISHED,
            cid,
            "voice turn complete",
        );

        // Push-to-talk reverts to idle after each turn; always-on stays awakened.
        if self.config.read().mode == ListeningMode::PushToTalk {
            self.awakened.store(false, Ordering::SeqCst);
        }
    }

    /// Stream-synthesize `text` and play it, honouring cancellation (barge-in).
    async fn synthesize_and_play(self: Arc<Self>, text: String, cancel: &Cancellation) {
        let output = self.output.read().clone();
        let _ = output.start_playback().await;
        publish(
            &self.kernel,
            TTS_STARTED,
            Uuid::new_v4(),
            format!("synthesizing {} chars", text.len()),
        );

        let (frame_tx, mut frame_rx) = mpsc::unbounded_channel();
        let tts = self.tts.read().clone();
        let tts_handle = {
            let text = text.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                let _ = tts.synthesize(&text, &frame_tx, &cancel).await;
            })
        };

        while let Some(frame) = frame_rx.recv().await {
            if cancel.is_cancelled() {
                break;
            }
            let _ = output.play_frame(&frame).await;
        }
        tts_handle.abort();
        let _ = output.stop_playback().await;

        publish(
            &self.kernel,
            TTS_FINISHED,
            Uuid::new_v4(),
            "playback complete",
        );
    }

    /// Stop the pipeline: ends the capture loop and cancels any active response.
    pub async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        // Abort any in-flight response, but do not narrate an "interruption" — this is a
        // shutdown, not a barge-in.
        if let Some(a) = self.active.write().take() {
            a.handle.abort();
        }
        let capture = self.capture.read().clone();
        let _ = capture.stop_capture().await;
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
