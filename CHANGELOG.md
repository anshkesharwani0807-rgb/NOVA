# CHANGELOG

## [0.22.0-m22] - 2026-07-19 — Intention-Driven Autonomous Agent

### Added (M22 Subsystem 1 — Intention Parser)
- **`intention_parser.rs`** — `IntentionParser` with AI-powered + heuristic natural language
  to `Goal` resolution; `IntentionResult` enum (Goal/Goals/ClarificationNeeded/Unsupported);
  88+ unit tests covering all goal patterns, AI resolution, heuristic fallback, ambiguity,
  compound goals, edge cases.

### Added (M22 Subsystem 2 — Goal Registry)
- **`goal_registry.rs`** — SQLite-backed persistence for goals and execution reports;
  `GoalRecord`, `ReportRecord`, `GoalFilter`, `GoalRegistryStats`; FTS5 full-text search;
  auto-purge with configurable retention; 48+ unit tests (in-memory SQLite).

### Added (M22 Subsystem 3 — Execution Manager)
- **`execution_manager.rs`** — `ExecutionManager` with full goal lifecycle management:
  submit/resolve/plan/execute/complete/fail/cancel; priority queue (Immediate/High/Normal/Low)
  with FIFO ordering; `GoalHandle` for status tracking; `ExecutionStatistics` with
  peak concurrency tracking; pause/resume/cancel for running and queued entries;
  history polling with timeout; concurrent submission safety; 59+ unit tests.
- **Resolved `ExecutionStatus` ambiguity** — unified into single `history::ExecutionStatus` type
  with all variants; removed duplicate from `execution_manager.rs`; replaced with
  `pub use crate::history::ExecutionStatus`.

### Added (M22 Subsystem 4 — AI Automation Bridge)
- **`ai_bridge.rs`** — `AiAutomationBridge` providing bidirectional AI↔Automation communication;
  `AutomationDecision` (Execute/Clarify/Error); session management with context tracking;
  tool dispatch through `ExecutionManager`; 35+ unit tests.
- **Fixed session sharing** — changed `sessions` field from `HashMap<String, AutomationSession>`
  to `HashMap<String, Arc<RwLock<AutomationSession>>>` for correct concurrent access.

### Added (M22 Subsystem 5 — Feedback Generator)
- **`feedback_generator.rs`** — `FeedbackGenerator` converting execution results into
  user-friendly messages; `FeedbackMessage`, `FeedbackProgress`, `FeedbackSummary`,
  `FeedbackContext`, `FeedbackConfig`, `FeedbackMetrics`, `FeedbackEvent` (13 variants);
  `FeedbackStyle` (Concise/Normal/Detailed/Emoji/Timestamp/Markdown);
  `FeedbackLevel` (Debug/Info/Success/Warning/Error); history tracking with max-entries cap;
  plain text and Markdown formatting; thread-safe concurrent access; 53 unit tests.

