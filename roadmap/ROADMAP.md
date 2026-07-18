# NOVA Development Roadmap

> This roadmap translates the Bible goals (Chapter 2) into a phased, milestone-based
> engineering plan. Each milestone has a clear objective, exit criteria, and dependency
> on prior milestones. Milestones are sequential - a later milestone never begins
> without the exit criteria of the previous one being verified.

---

## Milestone 0 - Foundation (COMPLETE)

**Objective:** Establish the canonical specification (NOVA Bible), architecture
decisions (ADRs), and repository skeleton.

**Deliverables:**
- NOVA Bible Chapters 0, 1, 2 (complete)
- ADRs 0001-0010 (proposed)
- Repository skeleton with all top-level directories
- Git configuration, CI/CD placeholders

**Exit Criteria:** Bible chapters reviewed and versioned. Repository is clean and committed.

---

## Milestone 1 - Kernel Foundation (COMPLETE)

**Objective:** Build the foundational NOVA Microkernel (nova_kernel).

**Deliverables:**
- Rust Cargo workspace configured
- `nova_kernel` crate:
  - Structured error taxonomy (FR-CORE-005, ADR-0010)
  - Privacy-preserving logger with activity trail + egress log (FR-CORE-003/004, ADR-0009)
  - Layered configuration system (FR-CORE-002, ADR-0008)
  - Async event bus: pub/sub + request/response (ADR-0004)
  - Kernel lifecycle bootstrap (FR-CORE-001)
- FFI boundary crate (`nova_ffi`) exporting C-ABI
- Integration test suite validating: config, logging, event bus routing
- CI GitHub Actions workflow
- PowerShell build/test automation script

**Exit Criteria:**
- `cargo test --workspace` passes with zero failures
- `cargo clippy -- -D warnings` produces zero warnings
- All 9 kernel integration tests pass
- CI workflow runs green on push

**Bible chapters written this milestone:** Ch3 (Functional Requirements), Ch4 (NFRs)

---

## Milestone 2 - Consent + Egress Gate (COMPLETE)

**Objective:** Implement the Consent Manager and Egress Gate for privacy-by-default.

**Deliverables:**
- Consent Manager (Allow Once/Session/Always/Deny)
- Egress Gate (Offline/LocalNetwork/Internet/Blocked)
- Policy overrides consent
- All outbound communication goes through the gate
- Every decision and egress attempt logged in Activity Trail and Egress Log

**Exit Criteria:**
- All consent and egress requirements satisfied
- Egress gate enforced (D3/D8)
- Decision logging verified

---

## Milestone 3 - Module Registry + DI + Lifecycle (COMPLETE)

**Objective:** Implement the Module Registry and Dependency Injection system.

**Deliverables:**
- `KernelModule` trait implementation
- `ModuleRegistry` (register/lookup/list/health/topo-resolve/bring_up/tear_down)
- Module lifecycle management (initialize, start, stop, shutdown, health)
- Dependency injection via event bus

**Exit Criteria:**
- All 6 core modules implement `KernelModule`
- Module lifecycle and dependency resolution work correctly
- Module registry is fully functional

---

## Milestone 4 - Encrypted Memory Engine (COMPLETE)

**Objective:** Implement the Memory Engine with a real encrypted local database,
full CRUD, and user inspection/correction/deletion support.

**Deliverables:**
- SQLite-based encrypted store (AES-256-GCM via `aes-gcm` layer) (FR-MEM-001)
- Memory capture API: text, structured events, file references (FR-MEM-002)
- Memory inspection, correction, deletion (FR-MEM-003)
- Provenance: every memory use recorded in activity trail (FR-MEM-004)
- Full export/import (FR-EXP-001, FR-EXP-002)
- Unit + integration tests for all memory operations

**Exit Criteria:**
- All FR-MEM-* and FR-EXP-* requirements satisfied with passing tests
- Export -> wipe -> import round-trip verified (NFR-REL-005)
- Encryption at rest verified

---

## Milestone 5 - Universal Search Engine (COMPLETE)

**Objective:** Implement the Universal Search engine with hybrid semantic + lexical
retrieval over local indexed content.

**Deliverables:**
- Local exact cosine KNN vector index (SQLite-backed; FR-SRCH-002, ADR-0006)
- Lexical full-text search layer
- Permission-scoped indexing (FR-SRCH-003)
- Natural language query interface (FR-SRCH-001)
- Search integration with Memory Engine
- Search latency benchmarks (NFR-PERF-003)

