# NOVA Decision Log

A consolidated index of material decisions across the NOVA Bible. Mandated by Phase 0
section 0.3 (decisions are first-class objects with IDs). This is a living document;
each Bible chapter remains the authoritative source for its own decisions.

## Legend

- **ID** — decision identifier (D<n> for Chapter 1; D<chapter>.<n> thereafter).
- **Status** — Active / Amended / Superseded.
- **Source** — the chapter that owns the decision.

## Chapter 1 — Product Vision & Philosophy

| ID | Decision | Status | Source |
|---|---|---|---|
| D1 | On-device-first architecture; cloud is an optional consent-gated accelerant | Active | Ch1 section 1.6.1 |
| D2 | Single-user, single-instance model; clean same-user multi-device seam | Active (pending OQ-1) | Ch1 section 1.6.2 |
| D3 | Privacy-by-default; egress is a privileged, logged, consent-gated chokepoint | Active | Ch1 section 1.6.3 |
| D4 | The nine principles are strictly ordered; lower number wins on conflict | Active | Ch1 section 1.6.4 |
| D5 | Concrete technology recommendations with documented alternatives | Active (pending OQ-2) | Ch1 section 1.6.5 |
| D6 | Android + Windows are the launch platforms; Linux/macOS later | Active | Ch1 section 1.6.6 |
| D7 | Memory is a first-class, durable, encrypted, owned subsystem | Active | Ch1 section 1.6.7 |
| D8 | Agentic behavior, always classified by stakes and gated by consent | Active | Ch1 section 1.6.8 |

## Open questions (cross-chapter)

| ID | Question | Default | Must resolve before |
|---|---|---|---|
| OQ-1 | Single-user vs. multi-user for v1 | Single-user + seam | Ch8/12/14/15 finalized |
| OQ-2 | Concrete stack vs. fully abstract | Concrete + alternatives | Ch5 finalized |
| OQ-3 | Positive business model (prohibitions set) | Undecided | Pricing/packaging |
| OQ-4 | Definition of "what matters" for memory | Selective/meaningful | Ch8 |
| OQ-5 | Autonomy default calibration | Conservative | Ch6/Ch11 |
| OQ-6 | Linux/macOS timing | Deferred | Ch20 |

## Architecture Decision Records (docs/adr/)

| ID | Decision | Status | Source |
|---|---|---|---|
| ADR-0001 | Core in Rust; shared core + C-ABI FFI (resolves OQ-2) | Proposed | docs/adr |
| ADR-0002 | Shared core + platform-native UI shells (Compose MP fallback) | Proposed | docs/adr |
| ADR-0003 | Modular monolith / microkernel core; multi-process only where OS-forced | Proposed | docs/adr |
| ADR-0004 | In-process typed async Event Bus with provenance; sole inter-module channel | Proposed | docs/adr |
| ADR-0005 | Async runtime for I/O + bounded prioritized compute pool | Proposed | docs/adr |
| ADR-0006 | Encrypted SQLite-class store (system of record) + embedded HNSW vector index | Proposed | docs/adr |
| ADR-0007 | NOVA-owned InferenceRuntime interface; pluggable open local backends + consent-gated seam | Proposed | docs/adr |
| ADR-0008 | Centralized layered typed configuration; private/conservative defaults; no remote layer | Proposed | docs/adr |
| ADR-0009..0014 | Logging, error handling, DI, plugin sandboxing, key mgmt, build toolchain | Planned | docs/adr |

## Repository / governance decisions

| ID | Decision | Status | Source |
|---|---|---|---|
| RD-1 | License is MIT | Active | Owner instruction 2026-07-04 |
| RD-2 | All artifacts stored on D: drive; nothing on C: | Active | Owner instruction |
| RD-3 | Scaffold architecture-independent skeleton now; architecture-dependent folders (src/modules/plugins/sdk/api/db) are provisional until Ch5/6/13/14 | Active | Baseline audit + owner delegation |

> **How to add a decision:** when a chapter records a new decision, add a row here with
> its ID, one-line summary, status, and source. On amendment, change status and note
> the superseding decision; never delete history (Principle 7).