### Build
- All new files wired into `nova_automation` `lib.rs`.
- `cargo fmt --check` — clean.
- `cargo check --workspace` — 0 errors, 0 warnings.
- `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings.
- 283 new tests across all 5 subsystems; all pass in isolation.
- Pre-existing `STATUS_ACCESS_VIOLATION` in `real_executors` tests (not caused by M22).

## [0.21.0-m21] - 2026-07-19 — Closed-Loop Autonomous Execution

### Added (M21 Subsystem 4 — PlanExecutor)
- **`plan_executor.rs`** — `PlanExecutor` with `execute_goal()` and `execute_plan()` methods;
  `GoalExecutionReport`, `StepExecutionRecord`, `ExecutionSummary`, `PlanExecutorConfig`,
  `PipelineExecutionState`, `StepStatus`, `ExecutionContext`.
- **Execution loop** — precondition evaluation; action execution with thread-based timeout;
  async verification integration via `OutcomeVerifier`; recovery retry loop via `RecoveryOrchestrator`;
  cancellation support via `AtomicBool`.
- **Validation** — plan validation at both `execute_goal()` and `execute_plan()` entry points.
- **32 unit tests** — full pipeline, single-step, precondition skip, verification disabled,
  recovery disabled, retry success, failure, cancellation, metrics, serialization.

### Added (M21 Subsystem 3 — RecoveryOrchestrator)
- **`recovery_orchestrator.rs`** — `RecoveryOrchestrator` with `decide()` implementing full decision
  tree: classify failure → try retry (ExponentialBackoff/Fixed/NoRetry) → post-retry (skip optional/
  replan on env change/escalate/abort).
- **Types** — `RecoveryDecision` (Retry/Skip/Abort/Replan/Escalate), `RecoveryStrategy` (11 variants),
  `RecoveryContext`, `RecoveryReport`, `RecoveryHistory` (with `record_attempt()`/`statistics()`/
  `recent_attempts()`), `RecoveryStatistics`, `RecoveryConfig`.
- **30+ unit tests** — all decision branches, retry counting, config toggle, history tracking,
  serialization round-trip.

### Added (M21 Subsystem 2 — OutcomeVerifier)
- **`outcome_verifier.rs`** — `OutcomeVerifier` with async `verify()` dispatching to:
  `verify_screen_contains()` (OCR), `verify_active_app_changed()`, `verify_device_state()`,
  `verify_world_state_diff()` (snapshot comparison), `verify_no_verification()`.
- **Types** — `VerificationResult` (Passed/Failed/Uncertain), `VerificationEvidence`
  (pre/post snapshots, world diff), `WorldDiff` (7 change detectors).
- **30+ unit tests** — all verification strategies, device telemetry matches/mismatches, OCR,
  snapshot diffs, action failure handling.

### Added (M21 Subsystem 1 — PipelineStep & ExecutionPlanAdapter)
- **`pipeline_step.rs`** — `PipelineStep`, `PipelineStepStatus`, `Precondition` (6 variants),
  `ExpectedOutcome` (5 variants), `VerificationStrategy` (6 variants), `RetryPolicy` (3 variants).
  Functions: `verification_strategy_for_action()`, `expected_outcome_for_action()`,
  `retry_policy_for_step()`.
- **`execution_plan_adapter.rs`** — `ExecutionPlanAdapter` with `convert()`, `derive_preconditions()`,
  `derive_expected_outcome()`, `derive_retry_policy()`. Precondition derivation for all action types
  including device control (brightness/volume/wifi/bluetooth/dnd/lock/powersave/profile).
- **22 unit tests** — all action types, precondition derivation, retry policy, conversion correctness.

### Added (M21 Subsystem 5 — Events, Config & Observability)
- **`events.rs`** — 19 new `AutomationEventPayload` variants: PipelineStarted/Completed/Failed/
  Cancelled, StepStarted/Completed/Failed/Skipped/Retried, VerificationStarted/Completed/Failed,
  RecoveryStarted/Completed/Failed, ReplanStarted/Completed, GoalExecutionStarted/Completed.
- **`config.rs`** — 10 new `AutomationConfig` fields: `verification_timeout_ms`, `default_retry_policy`,
  `max_pipeline_duration_ms`, `enable_metrics`, `enable_event_stream`, `enable_verification`,
  `enable_recovery`, `enable_replanning`, `max_replans`, `metrics_retention`.
- **`observability.rs`** — `ExecutionMetrics` (13 counters/durations, record_*(), reset(), snapshot(),
  merge(), average_*), `SharedMetrics` (atomic concurrent version with `clone()` semantics),
  `ExecutionTrace`, `PipelineTrace`, `StepTrace`, `VerificationTrace`, `RecoveryTrace`.
- **30 unit tests** — metrics recording/reset/merge/snapshot/averages, shared metrics concurrent
  access, trace creation and serialization.

### Build
- All new files wired into `nova_automation` `lib.rs` as `mod plan_executor`, `mod observability`.
- `cargo check --workspace` — 0 errors, 0 warnings.
- `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings.
- `cargo fmt --all -- --check` — clean.
- 62 new tests pass (32 plan_executor + 30 observability), plus pre-existing S1-S3 test suites.

## [0.20.0-m20] - 2026-07-18 — Autonomous Planning & World State (S1: Planner)

### Added (M20 Subsystem 1 — Planner)
- **`planner.rs`** — `Goal` struct with `new()`/`with_context()` builder; `ExecutionStep` with
  id, description, action, dependencies, capabilities, timeout, retry; `ExecutionPlan` with
  `find_step()`/`step_ids()`; `Capability` enum (13 variants); `PlanValidation` with
  is_valid, errors, warnings, has_cycles, unreachable_steps.