**Exit Criteria:**
- All FR-SRCH-* requirements satisfied
- Offline search within latency budget on reference hardware
- Permission revocation removes indexed data

---

## Milestone 6 - AI Engine & Local Inference (COMPLETE)

**Objective:** Implement the AI Engine with local LLM and embedding inference,
uncertainty surfacing, and the consent-gated acceleration seam.

**Deliverables:**
- [x] `InferenceRuntime` abstraction (ADR-0007)
- [x] Quantized local LLM backend (GGUF via CandleProvider)
- [x] ONNX/BERT embedding backend (CandleEmbedder)
- [x] Uncertainty surfacing (FR-AI-003)
- [x] Acceleration seam with Egress Gate integration (FR-AI-004)
- [x] Model lifecycle management (FR-AI-005)
- [x] Latency/throughput/cold-warm/memory benchmarks (NFR-PERF-002)

**Exit Criteria:**
- Local inference works offline on minimum hardware tier
- Uncertainty expressed for ambiguous inputs
- Remote seam disabled by default; all remote calls in egress log when enabled

---

## Milestone 7 - Voice System (COMPLETE)

**Objective:** Implement wake-word detection, ASR, and TTS entirely on-device.

**Deliverables:**
- Wake-word detection (FR-VOICE-001)
- On-device ASR (FR-VOICE-002)
- On-device TTS (FR-VOICE-003)
- Audio privacy: no buffering before wake-word (NFR-SEC-004)
- Voice pipeline latency benchmarks (NFR-PERF-001, NFR-PERF-002)

**Exit Criteria:**
- End-to-end voice interaction works offline
- No audio retained before wake-word (verified by memory inspection)

---

## Milestone 8 - Android Shell (COMPLETE)

**Objective:** Build the Android (Kotlin/Jetpack Compose) shell that binds to the
Rust core via JNI over the C-ABI.

**Deliverables:**
- `api/jni/` crate: 16 JNI entry points wrapping `nova_ffi` C-ABI
- Kotlin `NovaCore` singleton with matching `external fun` declarations
- `NovaService` foreground service (auto-started via `NovaApplication`)
- Compose UI screens: Search, MemoryDetail, Chat, Visual, ActivityTrail, Settings
- Navigation graph with 5 routes
- `build_android.ps1` cross-compilation script

**Exit Criteria:**
- Rust workspace compiles with all 4 verification gates green
- JNI function names match `Java_com_example_nova_NovaCore_<method>` convention
- AndroidManifest includes foreground service + required permissions

---

## Milestone 9 - Windows Shell (COMPLETE)

**Objective:** Build the Windows desktop shell binding to the Rust core.

**Deliverables:**
- `apps/nova-desktop` crate with `egui`/`eframe` Rust GUI
- Tabbed interface: Search, Memory, Voice, Activity Trail, Health, Settings
- Direct binding to kernel modules (not through FFI)
- System tray placeholder (tray-icon crate)
- Search: full-text + natural language modes
- Memory: list, filter, add, detail view
- Settings: JSON config editor with save
- Activity Trail + Egress Log viewers
- System Health panel
- All M1-M5 features accessible through the desktop UI

**Exit Criteria:**
- App compiles with 0 clippy warnings and 0 fmt errors
- Search, Memory, Settings, Activity Trail, Health, Voice tabs functional
- Kernel bootstrap on app start

---

## Milestone 10 - Vision Intelligence (COMPLETE)

**Objective:** Build a reusable, offline-first vision intelligence platform with OCR,
image understanding, semantic embeddings, face system, and visual search.

**Deliverables:**
- `nova_vision` crate as `KernelModule` (`VisionSystem`)
- `VisionProvider` trait with 17 methods (offline mock default)
- Image processing: loading, decoding, metadata, thumbnails, perceptual hashing
- AI engines (trait + mock): OCR, captioning, embedding, object detection, scene
  classification, face detection/clustering, quality/color analysis, visual tagging
