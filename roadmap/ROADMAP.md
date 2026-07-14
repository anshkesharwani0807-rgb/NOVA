# NOVA Development Roadmap

> This roadmap translates the Bible goals (Chapter 2) into a phased, milestone-based
> engineering plan. Each milestone has a clear objective, exit criteria, and dependency
> on prior milestones. Milestones are sequential â€” a later milestone never begins
> without the exit criteria of the previous one being verified.

---

## Milestone 0 â€” Foundation (COMPLETE âœ“)

**Objective:** Establish the canonical specification (NOVA Bible), architecture
decisions (ADRs), and repository skeleton.

**Deliverables:**
- NOVA Bible Chapters 0, 1, 2 (complete)
- ADRs 0001â€“0010 (proposed)
- Repository skeleton with all top-level directories
- Git configuration, CI/CD placeholders

**Exit Criteria:** âœ“ Bible chapters reviewed and versioned. âœ“ Repository is clean and
committed.

---

## Milestone 1 â€” Kernel Foundation (COMPLETE âœ“)

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

## Milestone 2 â€” Consent + Egress Gate (COMPLETE âœ“)

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

## Milestone 3 â€” Module Registry + DI + Lifecycle (COMPLETE âœ“)

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

## Milestone 4 â€” Encrypted Memory Engine (COMPLETE âœ“)

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
- Export â†’ wipe â†  import round-trip verified (NFR-REL-005)
- Encryption at rest verified

---

## Milestone 5 — Universal Search Engine (COMPLETE ✅)

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

## Milestone 6 â€” AI Engine & Local Inference (COMPLETE)

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

## Milestone 7 â€” Voice System (COMPLETE)

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

## Milestone 8 â€” Android Shell (COMPLETE âœ“)

**Objective:** Build the Android (Kotlin/Jetpack Compose) shell that binds to the
Rust core via JNI over the C-ABI.

**Deliverables:**
- âœ“ `api/jni/` crate: 16 JNI entry points wrapping `nova_ffi` C-ABI
- âœ“ Kotlin `NovaCore` singleton with matching `external fun` declarations
- âœ“ `NovaService` foreground service (auto-started via `NovaApplication`)
- âœ“ Compose UI screens: Search, MemoryDetail, Chat, Visual, ActivityTrail, Settings
- âœ“ Navigation graph with 5 routes
- âœ“ `build_android.ps1` cross-compilation script

**Exit Criteria:**
- Rust workspace compiles with all 4 verification gates green
- JNI function names match `Java_com_example_nova_NovaCore_<method>` convention
- AndroidManifest includes foreground service + required permissions

---

## Milestone 9 â€” Windows Shell (NEXT)

**Objective:** Build the Windows desktop shell binding to the Rust core.

**Deliverables:**
- Windows native shell (WinUI / Win32 or Rust-based desktop UI TBD in Ch17 ADR)
- C-ABI bridge to `nova_ffi`
- Core NOVA UI flows: search, memory, settings, voice
- System tray integration for always-available access

**Exit Criteria:**
- App installs and runs on Windows 10+
- All M1-M5 features accessible through the Windows UI

---

## Milestone 10 â€” Device Sync & Communication (NEXT)

**Objective:** Implement opt-in, end-to-end encrypted cross-device sync between Android
and Windows for the same user.

**Deliverables:**
- Sync protocol implementation (FR-DEV-001, FR-DEV-002)
- End-to-end encryption (no plaintext on transit)
- User UI for device pairing/unpairing
- Sync window SLA verification

**Exit Criteria:**
- Memory created on Device A appears on Device B within SLA when sync enabled
- With sync disabled, zero data flows (confirmed by egress log)

---

## Milestone 11 â€” Automation & Plugin System (NEXT)

**Objective:** Implement the Consent Gate, basic automation actions, and the Plugin
Host sandbox.

**Deliverables:**
- Consequence/Consent Gate with classification ruleset (FR-AUTO-001/002/003)
- Core automation actions: file management, reminders, app launch
- Plugin Host sandbox (ADR-0012 â€” to be written)
- Plugin permission system (ADR-0008 integration)

**Exit Criteria:**
- All FR-AUTO-* requirements pass
- Irreversible actions always require confirmation (even in autonomous mode)
- Plugin sandbox prevents unauthorised resource access

---

## Milestone 12 â€” Security Hardening, QA & v1.0 Release (NEXT)

**Objective:** Complete security audit, performance profiling, QA, and ship v1.0.

**Deliverables:**
- Security audit (Ch15 requirements implemented)
- Performance profiling on minimum hardware (all NFR-PERF-* targets met)
- Full test suite: unit + integration + E2E + security
- Documentation: user guide, developer guide
- v1.0 release build for Android and Windows

**Exit Criteria:**
- All Milestone 1â€“11 exit criteria verified
- All `MUST` NFRs pass automated tests
- Zero known critical security issues

---

## Future Phases (Post-v1.0)

- **v2.x:** Visual Intelligence (face recognition, OCR, scene analysis)
- **v3.x:** Proactive helpfulness (anticipation engine, LG-2)
- **v4.x:** Linux + macOS shells (LG-3)
- **v5.x:** Advanced automation workflows (XG-2)
- **Long-term:** Near-frontier local reasoning (XG-1), portable AI self (XG-3)

---

*Roadmap version: 0.2 (post-sync). Updates require milestone completion review.*