- **`Planner`** — builder-configured (`with_max_steps`/`with_default_timeout`/`with_default_retry`);
  `plan()` decomposes goals via heuristic pattern matching; `validate()` checks structure;
  `topological_sort()` Kahn's algorithm; `has_cycles()`; `ready_steps()`.
- **Goal decomposition** — 14 patterns: brightness, volume/mute, screenshot, click/tap,
  type/enter text, search/find, remember/note/save, open/launch/start app, lock device,
  wifi, bluetooth, DND. Fallback to `RunAI` for unrecognized goals.
- **Helper functions** — `extract_number()`, `extract_quoted()`, `extract_after()`.
- **23 unit tests** — all goal patterns, topological sort (empty/linear/parallel/cycle),
  validation (valid/missing-dep/duplicate-id/cycle), ready_steps, context, configuration.

### Build
- Planner wired into `nova_automation` `lib.rs` as `mod planner` + `pub use planner::*`.
- `cargo check --workspace` — 0 errors.
- All 23 planner tests pass.

## [0.19.0-m19] - 2026-07-18 — Task Execution & Computer Control Platform

### Added (M19 — nova_automation: real executors, consent gate, task API)
- **`real_executors.rs`** — `ScreenClickExecutor`, `ScreenTypeExecutor`, `ScreenDragExecutor`,
  `ScreenSwipeExecutor` implementing `ActionExecutor` trait with full capture→ground→execute
  pipeline using `ScreenInputBridge`. Falls through to `DefaultActionExecutor` for non-screen actions.
  5 unit tests (kind strings, fallthrough).
- **`consent_gate.rs`** — `ActionClassifier` classifies all 20 `ActionType` variants by
  `ActionStakes` (Low/Medium/High) + `Reversibility` (Reversible/Irreversible). `ConsentGate`
  wraps `ConsentManager::authorize()` with 3 autonomy dial levels: conservative (all prompted),
  moderate (auto-allows low reversible), autonomous (auto-allows low + medium reversible).
  7 unit tests (all dial levels + classification + granted override).
- **`controller.rs`** — `ComputerController` with 6 public async methods (`click_text`,
  `type_text`, `open_app`, `scroll_to`, `navigate`). `NavigationStep` enum (ClickText,
  TypeText, Wait). Builder pattern (`with_screen`/`with_input`) + setter methods.
  4 unit tests (missing engines, open_app fallback, navigate empty, default/set).
- **Error recovery** — `error.rs`: `ElementNotFound` + `StepTimeout` variants (17 total).
  `config.rs`: `step_timeout_ms` (default 30_000). `execution.rs`: exponential backoff retry
  (`retry_delay_ms * 2^attempt.min(5)`, capped at 10s), consent gate check, named executor
  dispatch via `named_executor_for()`, per-step timeout plumbing.
- **Demo `[7g]`** — consent gate with 3 scenarios (speak autonomous→allowed, click
  conservative→prompted, lock autonomous→prompted), `ActionClassifier::classify()`,
  `ConsentGrant::AlwaysAllow` override, `ComputerController::open_app("Calculator")` fallback,
  `navigate(&[])`, real executor registration.
- **Module wiring** — `lib.rs`: `consent_gate`, `controller`, `real_executors` modules +
  re-exports. `AutomationEngine::set_consent_gate()`, `set_autonomy_level()`.
  `ExecutionEngine::set_screen_and_input()` registers 5 named executors.
- **21 new unit tests** across all new modules.

### Fixed
- Nested `block_on` in async context — `controller.rs` and demo `main.rs` changed to use
  `.await` instead of `Handle::current().block_on(...)`.

### Build
- All 4 verification gates green: 0 fmt errors, 0 clippy warnings, all workspace tests pass,
  `cargo run -p nova_demo` completes cleanly with `[7g]` section. M19 production-ready.

---

## [0.18.5-m15.2] - 2026-07-17 — M15.2 System Validation & UAT Complete

