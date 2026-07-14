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