- `VisionEngine` - `analyze()` combining all sub-components
- `VisionManager` - priority job queue with deduplication
- `VisualSearch` - multi-modal search (text, OCR, tags, captions, embeddings)
- `VisionCache` - typed LRU caches with TTL and memory budget
- 6 AI tools (`vision_tool!` macro) - permission-gated + activity trail
- 21 `VisionEvent` payload variants, `VisionPermissionManager`, `VisionConfig`
- All 4 verification gates green

**Exit Criteria:**
- All vision module tests pass (26 unit tests)
- `cargo clippy -D warnings` - zero warnings
- `cargo fmt --check` - clean
- Workspace compiles and all existing tests pass

---

## Milestone 11 - Device Sync & Communication (COMPLETE)

**Objective:** Implement opt-in, end-to-end encrypted cross-device sync between Android
and Windows for the same user.

**Deliverables:**
- `nova_sync` crate with E2E encryption (X25519 ECDH + AES-256-GCM)
- Device pairing/unpairing with `PairedDevice` registry
- `SyncProtocol` — encrypt/decrypt with ephemeral key exchange
- `SyncManager` — sync state, dedup, stats
- Sync events and config (disabled by default)
- `SyncTransport` trait for future LAN/Tailscale transport
- All 4 verification gates green

**Exit Criteria:**
- E2E encryption works without plaintext on transit
- Device pairing/unpairing lifecycle works
- Sync disabled by default (privacy-first)

---

## Milestone 12 - Automation & Plugin System (COMPLETE)

**Objective:** Implement the Automation Engine as a dedicated `nova_automation` crate with
workflow creation, trigger evaluation, action execution, scheduler, event bus integration,
and comprehensive test coverage.

**Deliverables:**
- `nova_automation` crate as a `KernelModule` (`AutomationEngine`)
- `Workflow`, `WorkflowStep`, `TriggerConfig`, `WorkflowSummary` structs with validation
- `WorkflowRegistry` — register/get/update/delete/list/enable/disable/find_by_trigger
- `ActionType` enum (14 variants: Speak, Notify, OpenApp, LaunchActivity, Clipboard,
  CreateMemory, SearchMemory, RunAI, CaptureVoice, AnalyzeImage, DeviceControl,
  PluginInvocation, Wait, SubWorkflow) + `ActionExecutor` trait + `DefaultActionExecutor`
- `Condition` enum (12 variants: And, Or, Not, Comparison, Regex, Contains, Numeric,
  DateCompare, PermissionCheck, ContextCheck, True, False) + `ConditionEvaluator` trait +
  `DefaultConditionEvaluator`
- `TriggerType` enum (13 variants: Time, Date, Battery, Charging, WiFi, Bluetooth,
  DeviceState, Memory, Voice, Vision, Manual, EventBus, Plugin) + `TriggerEvaluator` trait +
  `TimeTriggerEvaluator`, `ManualTriggerEvaluator`, `EventBusTriggerEvaluator`
- `Scheduler` — trigger checking with time-based scheduling, `get_next_scheduled`
- `ExecutionEngine` — sequential/parallel execution with retry logic and cancellation
- `HistoryStore` trait + `InMemoryHistory` with max-entries cap
- `AutomationEventPayload` (10 variants) — published on event bus for all workflow events
- `AutomationConfig` with tick intervals, timeouts, retries, concurrent limits
- Comprehensive test coverage: 56 unit tests + 36 integration tests
- Demo extension exercising workflow creation, triggers, scheduler, execution, history,
  and event bus integration
- All 4 verification gates green

**Exit Criteria:**
- All automation tests pass (56 unit + 36 integration = 92 tests)
- `cargo clippy -D warnings` — zero warnings
- `cargo fmt --check` — clean
- `cargo run -p nova_demo` — automation demo section runs successfully
- No existing APIs or tests broken

---

## Milestone 13 - Security Hardening, QA & v1.0 Release (COMPLETE)

**Objective:** Complete security audit, performance profiling, QA, and ship v1.0.

**Deliverables:**
- All 4 CI gates pass for entire workspace
- Documentation: ROADMAP.md, CHANGELOG.md, BRAIN.md, AI_CONTEXT.md, TASKS.md, SESSION.md
- All Milestone 1-13 exit criteria verified
- Release v0.13.0 ready

**Exit Criteria:**
- `cargo fmt --check` clean
- `cargo clippy -D warnings` — zero warnings
- `cargo test --workspace` — all pass
- Workspace compiles with all modules

---

