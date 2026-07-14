# CHANGELOG

## [Unreleased] - Milestone 8 (Android Shell)

### Added
- **`api/jni/` crate:** JNI bridge (`cdylib`) with 16 entry points wrapping `nova_ffi`:
  `nativeInit`, `nativeShutdown`, `nativeMemoryInsert`, `nativeMemorySearch`,
  `nativeMemoryFindById`, `nativeMemoryDelete`, `nativeMemoryList`, `nativeMemoryCount`,
  `nativeSearchText`, `nativeSearchNl`, `nativeGetActivityTrail`, `nativeGetEgressLog`,
  `nativeGetConfig`, `nativeUpdateConfig`, `nativeGetHealthReport`, `nativeSearchStats`.
  Naming follows the `Java_com_example_nova_NovaCore_<method>` JNI convention.
- **`nova_ffi` extensions:** 16 new C-ABI functions (memory CRUD, search, config R/W,
  activity trail, egress log, health report) — all returning JSON strings freed via
  `nova_free_string`.
- **Kotlin `NovaCore` object:** singleton in `com.example.nova` with `external fun`
  matching every JNI entry point; loads `libnova_jni.so` via `System.loadLibrary`.
- **`NovaService`:** Android foreground service (notification channel `nova_core_channel`,
  `START_STICKY`) that calls `NovaCore.init`/`shutdown` — auto-started from
  `NovaApplication.onCreate`.
- **Compose UI screens:**
  - `ActivityTrailScreen` — displays activity trail + egress log from native core
  - `SettingsScreen` — config JSON editor, health report, search stats, memory count
  - `SearchScreen` — added `onActivityTrailClick` callback with timeline icon
- **Navigation:** `Route.ActivityTrail` + `Route.Settings` added to sealed interface;
  wired into `MainActivity` entry provider (5 routes total).
- **`build_android.ps1`:** cross-compilation script for `aarch64-linux-android` /
  `x86_64-linux-android`; copies `libnova_jni.so` to `app/src/main/jniLibs/<abi>/`.

### Changed
- `nova_ffi` crate type includes `"rlib"` in addition to `"cdylib"` and `"staticlib"`.
- `modules/memory/src/record.rs`: added `serde::Serialize`/`Deserialize` to `Query`,
  `SearchMode`, `SortBy`.
- `modules/search/src/engine.rs`: added `serde::Serialize`/`Deserialize` to `IndexStats`.

### Build
- All 4 verification gates green: `fmt`, `clippy -D warnings`, `test --workspace`,
  `run -p nova_demo`.

---

## [Unreleased] - Milestone 7 (Offline Voice System)

### Added
- **`nova_voice` module (`modules/voice`):** the offline-first voice subsystem, integrated as a
  kernel `KernelModule` (`VoiceSystem`) that depends only on the AI Runtime through the Event Bus
  (`ai:inference` request) — never touching memory/search directly (BRAIN §3, ADR-0004).
- **Provider abstractions (`provider.rs`):** `AudioCaptureProvider`, `AudioOutputProvider`,
  `VadProvider`, `WakeWordProvider`, `AsrProvider`, `TtsProvider`, `NoiseFilterProvider` traits
  plus a `Cancellation` token. Whisper.cpp / Vosk / Sherpa-ONNX / Coqui / Piper / Silero /
  Porcupine / future cloud engines plug in with no orchestration changes.
- **Voice pipeline (`pipeline.rs`):** `capture → VAD → wake-word → ASR → AI → TTS → speaker`
  with streaming ASR partials, cooperative cancellation, and barge-in (a new utterance cancels
  the active response and emits `voice.interrupted`).
- **Required event-bus events (`events.rs`):** `voice.wake_word_detected`, `voice.listening_started`,
  `voice.listening_stopped`, `voice.speech_recognized`, `voice.speech_recognition_failed`,
  `voice.ai_request_started`, `voice.response_started`, `voice.response_finished`,
  `voice.tts_started`, `voice.tts_finished`, `voice.interrupted` — each mirrored to the Activity
  Trail.
- **Offline-default mock stack (`mock.rs`):** deterministic scripted capture/VAD/wake/ASR/TTS
  providers so the whole pipeline runs with no microphone and no network (tests + demo default).
- **Session manager (`session.rs`):** live per-session statistics (wake words, commands,
  responses, interruptions, failures) gathered from the event stream.
- **Config (`types.rs`):** `VoiceConfig` with default wake word `"NOVA"`, custom/multiple wake
  words, always-on and push-to-talk modes, VAD threshold, noise filter toggle.
