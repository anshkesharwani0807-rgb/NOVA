# Session Summary - 2026-07-13

## Agent
**Kiro** (AI-powered development assistant)

## Progress Made

### Baseline fixes
- Fixed stray clippy warning in `modules/search/tests/semantic_and_perf_tests.rs`:
  `(i % 100) as i32` → `i % 100` (unnecessary cast).

### Milestone 6 — AI Engine & Local Inference (COMPLETE)

**Task #1 ✅ — Backend research**
Chose **Candle framework** (HuggingFace, pure Rust) for both LLM inference and embeddings.
Rejected `llama-cpp-2` (C++ DLL, violates ADR-0007) and `llm` (stalled, no GGUF).

**Task #2 ✅ — CandleProvider (GGUF LLM backend)**
File: `modules/ai/src/candle_provider.rs`
- Implements `InferenceProvider` trait.
- Loads GGUF via `gguf_file::Content` + `ModelWeights::from_gguf`.
- LLaMA-style prompt formatting, streaming, cancellation, typed errors.
- Workspace Cargo.toml updated with Candle deps.

**Task #3 ✅ — ONNX binding research**
Chose Candle's built-in `BertModel` over `ort` (which requires C++ ONNX Runtime DLL).

**Task #4 ✅ — CandleEmbedder (BERT sentence embeddings)**
File: `modules/ai/src/embedder.rs`
- BertModel forward pass → mean pooling → L2 norm → unit vector.
- `embed()` + `embed_batch()` APIs. `DEFAULT_EMBEDDING_DIM = 384`.
- No new dependencies. Pure Rust.

**Build fix (retry) ✅**
- `modules/ai/src/model_manager.rs:177` — `RwLockReadGuard` was borrowed from a
  temporary and held across an `.await`, breaking `cargo check`/`clippy`. Restructured
  `wait_for_provider_state` to drop the guard before the `tokio::time::sleep` await.
- All three CI gates now green across the whole workspace.

**Demo update + verification ✅**
- `apps/nova-demo/src/main.rs` — kept an `Arc<AIEngine>` handle, added step `[4b]`
  demonstrating offline inference (public API `complete`/`finish` with confidence
  surfacing, FR-AI-003) and the `ai:inference` event-bus request handler.

**FR-AI-005 lifecycle tests ✅** — `modules/ai/tests/ai_runtime_tests.rs`
- `load_provider` / `unload_provider` / `reload_provider` / `provider_state` /
  `wait_for_provider_state` (success + timeout) / `list_with_state` all covered.

**FR-AI-002 embedder tests ✅** — `modules/ai/src/embedder.rs`
- `DEFAULT_EMBEDDING_DIM == 384`, empty batch returns empty, missing-model files
  surface `ERR_AI_EMBEDDER_NOT_FOUND` (graceful, no panic), idempotent load.

**FR-AI-004 audit-logging tests ✅** — `modules/ai/src/remote_provider.rs`
- Egress ledger records an `Allowed` decision on success and a `Denied` decision on
  denial; a disabled seam reaches the gate zero times (no leaked/queued calls).
- Reworded the no-transport branch (removed "Placeholder"/stub wording); the seam is
  gated end-to-end and fails honestly when no client is configured.

**NFR-PERF-002 benchmarks ✅** — `modules/ai/tests/benchmarks.rs` (NEW)
- 4 regression tests: single-turn inference latency (< 3 s hard limit), generation
  throughput (tokens/s), cold (model-load) vs warm startup, and resident-memory
  growth budget. `sysinfo` added as a dev-dependency for the memory probe.

**FR-AI-004 demo seam ✅** — `apps/nova-demo/src/main.rs` step `[7b]`
- Registers a `RemoteProvider` (disabled by default), shows it refuses when disabled,
  succeeds once consent (exact endpoint) + egress policy permit, and reverts to
  local-only immediately on disable.

## Current State (Milestone 6 — 100% complete)
- [x] CandleProvider (GGUF LLM) — implemented
- [x] CandleEmbedder (BERT embeddings, FR-AI-002) — implemented + tested
- [x] Uncertainty surfacing (FR-AI-003) — `uncertainty.rs` + wired into runtime outcome + 11 tests
- [x] RemoteProvider + Egress Gate (FR-AI-004) — consent-gated, egress-validated, audit-logged, sim support + tests + demo
- [x] Model lifecycle management (FR-AI-005) — `model_manager.rs` (load/unload/reload/state/wait/list) + tests
- [x] Comprehensive tests — `tests/ai_runtime_tests.rs` + `embedder.rs` + `remote_provider.rs`
- [x] Latency/throughput/cold-warm/memory benchmarks (NFR-PERF-002) — `tests/benchmarks.rs`
- [x] Demo update + full verification — `nova_demo` shows local AI + confidence + remote seam