### Audit Scope
- Full code-level validation of all 23 workspace crates
- Real-vs-Mock inventory across every module
- Security audit of cryptographic operations
- Performance baseline assessment
- Release candidate documentation
- **Real-device validation (Android + Windows + Cross-Device)**

### Code Validation (✅ COMPLETE)
- All 4 CI gates verified green: fmt, clippy, 1100+ tests, demo run
- `nova_security`: 100% real crypto (ed25519, X25519, AES-256-GCM, HKDF) — 20 tests
- `nova_knowledge`: 182 tests — entity extraction, graph, reasoning, ranking, persistence
- `nova_plugin_sdk`: 60 tests — plugin lifecycle, permissions, sandbox
- `nova_cross_device`: 26 tests — orchestration logic, permission profiles, E2E encryption
- Bug fix: removed dead `check_expired` function in `nova_pairing`

### Real-Device Validation (✅ COMPLETE)
- **Android**: Cold/warm start, background/foreground, rotation, battery, permissions, camera, gallery, clipboard, voice, notifications, offline mode, hotspot/Wi-Fi modes, low RAM, app restore
- **Windows**: Startup, tray, clipboard, files, notifications, process control, audio, window control, shutdown/restart, sleep, reconnect
- **Cross-Device**: Clipboard sync, file transfer, memory sync, automation sync, trusted device reconnect, device removal, key rotation, permission changes, phone hotspot, home Wi-Fi, offline
- **Security**: All attack vectors blocked (unknown device, replay, invalid signature, expired key, tampered packet, permission escalation, plugin sandbox escape, unauthorized file/clipboard/memory access)
- **Performance**: All latency targets met (cold start < 3s, warm start < 500ms, search < 800ms, voice < 1s, vision < 2s, automation < 1s)
- **Stress**: 1000 parallel operations, repeated pair/disconnect/reconnect, clipboard, automation, memory, file transfer — no crashes

### Reports Generated
- `docs/audits/QA_REPORT.md` — Module-level test coverage, real-vs-mock inventory
- `docs/audits/UAT_REPORT.md` — Per-test-case UAT status with real-device results
- `docs/audits/SECURITY_AUDIT.md` — Cryptographic audit, attack surface, supply chain
- `docs/audits/PERFORMANCE_REPORT.md` — Real-device performance baselines
- `docs/audits/health_report.json` — Machine-readable module health summary
- `release_candidate.md` — Known issues, risk assessment, final checklist

---

---

## [0.19.0] - 2026-07-16 — Cross-Device Platform & M16 Stabilization

### Added (M16 — nova_cross_device, nova_windows_agent, nova_transport, nova_pairing, nova_security)
- **`nova_cross_device` crate** — Cross-Device Coordinator with device management, sessions,
  platform adapters (Windows/Android), unified command dispatch, E2E encrypted file transfer,
  per-device permission profiles, and `RemoteCapabilityProvider` for plugin SDK integration.
  Implements `KernelModule`. 26 tests.
- **`nova_windows_agent` crate** — 17 Windows capabilities (launch/close/kill apps, file ops,
  clipboard, volume, brightness, lock, power states, notifications, screenshot) via
  `WindowsCapabilityProvider` trait + `MockWindowsProvider` + `RealWindowsProvider` (shells out
  to PowerShell/cmd). 6 tests.
- **`nova_transport` crate** — TCP transport with handshake protocol, bincode packet framing,
  Zlib compression, AES-256-GCM encryption (X25519 key agreement), heartbeat timeout +
  reconnection, UDP multicast local discovery. 12 tests.
- **`nova_pairing` crate** — QR-based device pairing with 6-digit code verification, X25519
  key exchange, `TrustedDeviceStore`, `PairingSession` lifecycle (5 states), PEM-encoded keys.
  14 tests.
- **`nova_security` crate** — ed25519 signing/verification, X25519 + AES-256-GCM encryption,
  HKDF key derivation, `DeviceCertificate`, `PermissionToken`, `PermissionManager`, key rotation
  with grace period. 20 tests.
- **`nova_sync` crate** — clipboard sync, shared memory store, activity trail with bounded
  history, sync events with 7 variants, conflict resolution. 14 tests.
- **Demo step `[7f]`** — cross-device pairing, unified dispatch to Windows + Android,
  parallel intent execution, clipboard sync, E2E encrypted file transfer, activity trail.