- **Demo (`apps/nova-demo`, step `[4c]`):** prints the voice session outcome and shows the
  pipeline stayed fully on-device.
- **Tests (`modules/voice/tests/voice_tests.rs`):** offline-stack shape, full round-trip event
  emission, custom wake word, microphone-permission-denied path, and provider swapping.

### Privacy
- Default is offline; no microphone audio is retained before the wake word, and remote speech
  providers (future) must go through the Egress Gate + explicit consent.

---

## [Unreleased] - Milestone 6 (AI Engine & Local Inference)

### Added
- **CandleProvider (GGUF LLM backend):** `modules/ai/src/candle_provider.rs` implements the
  `InferenceProvider` trait, loads GGUF via `gguf_file` + `ModelWeights::from_gguf`, with
  LLaMA-style prompt formatting, streaming, cancellation, and typed errors.
- **CandleEmbedder (BERT embeddings):** `modules/ai/src/embedder.rs` runs a `BertModel` forward
  pass → mean pooling → L2 norm, exposing `embed()`/`embed_batch()` at `DEFAULT_EMBEDDING_DIM = 384`.
- **Uncertainty surfacing (FR-AI-003):** `modules/ai/src/uncertainty.rs` estimates a confidence
  score and rewrites low-confidence responses; surfaced on the inference outcome.
- **RemoteProvider + Egress Gate (FR-AI-004):** `modules/ai/src/remote_provider.rs` reaches a
  consent-gated cloud backend only through the kernel `EgressGate`.
- **Model lifecycle management:** `modules/ai/src/model_manager.rs` with lazy loading, active
  model selection, and `wait_for_provider_state`.
- **Inference engine + reasoning loop + tool calling:** `modules/ai/src/runtime.rs`, `tool.rs`,
  `session.rs`, `context.rs`, `prompt.rs`, `events.rs`; `ai:inference` event-bus handler.
- **AI demo showcase:** `apps/nova-demo/src/main.rs` step `[4b]` exercises offline inference and
  the confidence score.
- **Remote acceleration seam demo (FR-AI-004):** `apps/nova-demo/src/main.rs` step `[7b]` registers
  a `RemoteProvider` (disabled by default), shows it refuses while disabled, succeeds once consent
  (exact endpoint) + egress policy permit, and reverts to local-only immediately on disable.
- **Model lifecycle tests (FR-AI-005):** `modules/ai/tests/ai_runtime_tests.rs` covers
  `load_provider` / `unload_provider` / `reload_provider` / `provider_state` /
  `wait_for_provider_state` (success + timeout) / `list_with_state`.
- **Embedder unit tests (FR-AI-002):** `modules/ai/src/embedder.rs` covers the `384` dimension
  constant, empty-batch, graceful missing-model failure, and idempotent load.
- **Remote-provider audit tests (FR-AI-004):** `modules/ai/src/remote_provider.rs` verifies the
  egress ledger records `Allowed` on success and `Denied` on denial, and that a disabled seam
  reaches the gate zero times (no leaked/queued calls).
- **Performance benchmark suite (NFR-PERF-002):** `modules/ai/tests/benchmarks.rs` (new) asserts
  single-turn inference latency, generation throughput, cold-vs-warm startup, and resident-memory
  growth stay within budget; `sysinfo` added as a dev-dependency for the memory probe.

### Fixed
- **M6 build break:** `model_manager.rs` held an `RwLockReadGuard` across an `.await`; restructured
  `wait_for_provider_state` to drop the guard before sleeping. All CI gates green.

## [0.1.0] - 2026-07-11

### Added
- **Natural Language Query Parser:** Implemented `QueryParser` in `modules/search/src/parser.rs` to allow filtering via `tag:`, `cat:`, and `src:`.
- **Performance Benchmarking Suite:** Created `modules/search/tests/benchmarks.rs` to measure search, indexing, and update latency.
- **Search Query Enhancements:** Added `SearchQuery::text()` method for flexible query building.

### Fixed
- **Search Engine SQLite Error:** Fixed `ESCAPE expression must be a single character` by updating `like_escape` to use backslash (``) instead of an empty string.
- **Schema Version Test:** Corrected `test_schema_version` to expect version 2.
- **Clippy Warnings:** Removed unused imports and needless borrows in the search module.

### Performance
- Verified search latency meets NFR-PERF-003 (< 800ms) for datasets up to 10,000 records.
