//! Offline, deterministic mock providers for the NOVA Voice module (Milestone 7).
//!
//! These let the whole voice pipeline run end-to-end — wake word, ASR, AI round-trip,
//! TTS — with **no microphone and no network**, purely for tests and the demo. They are
//! the offline-first default; real engines (Whisper.cpp, Vosk, Sherpa-ONNX, Coqui/Piper,
//! Silero, Porcupine) are swapped in by the composition root behind the same traits.

use async_trait::async_trait;
use nova_kernel::{ErrorCategory, NovaError, Result};
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::provider::{
    AsrProvider, AudioCaptureProvider, AudioOutputProvider, Cancellation, NoiseFilterProvider,
    TtsProvider, VadProvider, WakeWordProvider,
};
use crate::types::{
    AsrResult, AudioFrame, DeviceInfo, SpeechState, VoicePermissionState, DEFAULT_SAMPLE_RATE,
    DEFAULT_WAKE_WORD,
};

const FRAME_SIZE: usize = 512;

// ── Frame <-> text codec (mock only; real engines do real DSP) ────────────────

fn encode_speech(text: &str) -> Vec<AudioFrame> {
    let bytes = text.as_bytes();
    let mut payload: Vec<f32> = Vec::with_capacity(bytes.len() + 1);
    // Header slot carries the byte length as a non-zero marker.
    payload.push((bytes.len() as f32) / 1000.0 + 0.5);
    for &b in bytes {
        payload.push((b as f32) / 255.0 + 0.01);
    }
    let mut frames = Vec::new();
    for (seq, chunk) in payload.chunks(FRAME_SIZE).enumerate() {
        let mut data = vec![0.0f32; FRAME_SIZE];
        data[..chunk.len()].copy_from_slice(chunk);
        frames.push(AudioFrame {
            data,
            sample_rate: DEFAULT_SAMPLE_RATE,
            sequence: seq as u64,
        });
    }
    if frames.is_empty() {
        frames.push(AudioFrame::silence(DEFAULT_SAMPLE_RATE, 0));
    }
    frames
}

fn decode_speech(frames: &[AudioFrame]) -> Option<String> {
    let mut payload: Vec<f32> = Vec::new();
    for f in frames {
        payload.extend_from_slice(&f.data);
    }
    if payload.len() < 2 {
        return None;
    }
    let len = ((payload[0] - 0.5) * 1000.0).round() as usize;
    if len == 0 {
        return Some(String::new());
    }
    let mut bytes = Vec::with_capacity(len);
    for i in 0..len.min(payload.len() - 1) {
        let v = (payload[i + 1] - 0.01) * 255.0;
        bytes.push(v.clamp(0.0, 255.0).round() as u8);
    }
    String::from_utf8(bytes).ok()
}

// ── Capture ───────────────────────────────────────────────────────────────────

/// One step in a scripted capture sequence (used by the mock source).
#[derive(Debug, Clone)]
pub enum CaptureStep {
    Silence(u32),
    Speech(String),
}

/// A microphone source driven by a fixed script — no real audio device required.
pub struct MockAudioCapture {
    queue: RwLock<VecDeque<AudioFrame>>,
    permission: RwLock<VoicePermissionState>,
    device: RwLock<Option<String>>,
    running: RwLock<bool>,
}

impl MockAudioCapture {
    pub fn new() -> Self {
        Self {
            queue: RwLock::new(VecDeque::new()),
            permission: RwLock::new(VoicePermissionState::Granted),
            device: RwLock::new(None),
            running: RwLock::new(false),
        }
    }

    /// Build a capture source with a fixed permission outcome (used by tests).
    pub fn with_permission(permission: VoicePermissionState) -> Self {
        Self {
            queue: RwLock::new(VecDeque::new()),
            permission: RwLock::new(permission),
            device: RwLock::new(None),
            running: RwLock::new(false),
        }
    }

    /// Build a capture source that replays the given script.
    pub fn with_script(steps: &[CaptureStep]) -> Self {
        let mut queue: VecDeque<AudioFrame> = VecDeque::new();
        for step in steps {
            match step {
                CaptureStep::Silence(n) => {
                    for _ in 0..*n {
                        queue.push_back(AudioFrame::silence(DEFAULT_SAMPLE_RATE, 0));
                    }
                }
                CaptureStep::Speech(text) => {
                    for f in encode_speech(text) {
                        queue.push_back(f);
                    }
                }
            }
        }
        // Globally monotonic sequence numbers across the whole script.
        for (i, f) in queue.iter_mut().enumerate() {
            f.sequence = i as u64;
        }
        Self {
            queue: RwLock::new(queue),
            permission: RwLock::new(VoicePermissionState::Granted),
            device: RwLock::new(None),
            running: RwLock::new(false),
        }
    }

