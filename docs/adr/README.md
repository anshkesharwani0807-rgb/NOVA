# Architecture Decision Records (ADRs)

This directory holds the Architecture Decision Records for NOVA. An ADR captures a
single significant technical decision: its context, the options considered, the chosen
solution, the trade-offs, and the consequences.

## Status of this batch

These ADRs are authored in **Engineering Mode (Phase 1)** to reach engineering
readiness before coding. They **finalize the open technical questions** (notably OQ-2,
the technology stack) on the owner's delegated authority ("do what you think best"),
and they are **proposals the owner may veto**. Once ratified, the relevant decisions
must be reconciled into Bible Chapters 5 (Architecture) and 6 (Modules), which remain
the long-form source of truth; ADRs are the decision-level record.

## ADR lifecycle

- **Proposed** — drafted, awaiting ratification.
- **Accepted** — ratified; binding until superseded.
- **Superseded** — replaced by a later ADR (linked); kept for history (Principle 7).

## Conformance

Every ADR must conform to the nine ordered principles (Chapter 1). Where an ADR is
constrained by a principle or decision, it cites the ID (e.g. D1, D3, Principle 7).

## Index

| ID | Title | Status | Unblocks |
|---|---|---|---|
| ADR-0001 | Core language & technology stack | Proposed | Everything (resolves OQ-2) |
| ADR-0002 | Cross-platform strategy (shared core + native shells) | Proposed | Ch5, Ch12, UI |
| ADR-0003 | Application architecture pattern (modular monolith / microkernel core) | Proposed | Ch5, Ch6 |
| ADR-0004 | Inter-module communication & the internal Event Bus | Proposed | Core Engine, Modules, IPC |
| ADR-0005 | Concurrency & asynchronous execution model | Proposed | Core Engine, Performance |
| ADR-0006 | Local persistence & storage engine | Proposed | Memory, Search, DB (Ch14) |
| ADR-0007 | On-device AI inference runtime | Proposed | AI Engine (Ch11) |
| ADR-0008 | Configuration system | Proposed | Config System spec |
| ADR-0009 | Logging & observability (privacy-preserving) | Proposed | Logging spec |
| ADR-0010 | Error-handling strategy | Proposed | Error-handling spec |
| ADR-0011 | Dependency injection / composition | Planned (next) | DI spec |
| ADR-0012 | Plugin system & sandboxing model | Planned (next) | Plugin Loader (Ch13) |
| ADR-0013 | Encryption & key management | Planned (next) | Security (Ch15) |
| ADR-0014 | Build system & toolchain | Planned (next) | CI, Dev Standards (Ch18) |

## Template

Each ADR contains: **Decision ID**, **Status**, **Context**, **Options Considered**,
**Chosen Solution**, **Trade-offs**, **Consequences**, and principle citations.
