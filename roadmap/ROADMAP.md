# NOVA Development Roadmap

> This roadmap translates the Bible goals (Chapter 2) into a phased, milestone-based
> engineering plan. Each milestone has a clear objective, exit criteria, and dependency
> on prior milestones. Milestones are sequential — a later milestone never begins
> without the exit criteria of the previous one being verified.

---

## Milestone 0 — Foundation (COMPLETE ✓)

**Objective:** Establish the canonical specification (NOVA Bible), architecture
decisions (ADRs), and repository skeleton.

**Deliverables:**
- NOVA Bible Chapters 0, 1, 2 (complete)
- ADRs 0001–0010 (proposed)
- Repository skeleton with all top-level directories
- Git configuration, CI/CD placeholders

**Exit Criteria:** ✓ Bible chapters reviewed and versioned. ✓ Repository is clean and
committed.

---

## Milestone 1 — Core Substrate (IN PROGRESS 🔄)

**Objective:** Build the NOVA Microkernel — the foundational layer all modules depend
on. This milestone produces the first compilable, testable code in the project.

**Deliverables:**
- Rust Cargo workspace configured
- `nova_kernel` crate:
  - Structured error taxonomy (FR-CORE-005, ADR-0010)
  - Privacy-preserving logger with activity trail + egress log (FR-CORE-003/004, ADR-0009)
  - Layered configuration system (FR-CORE-002, ADR-0008)
  - Async event bus: pub/sub + request/response (ADR-0004)
  - Kernel lifecycle bootstrap (FR-CORE-001)
- Module skeleton crates: memory, search, voice, ai, comms, plugin_host
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

## Milestone 2 — Memory Engine (NEXT)

**Objective:** Implement the Memory Engine with a real encrypted local database,
full CRUD, and user inspection/correction/deletion support.

**Deliverables:**
- SQLite-based encrypted store (SQLCipher or equivalent) (FR-MEM-001)
- Memory capture API: text, structured events, file references (FR-MEM-002)
- Memory inspection, correction, deletion (FR-MEM-003)
- Provenance: every memory use recorded in activity trail (FR-MEM-004)
- Full export/import (FR-EXP-001, FR-EXP-002)
- Unit + integration tests for all memory operations

**Exit Criteria:**
- All FR-MEM-* and FR-EXP-* requirements satisfied with passing tests
- Export → wipe → import round-trip verified (NFR-REL-005)
- Encryption at rest verified

---

## Milestone 3 — Universal Search Engine

**Objective:** Implement the Universal Search engine with hybrid semantic + lexical
retrieval over local indexed content.

**Deliverables:**
- Local HNSW vector index (FR-SRCH-002, ADR-0006)
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

## Milestone 4 — AI Engine & Local Inference

**Objective:** Implement the AI Engine with local LLM and embedding inference,
uncertainty surfacing, and the consent-gated acceleration seam.

**Deliverables:**
- `InferenceRuntime` abstraction (ADR-0007)
- Quantized local LLM backend (GGUF/llama.cpp)
- ONNX embedding backend
- Uncertainty surfacing (FR-AI-003)
- Acceleration seam with Egress Gate integration (FR-AI-004)
- Model lifecycle management
- Latency benchmarks (NFR-PERF-002)

**Exit Criteria:**
- Local inference works offline on minimum hardware tier
- Uncertainty expressed for ambiguous inputs
- Remote seam disabled by default; all remote calls in egress log when enabled

---

## Milestone 5 — Voice System

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

## Milestone 6 — Android Shell

**Objective:** Build the Android (Kotlin/Jetpack Compose) shell that binds to the
Rust core via JNI over the C-ABI.

**Deliverables:**
- Android project with Jetpack Compose UI
- JNI bridge to `nova_ffi`
- Background service for always-available capture
- Core NOVA UI flows: search, memory view/edit, activity trail, settings

**Exit Criteria:**
- App installs and runs on Android 10+ (minimum target)
- All M1-M5 features accessible through the Android UI

---

## Milestone 7 — Windows Shell

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

## Milestone 8 — Device Sync & Communication

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

## Milestone 9 — Automation & Plugin System

**Objective:** Implement the Consent Gate, basic automation actions, and the Plugin
Host sandbox.

**Deliverables:**
- Consequence/Consent Gate with classification ruleset (FR-AUTO-001/002/003)
- Core automation actions: file management, reminders, app launch
- Plugin Host sandbox (ADR-0012 — to be written)
- Plugin permission system (ADR-0008 integration)

**Exit Criteria:**
- All FR-AUTO-* requirements pass
- Irreversible actions always require confirmation (even in autonomous mode)
- Plugin sandbox prevents unauthorised resource access

---

## Milestone 10 — Security Hardening, QA & v1.0 Release

**Objective:** Complete security audit, performance profiling, QA, and ship v1.0.

**Deliverables:**
- Security audit (Ch15 requirements implemented)
- Performance profiling on minimum hardware (all NFR-PERF-* targets met)
- Full test suite: unit + integration + E2E + security
- Documentation: user guide, developer guide
- v1.0 release build for Android and Windows

**Exit Criteria:**
- All Milestone 1–9 exit criteria verified
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

*Roadmap version: 0.1 (pre-v1.0). Updates require milestone completion review.*