    /// The script used by the default demo pipeline: wake word, then one command.
    pub fn default_script() -> Vec<CaptureStep> {
        vec![
            CaptureStep::Silence(2),
            CaptureStep::Speech(DEFAULT_WAKE_WORD.to_string()),
            CaptureStep::Silence(2),
            CaptureStep::Speech("What is the weather today?".to_string()),
            CaptureStep::Silence(3),
        ]
    }
}

#[async_trait]
impl AudioCaptureProvider for MockAudioCapture {
    fn id(&self) -> &str {
        "mock-capture"
    }

    async fn request_permission(&self) -> Result<VoicePermissionState> {
        Ok(*self.permission.read())
    }

    fn permission_state(&self) -> VoicePermissionState {
        *self.permission.read()
    }

    async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        Ok(vec![
            DeviceInfo {
                id: "default".to_string(),
                name: "Default Mock Microphone".to_string(),
                is_default: true,
            },
            DeviceInfo {
                id: "headset".to_string(),
                name: "Mock USB Headset".to_string(),
                is_default: false,
            },
        ])
    }

    fn set_device(&self, device_id: Option<&str>) -> Result<()> {
        *self.device.write() = device_id.map(|s| s.to_string());
        Ok(())
    }

    async fn start_capture(&self) -> Result<()> {
        *self.running.write() = true;
        Ok(())
    }

    async fn stop_capture(&self) -> Result<()> {
        *self.running.write() = false;
        Ok(())
    }

    async fn next_frame(&self) -> Result<Option<AudioFrame>> {
        Ok(self.queue.write().pop_front())
    }
}

// ── Output ────────────────────────────────────────────────────────────────────

/// A speaker sink that discards frames (no real audio device).
pub struct MockAudioOutput {
    playing: RwLock<bool>,
}

impl MockAudioOutput {
    pub fn new() -> Self {
        Self {
            playing: RwLock::new(false),
        }
    }
}

impl Default for MockAudioCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for MockAudioOutput {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AudioOutputProvider for MockAudioOutput {
    fn id(&self) -> &str {
        "mock-output"
    }

    async fn start_playback(&self) -> Result<()> {
        *self.playing.write() = true;
        Ok(())
    }

    async fn play_frame(&self, _frame: &AudioFrame) -> Result<()> {
        Ok(())
    }

    async fn stop_playback(&self) -> Result<()> {
        *self.playing.write() = false;
        Ok(())
    }

    fn is_playing(&self) -> bool {
        *self.playing.read()
    }
}

// ── VAD ──────────────────────────────────────────────────────────────────────

/// Energy-based VAD: speech when frame RMS energy exceeds the threshold.
pub struct MockVad {
    threshold: f32,
}

impl MockVad {
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }
}

impl VadProvider for MockVad {
    fn id(&self) -> &str {
        "mock-vad"
    }

    fn classify(&self, frame: &AudioFrame) -> SpeechState {
        if frame.energy() > self.threshold {
            SpeechState::Speech
        } else {
            SpeechState::Silence
        }
    }
}

// ── Wake word ──────────────────────────────────────────────────────────────────

/// Detects a configured wake word within a decoded speech segment.
pub struct MockWakeWord {
    wake_words: RwLock<Vec<String>>,
}

impl MockWakeWord {
    pub fn new(wake_words: &[String]) -> Self {
        Self {
            wake_words: RwLock::new(wake_words.to_vec()),
        }
    }
}

impl WakeWordProvider for MockWakeWord {
    fn id(&self) -> &str {
        "mock-wakeword"
    }

    fn wake_words(&self) -> Vec<String> {
        self.wake_words.read().clone()
    }

    fn process_segment(&self, frames: &[AudioFrame]) -> bool {
        match decode_speech(frames) {
            Some(text) => self
                .wake_words
                .read()
                .iter()
                .any(|w| text.eq_ignore_ascii_case(w)),
            None => false,
        }
    }
}

// ── ASR ──────────────────────────────────────────────────────────────────────

/// Offline mock ASR: decodes the scripted speech segment back to text.
pub struct MockAsr {
    language: RwLock<String>,
}

impl MockAsr {
    pub fn new(language: &str) -> Self {
        Self {
            language: RwLock::new(language.to_string()),
        }
    }
}

#[async_trait]
impl AsrProvider for MockAsr {
    fn id(&self) -> &str {
        "mock-asr"
    }

    fn language(&self) -> String {
        self.language.read().clone()
    }

    fn set_language(&self, language: &str) {
        *self.language.write() = language.to_string();
    }