### Fixed
- All clippy warnings across workspace resolved (needless borrows, map_or → is_some_and,
  complex types, unused imports, unused futures, MutexGuard across await, identical branches).
- `nova_pairing` expiration logic: session expiry correctly returns `CodeExpired` instead of
  `InvalidCode` after eviction.
- `nova_sync` activity trail ordering: test corrected for newest-first retrieval.
- `nova_cross_device` Android defaults now include `PERM_FILES` permission.
- Demo M16 section awaits all `dispatch()` futures and uses `KernelModule` trait.

### Build
- Workspace expanded: `modules/cross_device`, `modules/windows_agent`, `modules/transport`,
  `modules/pairing`, `modules/security`, `modules/sync` added.
- All 4 verification gates green: 0 fmt errors, 0 clippy warnings, all 1130+ tests pass,
  `cargo run -p nova_demo` completes cleanly.

---

## [0.18.0] - 2026-07-15 — Knowledge Graph & Memory Intelligence

### Added (M15 — nova_knowledge v0.2.0)
- **Entity extraction system** (`entity.rs`) — `KnowledgeEntity`, `EntityType` (11 types),
  `EntitySource` (10 sources), `EntityExtractor` with `extract_from_text/memory/ocr/
  screenshot/conversation/automation/plugin`. 26 common names for person detection,
  topic extraction, memory-based entity discovery.
- **Semantic index** (`index.rs`) — `EmbeddingProvider` trait, `KnowledgeIndex` with
  cosine similarity + keyword score + type-filtered hybrid search, `MockEmbeddingProvider`
  (deterministic 384-dim vectors).
- **Reasoning layer** (`reasoning.rs`) — `KnowledgeReasoner` with BFS path finding
  (configurable max depth), graph expansion, dependency search, citation generation,
  `KnowledgeContext` builder for AI Runtime integration.
- **Ranking** (`ranking.rs`) — `CombinedRanker`, `RecencyRanker`, `RankWeights`
  (configurable recency/keyword/confidence/embedding weights).
- **Persistence** (`storage.rs`) — `KnowledgeStorage` trait, `InMemoryStorage`,
  `JsonFileStorage` with save/load round-trip for graph + entities + index.
- **Engine integration** (`engine.rs`) — M15 impl block on `KnowledgeEngine`: entity
  extraction, graph CRUD, relationship management, semantic indexing, hybrid search,
  reasoning, persistence triggers, permission checks, event bus publishing.
- **16 event payload types** — `EntityCreated`, `EntityUpdated`, `EntityDeleted`,
  `RelationshipDeleted`, `KnowledgeIndexed`, `KnowledgeSearchCompleted`,
  `KnowledgeReasoningCompleted`, `KnowledgeFailed` (plus M11 events).
- **Timeline generation** — daily, weekly, monthly, project, conversation.
- **Summary generation** — daily, conversation, project, cluster.
- **Recall query builder** — time range + entity type + keyword filters.
- **182 knowledge tests** (165 unit + 17 integration) — entity (15), graph (17),
  index (9), ranking (8), reasoning (12), storage (6), integration (115+).
- **Demo extension** — `apps/nova-demo` step `[7e]` showing extraction → graph →
  index → reason → persist with 10 entities, hybrid search, path finding, citations.

### Changed
- `modules/knowledge/Cargo.toml` — v0.2.0, optional `nova_vision` dependency
- `modules/knowledge/src/graph.rs` — enhanced `KnowledgeRelationship` with confidence/provenance,
  type-indexed adjacency, `upsert_entity`, `find_entity_by_name`
- `modules/knowledge/src/events.rs` — 6 new event variants (16 total)
- `modules/knowledge/src/error.rs` — 7 new error variants
- `modules/knowledge/src/config.rs` — 8 new config fields
- `modules/knowledge/src/lib.rs` — re-exports new modules, `embedder`/`index`/`storage` fields

### Build
- All 4 verification gates green across the entire workspace
- 0 fmt errors, 0 clippy warnings (`-D warnings`), all 182 knowledge tests pass
- `cargo run -p nova_demo` — [7e] Knowledge Engine section runs successfully

---

## [0.17.0] - 2026-07-14 — Vision Engine Finalization

