//! Voice pipeline types and configuration for the NOVA Voice module (Milestone 7).
//!
//! Everything here is backend-agnostic. Concrete engines (Whisper.cpp, Vosk, Sherpa-ONNX,
//! Coqui/Piper TTS, Silero VAD, Porcupine, future cloud providers) live behind the provider
//! traits in `provider.rs` and are wired in by the composition root — never hard-coded.

use serde::{Deserialize, Serialize};

/// The default wake word NOVA listens for.
pub const DEFAULT_WAKE_WORD: &str = "NOVA";

/// Standard off-device-friendly sample rate used by the local engines.
pub const DEFAULT_SAMPLE_RATE: u32 = 16_000;

/// A single chunk of mono audio. Samples are `f32` in `[-1.0, 1.0]`.
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub data: Vec<f32>,
    pub sample_rate: u32,
    pub sequence: u64,
}

impl AudioFrame {
    /// A frame of pure silence (used to simulate pauses / non-speech).
    pub fn silence(sample_rate: u32, sequence: u64) -> Self {
        Self {
            data: vec![0.0; 512],
            sample_rate,
            sequence,
        }
    }

    /// RMS-style energy of the frame; used by VAD and the pipeline to tell speech from silence.
    pub fn energy(&self) -> f32 {
        if self.data.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = self.data.iter().map(|x| x * x).sum();
        (sum_sq / self.data.len() as f32).sqrt()
    }
}

/// Coarse speech/silence classification produced by a VAD provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeechState {
    Silence,
    Speech,
}

/// How the voice interface is triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListeningMode {
    /// Continuously listen for the wake word (always-on microphone).
    AlwaysOn,
    /// Only listen while a push-to-talk action is active.
    PushToTalk,
}

/// Result of an audio-permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoicePermissionState {
    Unknown,
    Granted,
    Denied,
}

/// A microphone / speaker device exposed by a capture/output provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// A recognized speech segment's partial or final transcription.
#[derive(Debug, Clone, PartialEq)]
pub struct AsrResult {
    pub text: String,
    pub is_final: bool,
    pub confidence: f32,
}

/// Top-level, user-facing voice configuration. All engines read from this.
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    /// BCP-47 language tag, e.g. `"en-US"`.
    pub language: String,
    /// Wake words the engine listens for (first is the default "NOVA").
    pub wake_words: Vec<String>,
    /// Trigger mode.
    pub mode: ListeningMode,
    /// Selected capture device id (`None` = system default).
    pub device_id: Option<String>,
    /// VAD energy threshold in `[0, 1]`; frames above this count as speech.
    pub vad_threshold: f32,
    /// Whether the noise filter provider is applied to captured frames.
    pub noise_filter_enabled: bool,
    /// TTS voice id (provider-specific).
    pub tts_voice: String,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            language: "en-US".to_string(),
            wake_words: vec![DEFAULT_WAKE_WORD.to_string()],
            mode: ListeningMode::AlwaysOn,
            device_id: None,
            vad_threshold: 1e-4,
            noise_filter_enabled: false,
            tts_voice: "nova-default".to_string(),
        }
    }
}