    async fn transcribe(&self, frames: &[AudioFrame]) -> Result<String> {
        decode_speech(frames).ok_or_else(|| {
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_VOICE_ASR_EMPTY",
                "ASR received an empty or undecodable segment",
            )
        })
    }

    async fn transcribe_stream(
        &self,
        frames: &[AudioFrame],
        partials: &tokio::sync::mpsc::UnboundedSender<AsrResult>,
        cancel: &Cancellation,
    ) -> Result<String> {
        let text = self.transcribe(frames).await?;
        // Emit a partial (first half) then the final result, honouring cancellation.
        if !text.is_empty() {
            let mid = text.len() / 2;
            let _ = partials.send(AsrResult {
                text: text[..mid].to_string(),
                is_final: false,
                confidence: 0.6,
            });
        }
        if cancel.is_cancelled() {
            return Ok(String::new());
        }
        Ok(text)
    }
}

// ── TTS ──────────────────────────────────────────────────────────────────────

/// Offline mock TTS: streams a few low-energy frames (no real synthesis).
pub struct MockTts {
    voice: RwLock<String>,
}

impl MockTts {
    pub fn new(voice: &str) -> Self {
        Self {
            voice: RwLock::new(voice.to_string()),
        }
    }
}

#[async_trait]
impl TtsProvider for MockTts {
    fn id(&self) -> &str {
        "mock-tts"
    }

    fn voice(&self) -> String {
        self.voice.read().clone()
    }

    fn set_voice(&self, voice: &str) {
        *self.voice.write() = voice.to_string();
    }

    async fn synthesize(
        &self,
        text: &str,
        frame_tx: &tokio::sync::mpsc::UnboundedSender<AudioFrame>,
        cancel: &Cancellation,
    ) -> Result<()> {
        // A short burst of quiet frames stands in for real audio output.
        for i in 0..4u64 {
            if cancel.is_cancelled() {
                break;
            }
            let mut data = vec![0.0f32; FRAME_SIZE];
            // Gentle tone-like pattern so the frame is non-trivial but inaudible/quiet.
            for (j, s) in data.iter_mut().enumerate() {
                *s = ((j as f32) * 0.001).sin() * 0.02;
            }
            if frame_tx
                .send(AudioFrame {
                    data,
                    sample_rate: DEFAULT_SAMPLE_RATE,
                    sequence: i,
                })
                .is_err()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        let _ = text;
        Ok(())
    }
}

// ── Noise filter ───────────────────────────────────────────────────────────────

/// A pass-through-ish noise filter (slight attenuation) for the offline default.
pub struct MockNoiseFilter;

impl NoiseFilterProvider for MockNoiseFilter {
    fn id(&self) -> &str {
        "mock-noise-filter"
    }

    fn filter(&self, frame: &AudioFrame) -> AudioFrame {
        let data = frame.data.iter().map(|x| x * 0.95).collect();
        AudioFrame {
            data,
            sample_rate: frame.sample_rate,
            sequence: frame.sequence,
        }
    }
}

/// Build a voice stack from explicit parameters (language, wake words, TTS voice, VAD
/// threshold and a scripted capture source). Used by tests and the composition root.
pub fn build_voice_stack(
    language: &str,
    wake_words: &[String],
    tts_voice: &str,
    vad_threshold: f32,
    script: &[CaptureStep],
) -> VoiceStack {
    VoiceStack {
        capture: Arc::new(MockAudioCapture::with_script(script)),
        output: Arc::new(MockAudioOutput::new()),
        vad: Arc::new(MockVad::new(vad_threshold)),
        wake: Arc::new(MockWakeWord::new(wake_words)),
        asr: Arc::new(MockAsr::new(language)),
        tts: Arc::new(MockTts::new(tts_voice)),
        noise: Arc::new(MockNoiseFilter),
    }
}

/// The offline-default ready-to-wire voice stack: default language, the "NOVA" wake word,
/// the default TTS voice, the default VAD threshold, and the demo capture script.
pub fn default_voice_stack() -> VoiceStack {
    build_voice_stack(
        "en-US",
        &[DEFAULT_WAKE_WORD.to_string()],
        "nova-default",
        1e-4,
        &MockAudioCapture::default_script(),
    )
}

/// A ready-to-wire bundle of providers.
pub struct VoiceStack {
    pub capture: Arc<dyn AudioCaptureProvider>,
    pub output: Arc<dyn AudioOutputProvider>,
    pub vad: Arc<dyn VadProvider>,
    pub wake: Arc<dyn WakeWordProvider>,
    pub asr: Arc<dyn AsrProvider>,
    pub tts: Arc<dyn TtsProvider>,
    pub noise: Arc<dyn NoiseFilterProvider>,
}