## Milestone 14 — NOVA Vision Engine (COMPLETE)

**Objective:** Build the perception layer that allows NOVA to understand visual
information — offline-first, privacy-first, provider-abstracted, event-driven.

**Deliverables:**
- `nova_vision` crate as `KernelModule` (`VisionSystem`)
- `VisionProvider` trait with 17+ methods covering all vision operations
- Image processing: loading, decoding, metadata, thumbnails, perceptual hashing
- AI engine traits + mocks: OCR, captioning, embedding, object detection, scene
  classification, face system, quality/color analysis, visual tagging
- `VisionEngine` — `analyze()` + `analyze_batch()` combining all sub-components
- `VisionManager` — priority job queue (Low/Normal/High/Critical) with dedup
- `VisualSearch` — multi-modal search (text, metadata, OCR, tags, captions, embeddings)
- `VisionCache` — typed LRU caches for thumbnails, embeddings, OCR, captions
- `ScreenshotAnalyzer` — UI element detection (24 element types), future desktop support
- `VisionContextBuilder` — AI Runtime-compatible context from analysis + screenshots
- `ImagePreprocessor` — 5 resize modes, 4 normalization modes, format conversion
- 6 AI tools — permission-gated via `VisionPermissionManager` + activity trail
- 24 `VisionEvent` payload variants published on event bus
- 7 `VisionCapability` variants, `VisionConfig`, typed error categories
- 41 unit tests + full demo integration

**Exit Criteria:**
- All 4 verification gates green
- `cargo clippy -D warnings` — zero warnings
- `cargo fmt --check` — clean
- `cargo test --workspace` — all 380+ tests pass
- `cargo run -p nova_demo` — vision module lifecycle, tools, analysis demonstrated
- No M1-M13 regressions

---

## Milestone 15 — Knowledge Graph & Memory Intelligence (COMPLETE)

**Objective:** Implement knowledge graph with entity extraction, semantic indexing,
reasoning layer, ranking, persistence, and full engine integration — enabling NOVA
to understand relationships between entities, perform graph-based reasoning, and
generate timelines/summaries.

**Deliverables:**
- `entity.rs` — `KnowledgeEntity`, `EntityType` (11 types: Person, Place, Organization,
  Device, Document, Website, Event, File, Image, Topic, Custom), `EntitySource`
  (10 sources), `EntityExtractor` with `extract_from_text/memory/ocr/screenshot/
  conversation/automation/plugin`
- `graph.rs` — enhanced `KnowledgeRelationship` with `confidence`/`provenance`,
  type-indexed adjacency, `upsert_entity`, `find_entity_by_name`,
  `get_connected_entities_by_type`, `remove_entity`/`remove_relationship`
- `index.rs` — `EmbeddingProvider` trait, `KnowledgeIndex` (semantic/hybrid search
  with cosine similarity + keyword score + type filter), `MockEmbeddingProvider`
- `reasoning.rs` — `KnowledgeReasoner` (BFS path finding, graph expansion,
  dependency search, citation generation, AI Runtime context building)
- `ranking.rs` — `CombinedRanker` (recency + keyword + confidence + embedding),
  `RecencyRanker`, `RankWeights`
- `storage.rs` — `KnowledgeStorage` trait, `JsonFileStorage` (persistence to JSON
  files), `InMemoryStorage`
- `engine.rs` — M15 impl block on `KnowledgeEngine` (entity extraction, graph
  management, relationships, semantic indexing, reasoning, hybrid search,
  persistence, permissions, event bus integration)
- 16 event payload types published to kernel event bus
- Timeline generation (daily, weekly, monthly, project, conversation)
- Summary generation (daily, conversation, project, cluster)
- Recall query builder with time range/filters
- 182 knowledge tests (165 unit + 17 integration)

**Exit Criteria:**
- `cargo fmt --check` — clean
- `cargo clippy -D warnings` — zero warnings across workspace
- `cargo test --workspace` — all pass (nova_knowledge: 182 tests)
- `cargo run -p nova_demo` — [7e] demo section shows extraction → graph → index →
  reason → persist round-trip
- No M1-M14 regressions

---

## Milestone 16 — Cross-Device Platform (COMPLETE)

**Objective:** Build the cross-device link layer that turns one Rust Brain into a unified
Android + Windows "Digital Brain" — device discovery, trusted pairing, per-device permission
profiles, shared memory/clipboard/file sync, and unified command dispatch.