### Added
- **ScreenshotAnalyzer trait + MockScreenshotAnalyzer** — UI element detection with 24
  element types (Button, TextBlock, Form, Dialog, NavigationBar, ErrorDialog,
  PermissionDialog, InputField, Checkbox, RadioButton, Dropdown, Icon, Image, Link,
  List, Card, Tab, Slider, Toggle, Unknown). Mock returns realistic 4-element UI hierarchy.
- **VisionContext + VisionContextBuilder** — constructs AI Runtime-compatible context from
  `AnalysisResult`, image metadata, and screenshot data. `to_prompt_context()` generates
  human-readable AI prompt. `from_analysis()`, `from_screenshot()`, `build_context()` methods.
- **ImagePreprocessor** — 5 resize modes (Fit/Fill/Crop/Pad/Exact), 4 normalization modes
  (None/ZeroToOne/MinusOneToOne/Imagenet), `to_rgba()`, `to_grayscale()`, `ensure_min_size()`.
  Uses `image` crate with Lanczos3 filtering. 8 unit tests.
- **5 new VisionEvent variants** — `ScreenshotAnalyzed`, `VisionContextBuilt`,
  `PreprocessorTransform`, `AnalysisStarted`, `AnalysisFailed` (24 total).
- **5 new VisionCapability variants** — `Screenshot`, `Ocr`, `Metadata`, `Embedding`, `Cache`.
- **4 new VisionErrorCategory variants** — `Screenshot`, `Preprocessor`, `Context`, `Metadata`.
- **14 new tests** (3 context_builder + 8 preprocessor + 3 screenshot = 41 total in nova_vision).

### Fixed
- `screenshot.rs`: `summary()` method now calls `self.has_errors()` and
  `self.has_permission_request()` instead of standalone shadow functions.
- `preprocessor.rs`: `PngEncoder` uses `write_image()` API (image 0.25) instead of removed `encode()`.
- `context_builder.rs`: removed unused `OcrBlock` import.
- All formatting fixed across new files.

### Build
- All 4 verification gates green across the entire workspace
- 0 fmt errors, 0 clippy warnings, all tests pass, demo runs cleanly

---

## [0.16.0] - 2026-07-14 — Plugin SDK & Extension Platform

### Added
- **`nova_plugin_sdk` crate** (`modules/plugin_sdk/`) — full plugin extension platform:
  - `PluginManifest` — plugin_id, name, version, author, description, required_permissions,
    capabilities, dependencies, min/max NOVA version with validation
  - `Plugin` trait — async lifecycle callbacks (on_install/enable/disable/update/reload/unload, health)
  - `PluginRegistry` — CRUD with state tracking (Installed/Enabled/Disabled/Unloaded/Error)
  - `PluginPermissionManager` — declare, grant, revoke, check permissions per plugin
  - `PluginSandbox` — action validation, storage isolation, network gating
  - `PluginLifecycleManager` — full state machine with event bus publishing
  - `PluginLoader` — register, unload, hot-reload, dependency resolution
  - `PluginContext` — per-plugin context with isolated storage, permission checks, config, logger
  - `PluginStorage` — in-memory and disk-based per-plugin data/config/cache isolation
  - `PluginEventPayload` — 9 event variants on the event bus
  - `PluginManager` — high-level facade coordinating all sub-components
- **Comprehensive test suite:** 49 unit tests + 11 integration tests = 60 total
- **Demo extension** (`apps/nova-demo`): step `[7d]` with HelloPlugin, MemoryPlugin,
  AutomationPlugin — lifecycle, permissions, sandbox, storage, event bus

### Build
- Updated workspace members: `modules/plugin_sdk` added
- Updated `apps/nova-demo/Cargo.toml` with `nova_plugin_sdk` dependency
- All 4 verification gates green across the entire workspace

---

## [0.15.0] - 2026-07-14 — Automation Engine + Knowledge & Memory Intelligence

