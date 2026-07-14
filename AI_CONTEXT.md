# AI_CONTEXT.md — live state of the NOVA AI Runtime (Milestone 6)

> Companion to `BRAIN.md` and `SESSION.md`. Read this before touching `modules/ai`.

## Status: Milestone 6 COMPLETE (2026-07-14)

All M6 requirements are implemented, tested, and gated green
(`fmt` / `clippy -D warnings` / `test --workspace` / `run -p nova_demo`).

## What exists in `modules/ai`

| File | Purpose | Requirements |
|------|---------|--------------|
| `lib.rs` | `AIEngine` `KernelModule`, public API (`complete`/`chat`), `ai:inference` handler | M6 core |
| `provider.rs` | `InferenceProvider` trait, `MockProvider` (offline default), streaming, cancellation | FR-AI-001 |
| `candle_provider.rs` | GGUF LLM backend via Candle | FR-AI-001 |
| `embedder.rs` | BERT sentence embeddings (pure Rust) | FR-AI-002 |
| `uncertainty.rs` | Confidence scoring + honest "I don't know" (FR-AI-003) | FR-AI-003 |
| `remote_provider.rs` | Consent-gated remote seam through `EgressGate` (FR-AI-004) | FR-AI-004 |
| `model_manager.rs` | Lifecycle: register/set_active/load/unload/reload/state/wait/list | FR-AI-005 |
| `runtime.rs` | `InferenceEngine`: queue, lazy load, streaming, reasoning loop, uncertainty | M6 core |
| `context.rs` / `prompt.rs` / `session.rs` / `tool.rs` / `events.rs` | Modular context, deterministic prompt, history, tools, event narration | M6 core |

## Tests (in `tests/` + inline `#[cfg(test)]`)
- `tests/ai_runtime_tests.rs` — context, prompt, lifecycle (FR-AI-005), engine, cancellation,
  tool/reasoning, kernel lifecycle, failure recovery.
- `tests/benchmarks.rs` — NFR-PERF-002: latency, throughput, cold vs warm, memory (sysinfo).
- `embedder.rs` — dim constant, empty batch, missing-model failure, idempotent load.
- `remote_provider.rs` — consent-gated, egress-validated, audit-logged, sim support.
- `uncertainty.rs` — 11 scoring cases.

## Key invariants (do not break)
- Remote seam is **disabled by default** and **every** outbound attempt goes through
  `EgressGate::guard` (which logs to Activity Trail + Egress Log). A denied request errors;
  it never silently falls back to local.
- `model_manager::wait_for_provider_state` must NOT hold an `RwLockReadGuard` across an `.await`
  (clippy `await_holding_lock`). Keep the guard scoped, then sleep.
- `ai:inference` handler returns a `String`; callers must handle the `inference error:` Err case
  (the demo does via `match`, not `?`).
- The mock provider is the registered default; real providers are added by the composition root.

## Continuing to Milestone 7 (Voice)
Do NOT start M7 unless instructed. When you do: voice plugs into the kernel as a `KernelModule`
skeleton already exists (`modules/voice`); wire it through the event bus and (later) the AI
runtime per BRAIN §3 dependency graph (AI → Voice is a planned future edge).