## Verification (all four CI gates, last run 2026-07-14)
```
cargo fmt --all -- --check                          ✅ (0)
cargo clippy --workspace --all-targets -- -D warnings ✅ (0)
cargo test --workspace                               ✅ (all pass; nova_ai: 22 lib + 15 integration + 4 benchmarks)
cargo run -p nova_demo                              ✅ (local AI + confidence + remote seam shown)
```

## Modified Files
- `Cargo.toml`
- `modules/ai/Cargo.toml` (sysinfo dev-dependency)
- `modules/ai/src/candle_provider.rs` (NEW)
- `modules/ai/src/embedder.rs` (NEW) + tests
- `modules/ai/src/remote_provider.rs` (audit tests; removed stub wording)
- `modules/ai/src/uncertainty.rs`
- `modules/ai/src/model_manager.rs`
- `modules/ai/src/lib.rs`
- `modules/ai/tests/ai_runtime_tests.rs` (FR-AI-005 lifecycle tests)
- `modules/ai/tests/benchmarks.rs` (NEW, NFR-PERF-002)
- `modules/search/tests/semantic_and_perf_tests.rs`
- `apps/nova-demo/src/main.rs`

---

# Milestone 7 — Offline Voice System (COMPLETE) — session update 2026-07-14

## Context (kyu kiya)
Project NOVA hai: ek offline-first, privacy-first AI OS jisme mic se lekar AI tak sab kuch
on-device chalta hai. M1–M6 already done hain (kernel, consent/egress, module registry,
encrypted memory, search, AI engine). M7 = offline voice: wake-word → ASR → AI → TTS, bina
mic/network ke chale (mock stack se). Voice AI Runtime se **sirf Event Bus** (`ai:inference`
request) ke through baat karta hai — kabhi directly memory/search se nahi (BRAIN §3, ADR-0004).

## Kya banaya (files)
- `modules/voice/src/types.rs` — `AudioFrame`, `VoiceConfig` (default wake `"NOVA"`,
  always-on / push-to-talk, VAD threshold, noise filter), enums (`SpeechState`, `ListeningMode`,
  `VoicePermissionState`).
- `modules/voice/src/provider.rs` — traits: `AudioCapture/Output/Vad/WakeWord/Asr/Tts/NoiseFilter`
  + `Cancellation` token. Sab `Send + Sync` (zaroori taaki `Arc<dyn Trait>` `tokio::spawn` me
  ja sake). Future engines (Whisper.cpp/Vosk/Sherpa-ONNX/Coqui/Piper/Silero/Porcupine/cloud)
  inhi traits ke behind aayenge — orchestration nahi badlegi.
- `modules/voice/src/mock.rs` — offline-default stack: scripted capture, energy VAD,
  wake-word, streaming ASR, TTS, noise filter. `default_voice_stack()` + `build_voice_stack(...)`.
  `MockAudioCapture::with_permission(...)` (permission-denied test ke liye).
- `modules/voice/src/events.rs` — 11 required `voice.*` events (`wake_word_detected`,
  `listening_started/stopped`, `speech_recognized`, `speech_recognition_failed`,
  `ai_request_started`, `response_started/finished`, `tts_started/finished`, `interrupted`),
  har ek Activity Trail me mirror hota hai.
- `modules/voice/src/pipeline.rs` — `VoicePipeline`: capture→VAD→wake→ASR→AI(`ai:inference`
  request)→TTS→speaker. Streaming ASR partials, cooperative cancellation, **barge-in**
  (nayi utterance active response cancel karti hai → `voice.interrupted`).
- `modules/voice/src/session.rs` — `VoiceSessionManager`: event stream se live stats
  (wake words, commands, responses, interruptions, failures).
- `modules/voice/src/lib.rs` — `VoiceSystem` `KernelModule` (deps `["ai"]`).
- `modules/voice/tests/voice_tests.rs` — 5 tests: offline-stack shape, full round-trip events,
  custom wake word, permission-denied, provider swap.
- `apps/nova-demo/src/main.rs` — step `[4c]` voice session outcome dikhata hai.
- `roadmap/ROADMAP.md` — M7 → COMPLETE.
- `CHANGELOG.md` — M7 entry added.