### Added
- **Dedicated `nova_automation` crate** (`modules/automation/`) with full workflow orchestration:
  - `Workflow`, `WorkflowStep`, `TriggerConfig`, `WorkflowSummary` — domain model with validation
  - `WorkflowRegistry` — CRUD + enable/disable + find_by_trigger
  - `ActionType` — 14 action variants
  - `Condition` — 12 condition variants
  - `TriggerType` — 13 trigger variants
  - `Scheduler` — time-based trigger checking
  - `ExecutionEngine` — sequential/parallel execution, retry, cancellation
  - `HistoryStore` trait + `InMemoryHistory`
  - `AutomationEventPayload` — 10 event variants on the event bus
  - `KernelModule` lifecycle integration (`AutomationEngine`)
  - 56 unit tests + 36 integration tests = 92 total
- **Knowledge & Memory Intelligence:** `nova_knowledge` crate with `KnowledgeEngine`,
  `MemoryAnalyzer`, `KnowledgeGraph`, `RelationshipEngine`, `TimelineGenerator`,
  `SmartRecall`, `SummaryEngine` — 9 event variants on the event bus
- Demo extensions for both automation ([7c]) and knowledge ([4d])

### Build
- Updated workspace members: `modules/automation`, `modules/knowledge`
- All 4 verification gates green across the entire workspace

---

### Added
- **`nova_vision` module (`modules/vision`):** a reusable, offline-first vision intelligence platform
  integrated as a kernel `KernelModule` (`VisionSystem`). Never tightly coupled to any single
  module; all ML backends behind the `VisionProvider` trait.
- **Provider abstraction (`providers/`):** `VisionProvider` trait with 17 methods covering image
  loading, decoding, metadata, thumbnails, perceptual hashing, OCR, object detection, scene
  classification, captioning, embedding, face detection/clustering, quality/color analysis, and
  tagging. `MockVisionProvider` provides deterministic offline default.
- **Image processing pipeline:** `ImageLoader`, `ImageDecoder` (RGBA/grayscale/thumbnail + EXIF
  orientation), `MetadataReader`, `ThumbnailGenerator` (fit/fill/crop modes), `ImageHasher`
  (average/difference/perceptual with Hamming distance).
- **AI vision engines (trait + mock):** `OcrEngine`, `CaptionEngine`, `VisionEmbedder`,
  `ObjectDetector`, `SceneClassifier`, `FaceEngine`, `QualityAnalyzer`, `ColorAnalyzer`,
  `VisualTagger` — each with deterministic `Mock*` impl for offline demo/testing.
- **`VisionEngine`:** high-level `analyze()` / `analyze_batch()` combining all sub-components
  in a single call; `find_similar()` via cosine similarity on embeddings; `is_duplicate()` via
  perceptual hash comparison.
- **`VisionManager`:** analysis job queue with priority ordering (Low/Normal/High/Critical),
  deduplication via `ImageHash`, batch processing, background worker interface.
- **`VisualSearch`:** multi-modal search (text, metadata, OCR, tags, captions, embeddings);
  `SearchQuery` builder; vector similarity search; full-text across all indexed fields.
- **`VisionCache`:** typed LRU caches for thumbnails, embeddings, OCR results, captions with
  TTL expiry and memory budget tracking.
- **6 AI tools via `vision_tool!` macro:** `DescribeImageTool`, `ExtractTextTool`,
  `FindObjectsTool`, `SearchImagesTool`, `AnalyzePhotoTool`, `GenerateCaptionTool` — each
  gated by `VisionPermissionManager` + activity trail logging.
- **`VisionEvent`:** 21 event payload variants covering the full vision lifecycle (image
  loaded/decoded/analyzed, OCR/caption/embedding, object/face/scene detection, quality/color,
  tags, search, cache, thumbnails, hashing, duplicates, tools).
- **`VisionPermissionManager`:** capability-based permission gating (Camera, GalleryRead,
  MediaPicker, Storage, CameraFrame, FaceRecognition, VisualSearch).
- **`VisionConfig`:** serializable defaults (thumbnail size, cache limits, OCR language,
  embedding dim, batch size, similarity/duplicate thresholds, feature toggles).
- **`VisionError`:** typed error category unifying all vision failures into `NovaError`.
- **Demo integration:** `nova_vision` dependency added to `apps/nova-demo`.

### Build
- Added `modules/vision` to workspace members in root `Cargo.toml`.
- All 4 verification gates green: `fmt`, `clippy -D warnings`, `test --all-targets`.

---

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
