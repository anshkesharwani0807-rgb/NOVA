//! Provider abstractions for the NOVA Voice module (Milestone 7).
//!
//! Every speech capability is reached only through one of these traits. No engine is
//! hard-coded: the composition root (demo / FFI / future app shells) registers concrete
//! backends — offline-first by default, cloud only behind the Egress Gate. Future engines
//! (Whisper.cpp, Vosk, Sherpa-ONNX, Coqui/Piper TTS, Silero VAD, Porcupine, cloud ASR/TTS)
//! implement these same traits unchanged.

use async_trait::async_trait;
use nova_kernel::Result;
use tokio::sync::mpsc::UnboundedSender;

use crate::types::{AsrResult, AudioFrame, DeviceInfo, SpeechState, VoicePermissionState};

/// A cooperative cancellation token shared with a running voice operation.
#[derive(Clone, Default)]
pub struct Cancellation {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Cancellation {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Audio capture (microphone) abstraction.
#[async_trait]
pub trait AudioCaptureProvider: Send + Sync {
    /// Stable provider id (e.g. `"mock-capture"`, `"cpal-mic"`).
    fn id(&self) -> &str;

    /// Request OS microphone permission. Must be granted before capture begins.
    async fn request_permission(&self) -> Result<VoicePermissionState>;

    /// Last known permission state (without re-prompting).
    fn permission_state(&self) -> VoicePermissionState;

    /// Enumerate available capture devices.
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>>;

    /// Select the capture device by id (`None` resets to system default).
    fn set_device(&self, device_id: Option<&str>) -> Result<()>;

    /// Begin streaming capture (spawns the device thread if needed).
    async fn start_capture(&self) -> Result<()>;

    /// Stop streaming capture.
    async fn stop_capture(&self) -> Result<()>;

    /// Pull the next captured frame, or `None` when the source is exhausted (e.g. a
    /// scripted/test source finishes). Implementations must honour [`Cancellation`]
    /// passed by the caller where applicable.
    async fn next_frame(&self) -> Result<Option<AudioFrame>>;
}

/// Audio output (speaker) abstraction.
#[async_trait]
pub trait AudioOutputProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn start_playback(&self) -> Result<()>;
    /// Play one synthesized frame to the speaker.
    async fn play_frame(&self, frame: &AudioFrame) -> Result<()>;
    async fn stop_playback(&self) -> Result<()>;
    /// Whether playback is currently active.
    fn is_playing(&self) -> bool;
}

/// Voice Activity Detection (speech vs silence) abstraction.
///
/// Synchronous, per-frame classification. Real engines (Silero, webrtc-rs VAD) keep
/// hangover state internally.
pub trait VadProvider: Send + Sync {
    fn id(&self) -> &str;
    /// Classify one frame. Implementations may keep internal state for debouncing.
    fn classify(&self, frame: &AudioFrame) -> SpeechState;
}

/// Wake-word detection abstraction.
///
/// The pipeline feeds a VAD-bounded speech segment; the provider reports whether a
/// configured wake word was present.
pub trait WakeWordProvider: Send + Sync {
    fn id(&self) -> &str;
    /// The wake words this provider is currently listening for.
    fn wake_words(&self) -> Vec<String>;
    /// Process a complete speech segment and return `true` if a wake word was detected.
    fn process_segment(&self, frames: &[AudioFrame]) -> bool;
}

/// Speech-to-Text (ASR) abstraction, streaming-capable.
#[async_trait]
pub trait AsrProvider: Send + Sync {
    fn id(&self) -> &str;
    /// Active recognition language (BCP-47).
    fn language(&self) -> String;
    /// Change recognition language.
    fn set_language(&self, language: &str);

    /// Transcribe a complete speech segment to final text.
    async fn transcribe(&self, frames: &[AudioFrame]) -> Result<String>;

    /// Streaming transcription: emit partial [`AsrResult`]s on `partials` as recognition
    /// progresses, then return the final text. Honour [`Cancellation`] by stopping early.
    async fn transcribe_stream(
        &self,
        frames: &[AudioFrame],
        partials: &UnboundedSender<AsrResult>,
        cancel: &Cancellation,
    ) -> Result<String>;
}

/// Text-to-Speech (TTS) abstraction, streaming-capable.
#[async_trait]
pub trait TtsProvider: Send + Sync {
    fn id(&self) -> &str;
    /// Active voice id.
    fn voice(&self) -> String;
    /// Change the voice.
    fn set_voice(&self, voice: &str);

    /// Synthesize `text` into audio frames, streaming them on `frame_tx`. Honour
    /// [`Cancellation`] by stopping early (barge-in).
    async fn synthesize(
        &self,
        text: &str,
        frame_tx: &UnboundedSender<AudioFrame>,
        cancel: &Cancellation,
    ) -> Result<()>;
}

/// Optional noise-reduction abstraction applied to captured frames.
pub trait NoiseFilterProvider: Send + Sync {
    fn id(&self) -> &str;
    /// Return a filtered copy of the frame.
    fn filter(&self, frame: &AudioFrame) -> AudioFrame;
}
