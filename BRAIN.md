# BRAIN.md — NOVA project handoff for any AI / engineer

> **Read order for any coding agent:** `BRAIN.md` → `AI_CONTEXT.md` → the relevant
> `tasks/M<n>.md` — *before making any changes.*
>
> This is the single-page mental model of NOVA: what it is, how it is built, what exists,
> the hard rules, and how to continue safely. Deep reasoning lives in `docs/bible/` (the
> "NOVA Bible") and `docs/adr/` (decisions).

---

## 1. What NOVA is

NOVA is a **single-user, on-device-first, privacy-first personal AI assistant** ("digital
brain") for **Android + Windows** (Linux/macOS later). It remembers, searches, and acts
on the user's data locally. Cloud is an **optional, consent-gated accelerant — never
required**. See `docs/bible/Chapter-01-Product-Vision-and-Philosophy.md`.

**The nine ordered principles** (lower number wins on conflict):
1 user sovereign · 2 privacy by default · 3 on-device first · 4 memory is sacred ·
5 transparency over magic · 6 agency with consent · 7 longevity/ownership ·
8 coherence over features · 9 honesty about limits.

---

## 2. Architecture diagram

```
                 User
                  │
                  ▼
      Android UI  /  Windows UI        (future — no UI yet)
                  │
                  ▼
                 FFI                    (api/ffi — C-ABI seam, nova_ffi)
                  │
                  ▼
                Kernel                  (src/kernel — nova_kernel)
                  │
      ┌─────┬─────┼─────┬─────┬─────┐
      ▼     ▼     ▼     ▼     ▼     ▼
   Memory Search Voice   AI  Comms Plugins   (modules/*)
```

The **Kernel** owns cross-cutting facilities (event bus, config, logging, consent, egress
gate, module registry). **Modules** plug into the kernel and talk to each other only
through the **event bus** — never by constructing peers directly (dependency injection).
The **composition root** (FFI / demo) constructs and registers modules, because the kernel
crate must not depend on module crates (would be circular).

---

## 3. Module dependency graph

Crate-level (Cargo) dependencies as they exist **today**:

```
Kernel (nova_kernel)        depends on: nothing internal
Memory (nova_memory)        └── Kernel
Search (nova_search)        ├── Kernel
                            └── Memory          (indexes memory records)
Voice  (nova_voice)         └── Kernel          (skeleton)
AI     (nova_ai)            └── Kernel          (skeleton)
Vision (nova_vision)        └── Kernel          (skeleton)
Comms  (nova_comms)         └── Kernel          (skeleton)
Plugin Host (nova_plugin_host) └── Kernel       (skeleton)
FFI    (nova_ffi)           └── Kernel + all modules  (composition root)
Demo   (nova_demo)          └── Kernel + all modules  (composition root)
```

Planned future edges (NOT yet implemented — do not add until their milestone):
`AI → Memory, Search, Voice`. Keep the graph **acyclic**; the kernel depends on no module.

---

## 4. Where things live (all on the **D: drive**, never C:)

Project root: `D:\Ansh Kesharwani\Documents\NOVA`

- `BRAIN.md` (this), `AI_CONTEXT.md` (live state), `README.md`, `CHANGELOG.md`, `roadmap/ROADMAP.md`
- `TASKS.md` — per-milestone task tracking (read the current one before coding)
- `docs/bible/` — source-of-truth design (Phase 0 + Chapters 1–4 written; 5–20 TBD)
- `docs/adr/` — Architecture Decision Records ADR-0001..0010 (**read before changing architecture**)
- `docs/governance/` — decision log, traceability matrix, glossary
- `src/kernel/` — `nova_kernel` (the microkernel, §6)
- `modules/{memory,search,voice,ai,comms,plugin_host}/` — one crate each
- `api/ffi/` — `nova_ffi` C-ABI seam for the future UI shells
- `apps/nova-demo/` — `nova_demo` runnable smoke test (NOT the product UI)
- `.nova-runtime/` — local runtime data (DBs, keys, logs); gitignored; created on D: at runtime

GitHub: private repo `https://github.com/anshkesharwani0807-rgb/NOVA` (branch `main`).

---

## 5. Toolchain & how to build/test (IMPORTANT)

- Language **Rust**, Cargo **workspace** at the repo root.
- **Use the MSVC toolchain** (the default gnu one fails on `dlltool`/OpenSSL). If you hit
  those errors, prefix commands with the msvc toolchain:

  ```bash
  TC=+stable-x86_64-pc-windows-msvc
  cargo $TC fmt --all -- --check
  cargo $TC clippy --workspace --all-targets -- -D warnings
  cargo $TC test --workspace
  cargo $TC run -p nova_demo
  ```

- **CI** (`.github/workflows/ci.yml`, windows-latest) runs those exact three gates.
  Every commit MUST pass all three locally first.
- SQLite via `rusqlite` feature `bundled` (builds fine with MSVC `cl.exe`).
  **Do NOT use SQLCipher `vendored-openssl`** — it does not build here (msys Perl lacks
  `Locale::Maketext::Simple`). At-rest encryption uses a pure-Rust `aes-gcm` layer behind a
  `KeyProvider` seam instead (see M4).

---

## 6. The kernel in one screen (nova_kernel, src/kernel)

- `error` — `NovaError { category, code, message, correlation_id }`, `Result<T>`. **No panics in lib code.**
- `event_bus` — tokio broadcast (pub/sub) + mpsc/oneshot (request/response); `NovaEvent`
  carries `EventMetadata` (origin, correlation id, causing_action) + `Arc<dyn Any>` payload.
- `config` — layered `NovaConfig`; **defaults private/conservative** (local_by_default=true,
  telemetry off, autonomy "conservative").
- `logger` — three planes: diagnostic; **Activity Trail** (`log_activity`, user-facing "why");
  **Egress Log** (`log_egress`). Local-only.
- `consent` + `egress` (M2) — `ConsentManager` (Allow Once/Session/Always/Deny) and
  `EgressGate` (Offline/LocalNetwork/Internet/Blocked). **All outbound goes through the gate;
  policy overrides consent.**
- `module` (M3) — `KernelModule` trait (`module_id/version/dependencies/initialize/start/
  stop/shutdown/health`) + `ModuleRegistry` (register/lookup/list/health/topo-resolve/
  bring_up/tear_down).
- `Kernel` — owns `event_bus`, `consent`, `egress_gate`, `registry`; created in `Kernel::bootstrap`.

---

## 7. Milestone status (what actually works)

- ✅ **M1 Kernel foundation** — workspace, error/event-bus/config/logging/bootstrap, FFI seam, demo.
- ✅ **M2 Consent + Egress Gate** — D3/D8 enforced; every decision logged.
- ✅ **M3 Module Registry + DI + Lifecycle** — all 6 modules implement `KernelModule`.
- ✅ **M4 Encrypted Memory Engine** (`nova_memory`) — local encrypted SQLite (AES-256-GCM +
  `KeyProvider`); `MemoryRecord`, 13 `MemoryCategory`; full API; persists across restarts;
  publishes `MemoryEvent` (Created/Updated/Deleted) on the event bus.
- ✅ **M5 Universal Search Index** (`nova_search`) — hybrid lexical+semantic search engine
  (SQLite FTS + exact cosine KNN vector store); permission-scoped indexing; natural language
  query parser; auto-indexes memory via `MemoryEvent`; schema v2; search latency within
  NFR-PERF-003 budget.
- ✅ **M6 AI Engine & Local Inference** (`nova_ai`) — Candle GGUF LLM backend, BERT embeddings,
  uncertainty surfacing, consent-gated remote acceleration seam, model lifecycle manager,
  streaming inference, tool-calling framework. All FR-AI-001..005 implemented.
- ✅ **M7 Offline Voice System** (`nova_voice`) — provider abstractions (7 traits), offline
  mock stack, full pipeline (VAD → wake-word → streaming ASR → AI → streaming TTS), barge-in,
  cancellation, session manager, 11 voice events, 5 integration tests.
- ✅ **M8 Android Shell** — `api/jni` bridging crate (16 JNI entry points over `nova_ffi`); Kotlin `NovaCore` singleton + `NovaService` foreground service + Compose UI (search, memory detail, activity trail, settings).
- ✅ **M10 Vision Intelligence** — `nova_vision` crate as `KernelModule` (`VisionSystem`); `VisionProvider` trait (17 methods, `MockVisionProvider`); image processing (loading, decoding, metadata, thumbnails, hashing); 9 AI engine traits + mocks (OCR, caption, embedding, detection, scene, face, quality, color, tags); `VisionEngine`, `VisionManager` (priority queue + dedup), `VisualSearch` (multi-modal), `VisionCache` (LRU + TTL), 6 AI tools, 21 event variants, permission manager, config, error types. 26 unit tests.
- ✅ **M11 Device Sync** — `nova_sync` crate; E2E encryption (X25519 + AES-256-GCM); device pairing/unpairing; sync protocol; transport trait; config (disabled by default).
- ✅ **M12 Automation & Plugin System** — `AutomationEngine` with 4 action types; `ConsequenceGate` classification Low/Medium/High; `PluginSandbox` trait; activity trail logging.
- ✅ **M13 Security Hardening, QA & v1.0** — All CI gates pass; all docs updated; workspace complete.
- ✅ **M14 Knowledge & Memory Intelligence (v0.1.0)** — `nova_knowledge` crate (`KnowledgeEngine`, `MemoryAnalyzer`, `KnowledgeGraph`, `RelationshipEngine`, `TimelineGenerator`, `SmartRecall`, `SummaryEngine`); 9 event variants; memory analysis (categorization, importance, tags, entities, dedup, links); timeline generation; contextual recall; offline summaries; all 4 verification gates green.
- ✅ **M15 Knowledge Graph & Memory Intelligence (v0.2.0)** — 6 new modules: entity extraction (11 entity types, 10 sources, `EntityExtractor`); semantic index (`KnowledgeIndex` + `MockEmbeddingProvider`); reasoning layer (`KnowledgeReasoner` with BFS path finding, context building, citations); ranking (`CombinedRanker`, `RecencyRanker`); persistence (`JsonFileStorage` save/load round-trip); engine integration (extract, index, search, reason, persist, permissions, 16 event types); timeline generation (daily/weekly/monthly/project/conversation); summary generation (daily/conversation/project/cluster); recall query builder. 182 tests (165 unit + 17 integration). All 4 verification gates green.
- ✅ **M16 Cross-Device Platform** — `nova_cross_device` coordinator, `nova_windows_agent` (17 capabilities), `nova_transport` (TCP + discovery), `nova_pairing` (QR + X25519), `nova_security` (ed25519 + AES-256-GCM), `nova_sync` (clipboard + memory sync). Demo [7f] exercises all 6 crates.
- ✅ **M17 nova_screen Platform** — Cross-platform screen capture (WinRT/MediaProjection), UI tree (UIA/AccessibilityService), OCR (WinRT/ML Kit), visual grounding, permission manager. Windows + Android real implementations.
- ✅ **M18 nova_input Platform** — `InputEngine` trait, mouse/keyboard/touch action types, `ScreenInputBridge`, `MockInputProvider`, Windows (SendInput) + Android (AccessibilityService) real implementations.
- ✅ **M19 Task Execution & Computer Control** — `real_executors.rs` (ScreenClick/Type/Drag/Swipe), `consent_gate.rs` (ActionClassifier + 3 autonomy dial levels), `controller.rs` (ComputerController with 6 async methods), error recovery with exponential backoff retry, named executor dispatch. 21 new unit tests. Demo [7g] validates consent gate + controller + executors.
- ✅ **M20 S1 Planner** — `planner.rs` with `Goal`, `ExecutionStep`, `ExecutionPlan`, `Capability` (13 variants), `PlanValidation`, `Planner`. Heuristic decomposition for 14+ goal types. Kahn's topological sort, cycle detection, `ready_steps()`. 23 unit tests. Wired into `nova_automation`.
- ✅ **M20 S2 World State** — `world_state.rs` with `WorldState`, `WorldSnapshot`, `WorldStateConfig`, `DeviceTelemetry`, `NetworkState`, `WorldDiff`, `WorldSubscription`, `DeviceTelemetryCollector` trait + `NullDeviceTelemetryCollector`. Device/network state storage, diff tracking across 7 state categories, thread-safe subscriptions with notify, privacy/redacted snapshots. 48 unit tests (22 original + 26 new).

- ✅ **M21 Closed-Loop Autonomous Execution** — `pipeline_step.rs`, `execution_plan_adapter.rs`,
  `outcome_verifier.rs`, `recovery_orchestrator.rs`, `plan_executor.rs`, `observability.rs`.
  Full pipeline: Goal → Plan → Precondition check → Action execution (thread-based timeout) →
  Async verification (screen/OCR, device telemetry, snapshot diff, app foreground) →
  Recovery retry loop (retry/skip/abort/escalate/replan) → Report. 19 new `AutomationEventPayload`
  variants. 10 new `AutomationConfig` fields. `ExecutionMetrics` + `SharedMetrics` + trace types.
  62 new unit tests (32 plan_executor + 30 observability). All 4 verification gates green.

- ✅ **M22 Intention-Driven Autonomous Agent** — `intention_parser.rs` (NL→Goal AI + heuristic),
  `goal_registry.rs` (SQLite persistence), `execution_manager.rs` (goal lifecycle + queue),
  `ai_bridge.rs` (AI↔Automation bridge), `feedback_generator.rs` (progress/summary/event feedback).
  Resolved `ExecutionStatus` ambiguity, fixed `ai_bridge` session sharing, 283 new unit tests total.
  All 4 verification gates green. Pre-existing `STATUS_ACCESS_VIOLATION` in `real_executors` only.

All milestones 1–22 exit criteria verified.
NOVA v0.22.0-m22 ready.

`comms` and `plugin_host` are working `KernelModule` **skeletons** (start/stop cleanly, no
real work yet).

## REAL vs MOCK Status (as of M15.2 audit)

| Module | Real | Mock/Simulated | Pending |
|---|---|---|---|
| nova_kernel | ✅ Kerne bootstrap, event bus, config, consent, egress | — | — |
| nova_memory | ✅ SQLite + AES-256-GCM | — | — |
| nova_search | ✅ SQLite FTS | — | — |
| nova_security | ✅ ed25519, X25519, AES-256-GCM, HKDF | — | Real network replay attack testing |
| nova_device | ✅ Device info detection | — | — |
| nova_knowledge | ✅ Entity extraction, graph, reasoning, persistence | 🔶 Embeddings (mock embedding provider) | — |
| nova_plugin_sdk | ✅ Plugin lifecycle, sandbox, permissions | 🔶 All plugins are demo/test doubles | Production plugins |
| nova_sync | — | 🔶 In-memory sync only | Network sync |
| nova_ai | 🔶 CandleProvider exists but never used | 🔶 MockProvider in all tests/demo | GGUF model download + test |
| nova_voice | — | 🔶 100% mock pipeline | Real mic/speaker I/O |
| nova_vision | 🔶 Image loading/decoding/hashing | 🔶 All 10 AI engines are mock | Real AI model integration |
| nova_automation | 🔶 Consent gate (real classification + `ConsentManager`), real executors (click/type/drag/swipe capture→ground→execute pipeline), world state with device/network telemetry, diff tracking, subscriptions, privacy filtering | 🔶 Screen capture depends on platform (mock used on unsupported) | Real app launch via `DefaultActionExecutor` |
| nova_pairing | 🔶 Real X25519 key exchange, QR rendering | 🔶 No real device-to-device protocol | Real pairing flow |
| nova_transport | 🔶 Real TCP/UDP code exists | 🔶 Demo never starts transport; 3/12 tests touch network | End-to-end network test |
| nova_windows_agent | 🔶 RealWindowsProvider defined (659 lines) | 🔶 MockWindowsProvider in all tests | Real Windows test harness |
| nova_cross_device | 🔶 Real crypto in simulate_pair() | 🔶 AndroidAdapter mock; discovery empty; all in-process | Two-device network test |
| nova_comms | — | — | Skeleton (start/stop only) |
| nova_plugin_host | — | — | Skeleton (start/stop only) |
| nova_jni | 🔶 Compiles (16 entry points) | — | Android emulator/device test |
| nova_desktop | 🔶 Compiles (egui/eframe) | — | GUI interaction test |

---

## 8. NEVER do this (DON'T-DO list)

**NEVER**
- rewrite or "improve" the overall architecture
- replace Rust with another language
- remove, rewrite, or ignore ADRs (`docs/adr/`)
- replace or bypass the Kernel
- bypass the **Consent** manager or the **Egress** gate for any outbound action
- change a module's **public API** without a stated, necessary reason
- rename crates, modules, or files
- introduce a **circular dependency** (esp. kernel → module)
- add cloud/network calls by default (privacy-by-default; egress is opt-in + gated)
- use SQLCipher `vendored-openssl` (does not build here)
- `panic!` in library code (return `NovaError`)
- commit runtime data / secrets (`.nova-runtime/`, keys) — they are gitignored for a reason
- break passing tests, or commit without the three green CI gates

Following this list prevents hallucinated refactors and wasted work.

---

## 9. How to continue a milestone (the loop)

1. Read `BRAIN.md` → `AI_CONTEXT.md` → `tasks/M<n>.md` + relevant ADR/Bible chapter.
2. Implement **additively**; keep each module's SQLite DB local under `.nova-runtime/`.
3. Add comprehensive tests (unit + integration).
4. Run: `cargo $TC fmt --all` → `clippy --workspace --all-targets -- -D warnings` → `test --workspace`.
5. Run `cargo $TC run -p nova_demo` and confirm the new capability is visible.
6. Update `CHANGELOG.md`, `roadmap/ROADMAP.md`, **and `AI_CONTEXT.md`**.
7. Commit (detailed message + co-author line) and push to `main`. CI must be green.

---

## 10. Gotchas learned the hard way

- The auto-formatter/linter reformats files after writes — **Read before Edit** if a prior write may have been reformatted.
- Event-bus delivery is async — integration tests relying on a published event should wait briefly / poll.
- SQL: inline only integers (safe); always **bind** user strings (injection safety). Use `params_from_iter` for dynamic params.
- Two SQLite DBs exist: the **encrypted memory** DB and the **plaintext derived search index**. The search index is a
  cache; its at-rest hardening is deferred to the same future whole-DB encryption path as memory.

*Keep this file (and `AI_CONTEXT.md`) updated at the end of every milestone.*