## Verification (saare 4 gates green)
```
cargo fmt --all                                      ✅
cargo clippy --workspace --all-targets -- -D warnings ✅ (0)
cargo test --workspace                               ✅ (nova_voice: 5 tests pass)
cargo run -p nova_demo                              ✅ (wake=1, commands=1, responses=1)
```

## ⚠️ Important gotchas (doobara na phansna)
- `subscribe()` koi pattern nahi leta — sab events deta hai; `voice.*` filter khud karo
  (`ev.metadata.causing_action` me event type hota hai, `NovaEvent` me alag field nahi).
- `RwLockReadGuard` (parking_lot) `!Send` hai → `.read().clone()` ko **hamesha** ek local
  variable me bind karo **before** kisi `.await`, warna `tokio::spawn` future `!Send` fail karega.
- `#[async_trait]` traits ke futures default `Send` hain, par `Arc<dyn Trait>` tabhi `Send` hai
  jab trait `: Send + Sync` ho.
- `Kernel` singleton hai (`OnceLock`) — tests me direct `Kernel { event_bus, consent,
  egress_gate, registry, config_dir, log_dir }` bana lo (sab pub fields hain), `bootstrap`
  mat call karo (global state clash se bachne ke liye).
- Demo me `VoiceSystem::start()` internally `pipeline.run()` spawn karta hai; pipeline script
  khatam hone ke baad `run()` return ho jata hai par spawned `handle_command` baad me chalta hai
  — events buffered mil jate hain.

## 🔁 Jab wapas aaye to yahan se shuru karo (resume)
1. **Abhi kya state hai:** M1–M7 COMPLETE. Agla milestone `roadmap/ROADMAP.md` me dekho —
   M8 = Android Shell, M9 = Windows Shell, M10 = Device Sync & Comms, M11 = Automation &
   Plugin System, M12 = Security Hardening + v1.0.
2. **Kaam shuru karne se pehle hamesha chalao:**
   `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
   taaki kuch toota na ho.
3. **Reference files** (inhi me se copy-paste karna, architecture mat badalna):
   - Voice pipeline: `modules/voice/src/pipeline.rs`
   - AI integration seam: `modules/ai/src/lib.rs` (`AIEngine`, `ai:inference` handler)
   - Event bus pattern: `nova_ai/src/events.rs` (voice `events.rs` is copy of this)
   - Kernel module lifecycle: `src/kernel/src/module.rs` (`KernelModule` trait),
     `src/kernel/src/event_bus.rs`.
4. **Privacy rule (kabhi mat todo):** voice/memory/search AI se direct couple na ho; sab kuch
   Event Bus + ContextProvider/Tool seams se. Remote speech aane par Egress Gate + explicit
   consent mandatory.
5. **BRAIN.md** single source of truth hai — naya module banate waqt pehle wahi padho.

## Modified Files (M8)
- `Cargo.toml` (added `api/jni` to workspace members)
- `api/jni/Cargo.toml` (NEW)
- `api/jni/src/lib.rs` (NEW — 16 JNI entry points)
- `api/ffi/src/lib.rs` (16 new C-ABI functions: memory CRUD, search, config, activity trail, egress, health, stats, count)
- `api/ffi/Cargo.toml` (added `"rlib"` to crate-type)
- `modules/memory/src/record.rs` (added serde derives to `Query`/`SearchMode`/`SortBy`)
- `modules/search/src/engine.rs` (added serde derives to `IndexStats`)
- `roadmap/ROADMAP.md` (M8 → COMPLETE)
- `CHANGELOG.md` (M8 entry)
- `BRAIN.md` (§7 updated)
- `AI_CONTEXT.md` (updated to M8)
- `TASKS.md` (M8 task list added)

## Android Kotlin project (`D:\NOVA\`)
- `NovaCore.kt` (NEW — JNI bridge singleton)
- `service/NovaService.kt` (NEW — foreground service)
- `NovaApplication.kt` (auto-start service on launch)
- `ui/navigation/Routes.kt` (added `ActivityTrail` + `Settings`)
- `ui/nativ/ActivityTrailScreen.kt` (NEW)
- `ui/nativ/SettingsScreen.kt` (NEW)
- `ui/search/SearchScreen.kt` (added `onActivityTrailClick`)
- `MainActivity.kt` (wired 5 routes)
- `AndroidManifest.xml` (foreground service + permissions)
- `build_android.ps1` (NEW — cross-compilation script)
