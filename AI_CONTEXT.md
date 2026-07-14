# AI_CONTEXT.md — live state of the NOVA Project (Milestone 8)

> Companion to `BRAIN.md` and `SESSION.md`. Read before making any changes.

## Status: Milestones 1–8 COMPLETE (2026-07-14)

All M1–M8 requirements are implemented, tested, and all 4 verification gates green
(`fmt` / `clippy -D warnings` / `test --workspace` / `run -p nova_demo`).

## Project structure overview

```text
src/kernel/          nova_kernel — microkernel, event bus, config, consent, egress, module registry
modules/memory/      nova_memory — encrypted SQLite memory engine (AES-256-GCM)
modules/search/      nova_search — hybrid lexical+semantic search engine
modules/voice/       nova_voice — offline voice pipeline (VAD→wake→ASR→AI→TTS)
modules/ai/          nova_ai — Candle GGUF LLM + BERT embeddings, tool calling
modules/comms/       nova_comms — skeleton
modules/plugin_host/ nova_plugin_host — skeleton
api/ffi/             nova_ffi — C-ABI seam (31 exported functions)
api/jni/             nova_jni — Android JNI bridge (16 entry points over FFI)
apps/nova-demo/      nova_demo — CLI smoke test
```

## M8 Android Shell

### Rust side
- **`api/jni/src/lib.rs`** — 16 `extern "system"` JNI entry points wrapping `nova_ffi`.
  Names follow `Java_com_example_nova_NovaCore_<method>`.
- **`api/jni/Cargo.toml`** — `crate-type = ["cdylib"]`, deps `jni 0.21` + `nova_ffi`.
- **`api/ffi/src/lib.rs`** — extended with 16 new C-ABI functions (memory CRUD, search,
  config, activity trail, egress, health, stats, count, purge).

### Kotlin side (`D:\NOVA\`)
- **`NovaCore.kt`** — singleton, loads `libnova_jni.so`, 16 `external fun` matching JNI
- **`NovaService.kt`** — foreground service (`nova_core_channel`), `START_STICKY`
- **`NovaApplication.kt`** — starts `NovaService` on app launch
- **`ui/navigation/Routes.kt`** — 5 routes: Search, MemoryDetail(id), VisualIntelligence,
  ChatIntelligence, ActivityTrail, Settings
- **`ui/nativ/ActivityTrailScreen.kt`** — activity trail + egress log viewer
- **`ui/nativ/SettingsScreen.kt`** — config editor, health report, stats, memory count
- **`build_android.ps1`** — cross-compile for `aarch64-linux-android` / `x86_64-linux-android`

## AI Runtime (`modules/ai`) — unchanged from M6

| File | Purpose |
|------|---------|
| `lib.rs` | `AIEngine` `KernelModule`, public API, `ai:inference` handler |
| `provider.rs` | `InferenceProvider` trait, `MockProvider` |
| `candle_provider.rs` | GGUF LLM backend via Candle |
| `embedder.rs` | BERT sentence embeddings |
| `uncertainty.rs` | Confidence scoring (FR-AI-003) |
| `remote_provider.rs` | Consent-gated remote seam (FR-AI-004) |
| `model_manager.rs` | Lifecycle management (FR-AI-005) |
| `runtime.rs` | Inference engine, reasoning loop |

## Key invariants (do not break)
- Remote seam is **disabled by default**; every outbound attempt goes through `EgressGate::guard`.
- `model_manager::wait_for_provider_state` must NOT hold `RwLockReadGuard` across `.await`.
- `ai:inference` handler returns `String`; callers handle the `inference error:` Err case.
- The mock provider is the registered default; real providers added by composition root.
- JNI names must match `Java_com_example_nova_NovaCore_<method>` exactly.
- FFI functions return heap-allocated JSON `*mut c_char` that caller must free via `nova_free_string`.

## Continuing to Milestone 9 (Windows Shell)
Do NOT start M9 unless instructed. M9 will build a Windows desktop shell binding to
`nova_ffi` (WinUI / C-ABI binary). The `api/jni` crate is Android-only; Windows will use
`api/ffi` directly or via a thin C#/WinRT P/Invoke layer.
