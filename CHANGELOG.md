# CHANGELOG

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