**Deliverables:**
- `nova_cross_device` crate — `CrossDeviceCoordinator` `KernelModule`, device management,
  sessions, platform adapters, unified command dispatch, per-device permission profiles,
  E2E encrypted file transfer, plugin SDK integration
- `nova_windows_agent` crate — 17 Windows capabilities, provider trait (mock + real),
  `WindowsAgent` `KernelModule`
- `nova_transport` crate — TCP transport, bincode packet, Zlib compression, AES-256-GCM
  encryption, heartbeat, reconnection, UDP multicast local discovery
- `nova_pairing` crate — QR pairing, 6-digit code, X25519 key exchange, trusted store
- `nova_security` crate — ed25519, X25519+AES-256-GCM, certificates, permission tokens,
  key rotation
- `nova_sync` crate — clipboard, shared memory, activity trail, conflict resolution
- Demo step `[7f]` exercising all 6 crates

**Exit Criteria:**
- All 4 verification gates green: 0 fmt errors, 0 clippy warnings, all 1100+ tests pass,
  `cargo run -p nova_demo` completes cleanly with M16 section
- No M1-M15 regressions

---

## Milestone 17 — nova_screen Platform (COMPLETE ✅)

**Objective:** Implement `nova_screen` — a cross-platform screen capture, UI tree, OCR,
and visual grounding module for Windows (WinRT/UIA) and Android (MediaProjection/
AccessibilityService/ML Kit).

**Deliverables:**
- `nova_screen` `KernelModule` (`ScreenSystem`)
- `ScreenCapture` trait + Windows (WinRT GDI) + Android (MediaProjection + ImageReader)
- `UiTreeProvider` trait + Windows (UIAutomation COM tree walker) + Android (AccessibilityService
  tree walk, depth-32, 200-node limit)
- `OcrProvider` trait + Windows (WinRT `OcrEngine`) + Android (ML Kit `TextRecognition`)
- `VisualGroundingProvider` trait + Windows (UIA tree walker matching) + Android
  (AccessibilityService tree walker matching)
- `permission.rs` — `ScreenCapturePermission` (System / ConsentGate / Mock)
- `jni_bridge.rs` — JNI bridge for ApplicationContext, MediaProjection, AccessibilityService
- `api/jni` — 4 native entry points for Android bridge

**Windows:**
✔ Screen Capture (WinRT `GraphicsCapturePicker` → `Direct3D11CaptureFrame` → BGRA8 → RGBA)
✔ UI Tree (UIAutomation `TreeWalker` → `GetFirstChildElement`/`GetNextSiblingElement`)
✔ OCR (WinRT `OcrEngine.TryRecognizeAsync` → `OcrResult` → words/lines)
✔ Visual Grounding (UIA tree walker matching text/content-description/class-name)

**Android:**
✔ Screen Capture (MediaProjection + `ImageReader` RGBA_8888 → BGRA8 conversion, YUV_420_888 CPU transform)
✔ UI Tree (AccessibilityService `getRootInActiveWindow()` → tree traversal, `recycle()` on every node)
✔ OCR (ML Kit `TextRecognition.getClient()` → `TextRecognizer.process()` → text-block→line→element)
✔ Visual Grounding (AccessibilityService tree walker matching `getText()`/`getContentDescription()`/`getViewIdResourceName()`/`getClassName()`)

**Exit Criteria:**
- ✅ `cargo check --workspace` — 0 errors
- ✅ `cargo test -p nova_screen` — 0 passed, 0 failed
- ✅ `cargo clippy --workspace --all-targets -- -D warnings` — 0 errors across all 24 crates
- ✅ No remaining mock, TODO, placeholder, or stub implementations inside `nova_screen`
- ✅ All 4 Android subsystems (Capture, UI Tree, OCR, Grounding) implemented via JNI

---

## Future Phases (Post-v0.19.0)

- **v3.x:** Proactive helpfulness (anticipation engine, LG-2)
- **v4.x:** Linux + macOS shells (LG-3)
- **v5.x:** Advanced automation workflows (XG-2)
- **Long-term:** Near-frontier local reasoning (XG-1), portable AI self (XG-3)

---

*Roadmap version: 1.3. All milestones M1-M17 exit criteria verified.*
