# AI_CONTEXT.md — live state of the NOVA Project (v0.15.0)

> Companion to `BRAIN.md` and `SESSION.md`. Read before making any changes.

## Status: ALL MILESTONES 1–13 COMPLETE

All M1–M13 requirements are implemented, tested, and all 4 verification gates green
(`fmt` / `clippy -D warnings` / `test --workspace` / `run -p nova_demo`).

## Project structure overview

```text
src/kernel/          nova_kernel — microkernel, event bus, config, consent, egress, module registry
modules/memory/      nova_memory — encrypted SQLite memory engine (AES-256-GCM)
modules/search/      nova_search — hybrid lexical+semantic search engine
modules/voice/       nova_voice — offline voice pipeline (VAD→wake→ASR→AI→TTS)
modules/ai/          nova_ai — Candle GGUF LLM + BERT embeddings, tool calling
modules/vision/      nova_vision — offline vision intelligence (OCR, caption, detection, face, search)
modules/sync/        nova_sync — E2E encrypted cross-device sync
modules/knowledge/   nova_knowledge — memory analysis, knowledge graph, timeline, recall, summaries
modules/automation/  nova_automation — workflow engine, trigger/scheduler/execution/history
modules/plugin_sdk/  nova_plugin_sdk — plugin SDK (manifest, permissions, sandbox, lifecycle, storage, events)
modules/comms/       nova_comms — skeleton
modules/device/      nova_device — device API providers (battery, camera, clipboard, etc.)
modules/plugin_host/ nova_plugin_host — plugin sandbox + consequence gate
api/ffi/             nova_ffi — C-ABI seam (31 exported functions)
api/jni/             nova_jni — Android JNI bridge (16 entry points over FFI)
apps/nova-demo/      nova_demo — CLI smoke test
apps/nova-desktop/   nova_desktop — Windows desktop GUI (egui/eframe)
```

## Key invariants (do not break)
- Remote seam is **disabled by default**; every outbound attempt goes through `EgressGate::guard`.
- `model_manager::wait_for_provider_state` must NOT hold `RwLockReadGuard` across `.await`.
- `ai:inference` handler returns `String`; callers handle the `inference error:` Err case.
- The mock provider is the registered default; real providers added by composition root.
- JNI names must match `Java_com_example_nova_NovaCore_<method>` exactly.
- FFI functions return heap-allocated JSON `*mut c_char` that caller must free via `nova_free_string`.
- All ML behind `VisionProvider` trait — swap mock for real engine at composition root.
- Sync is **disabled by default** (`SyncConfig::enabled = false`).
- Irreversible automation actions always require ConsequenceGate consent.
- `AutomationEngine` uses `KernelModule` lifecycle; `scheduler_loop` runs in a `tokio::spawn` task.
  The `event_bus` reference must be cloned before moving into the spawn block.
- `PluginManager` is the single facade for all plugin operations (register, install, enable, disable, uninstall).
- Permissions are declared at plugin registration and granted before any lifecycle hook fires.
- Plugin sandbox validates actions against granted permissions; network access requires explicit `internet.access` permission.
- Plugin storage is isolated per plugin; `PluginStorage` provides in-memory and disk-based backends.
- 9 `PluginEventType` variants are published on the event bus for all lifecycle transitions.
- Plugins receive only `PluginContext` — no direct kernel/module access.

