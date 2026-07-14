//! Integration tests for the NOVA Voice module (Milestone 7).
//!
//! These exercise the full offline pipeline end-to-end against the AI Runtime over the Event
//! Bus, plus the offline default stack, custom wake words, and the microphone permission path.
//! No microphone and no network are used — only deterministic mock providers.

use nova_ai::AIEngine;
use nova_kernel::{
    ConsentManager, EgressGate, EgressPolicy, EventBus, Kernel, KernelModule, ModuleRegistry,
};
use nova_voice::events::*;
use nova_voice::mock::{
    build_voice_stack, default_voice_stack, CaptureStep, MockAsr, MockAudioCapture, MockTts,
};
use nova_voice::pipeline::VoicePipeline;
use nova_voice::types::{VoiceConfig, VoicePermissionState};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Build a fresh, fully-offline kernel (no global bootstrap singleton) for isolation.
fn make_kernel() -> Arc<Kernel> {
    let event_bus = Arc::new(EventBus::new(1024));
    let consent = Arc::new(ConsentManager::new());
    let egress_gate = Arc::new(EgressGate::new(consent.clone(), EgressPolicy::OfflineOnly));
    let registry = Arc::new(ModuleRegistry::new());
    Arc::new(Kernel {
        event_bus,
        consent,
        egress_gate,
        registry,
        config_dir: PathBuf::from("."),
        log_dir: PathBuf::from("."),
    })
}

/// Register the offline AI Runtime and its `ai:inference` request handler.
async fn setup_ai(kernel: &Arc<Kernel>) {
    let ai = AIEngine::new(kernel.clone());
    ai.initialize().await.unwrap();
    ai.start().await.unwrap();
}

/// Collect voice.* event kinds from the bus until `stop_at` is seen or the deadline passes.
async fn collect_voice_events(
    rx: &mut tokio::sync::broadcast::Receiver<nova_kernel::NovaEvent>,
    stop_at: &str,
    timeout: Duration,
) -> HashSet<String> {
    let mut seen = HashSet::new();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if seen.contains(stop_at) {
            break;
        }
        match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
            Ok(Ok(ev)) => {
                if let Some(k) = ev.metadata.causing_action.as_deref() {
                    if k.starts_with("voice.") {
                        seen.insert(k.to_string());
                    }
                }
            }
            Ok(Err(_)) => break, // all senders gone
            Err(_) => {
                if tokio::time::Instant::now() >= deadline {
                    break;
                }
            }
        }
    }
    seen
}

#[test]
fn default_stack_is_offline_first() {
    let stack = default_voice_stack();
    assert_eq!(stack.capture.id(), "mock-capture");
    assert_eq!(stack.output.id(), "mock-output");
    assert_eq!(stack.vad.id(), "mock-vad");
    assert_eq!(stack.wake.id(), "mock-wakeword");
    assert_eq!(stack.asr.id(), "mock-asr");
    assert_eq!(stack.tts.id(), "mock-tts");
    assert_eq!(stack.noise.id(), "mock-noise-filter");
}

#[tokio::test]
async fn full_pipeline_round_trip_emits_events() {
    let kernel = make_kernel();
    setup_ai(&kernel).await;

    let pipeline = VoicePipeline::new(
        kernel.clone(),
        default_voice_stack(),
        VoiceConfig::default(),
    );
    let mut rx = kernel.event_bus.subscribe();
    let p = pipeline.clone();
    let task = tokio::spawn(async move {
        let _ = p.run().await;
    });

    let seen =
        collect_voice_events(&mut rx, VOICE_RESPONSE_FINISHED, Duration::from_secs(10)).await;
    task.abort();

    for ev in [
        WAKE_WORD_DETECTED,
        LISTENING_STARTED,
        SPEECH_RECOGNIZED,
        AI_REQUEST_STARTED,
        VOICE_RESPONSE_STARTED,
        TTS_STARTED,
        TTS_FINISHED,
        VOICE_RESPONSE_FINISHED,
    ] {
        assert!(seen.contains(ev), "expected voice event not emitted: {ev}");
    }
}

#[tokio::test]
async fn custom_wake_word_is_detected() {
    let kernel = make_kernel();
    setup_ai(&kernel).await;

    let script = vec![
        CaptureStep::Silence(2),
        CaptureStep::Speech("COMPUTER".to_string()),
        CaptureStep::Silence(2),
        CaptureStep::Speech("what time is it".to_string()),
        CaptureStep::Silence(3),
    ];
    let stack = build_voice_stack(
        "en-US",
        &["COMPUTER".to_string()],
        "nova-default",
        1e-4,
        &script,
    );
    let pipeline = VoicePipeline::new(kernel.clone(), stack, VoiceConfig::default());
    let mut rx = kernel.event_bus.subscribe();
    let p = pipeline.clone();
    let task = tokio::spawn(async move {
        let _ = p.run().await;
    });

    let seen =
        collect_voice_events(&mut rx, VOICE_RESPONSE_FINISHED, Duration::from_secs(10)).await;
    task.abort();

    assert!(
        seen.contains(WAKE_WORD_DETECTED),
        "custom wake word 'COMPUTER' was not detected; events: {seen:?}"
    );
}

#[tokio::test]
async fn permission_denied_publishes_failure() {
    let kernel = make_kernel();
    setup_ai(&kernel).await;

    // A capture source whose permission request is denied.
    let mut stack = default_voice_stack();
    stack.capture = Arc::new(MockAudioCapture::with_permission(
        VoicePermissionState::Denied,
    ));

    let pipeline = VoicePipeline::new(kernel.clone(), stack, VoiceConfig::default());
    let mut rx = kernel.event_bus.subscribe();

    let result = pipeline.run().await;
    assert!(
        result.is_err(),
        "pipeline must fail when mic permission denied"
    );

    // The failure is narrated on the event bus.
    let mut seen = HashSet::new();
    while let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
        if let Some(k) = ev.metadata.causing_action.as_deref() {
            seen.insert(k.to_string());
        }
        if seen.contains(SPEECH_RECOGNITION_FAILED) {
            break;
        }
    }
    assert!(
        seen.contains(SPEECH_RECOGNITION_FAILED),
        "permission denial must publish a recognition-failure event"
    );
}

#[tokio::test]
async fn provider_swap_still_completes_turn() {
    let kernel = make_kernel();
    setup_ai(&kernel).await;

    let stack = default_voice_stack();
    let pipeline = VoicePipeline::new(kernel.clone(), stack, VoiceConfig::default());
    // Swap in fresh mock ASR/TTS instances behind the same traits (provider abstraction).
    pipeline.set_asr_provider(Arc::new(MockAsr::new("fr-FR")));
    pipeline.set_tts_provider(Arc::new(MockTts::new("nova-fr")));

    let mut rx = kernel.event_bus.subscribe();
    let p = pipeline.clone();
    let task = tokio::spawn(async move {
        let _ = p.run().await;
    });

    let seen =
        collect_voice_events(&mut rx, VOICE_RESPONSE_FINISHED, Duration::from_secs(10)).await;
    task.abort();

    assert!(
        seen.contains(VOICE_RESPONSE_FINISHED),
        "pipeline must still complete a turn after provider swap"
    );
}
