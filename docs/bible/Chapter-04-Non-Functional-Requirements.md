---
document: NOVA Bible
chapter: 4
title: Non-Functional Requirements
status: DRAFT
version: 1.0.0
last_updated: 2026-07-05
depends_on: [Chapter 1 v1.0.0, Chapter 2 v1.0.0, Chapter 3 v1.0.0]
authority: Specifies measurable quality constraints; subordinate to Chapters 1-3
---

# CHAPTER 4 — NON-FUNCTIONAL REQUIREMENTS

> **Conformance note.** This chapter converts Chapter 2 KPIs and Chapter 3 functional
> requirements into hard, measurable quality constraints. NFRs are not wishes — they
> are engineering commitments that gate shipping. Where an NFR appears to conflict with
> a Chapter 1 principle, the principle wins and the NFR is re-scoped downward.

---

## 4.0 Purpose

Non-functional requirements (NFRs) define the quality envelope within which NOVA's
functional requirements must operate. They answer: *how fast, how reliable, how
private, how efficient, and how safe* — with numbers that drive Chapter 16
(Performance & Scalability) and Chapter 19 (Testing & QA).

This chapter establishes **hard limits** (MUST), **strong targets** (SHOULD), and
**aspirational goals** (MAY) across seven quality dimensions.

---

## 4.1 Performance & Latency

All latency targets are measured on the **minimum supported hardware tier** (defined
as a device representative of a 3-year-old mid-range Android phone and a typical
Windows laptop without discrete GPU), **offline**, unless stated otherwise.

| ID | Requirement | Target | Hard Limit |
|----|-------------|--------|------------|
| NFR-PERF-001 | Voice wake-word detection latency | < 300 ms | 500 ms |
| NFR-PERF-002 | End-of-speech → NOVA response onset (local ASR + AI) | < 1.5 s | 3 s |
| NFR-PERF-003 | Universal Search: first-result latency (local, typical query) | < 800 ms | 2 s |
| NFR-PERF-004 | Memory recall: single item retrieval by ID | < 100 ms | 300 ms |
| NFR-PERF-005 | Kernel cold-start to event bus ready | < 2 s | 5 s |
| NFR-PERF-006 | Configuration load & validation on startup | < 200 ms | 500 ms |
| NFR-PERF-007 | Memory deletion propagation to all stores | < 5 s | 30 s |

**Rationale.** Latency is the felt advantage of on-device-first. If NOVA is slower than
a cloud call, users will disable offline mode, undermining Principle 3. The targets
above are conservative enough to be achievable on minimum hardware while remaining
perceptibly responsive.

---

## 4.2 Resource Usage (Battery & Memory)

| ID | Requirement | Target | Hard Limit |
|----|-------------|--------|------------|
| NFR-RES-001 | Background battery drain (idle, wake-word listening) | < 2% / hour | 4% / hour |
| NFR-RES-002 | Active AI inference battery drain (per session) | Within device thermal budget | No throttling within 10 min session |
| NFR-RES-003 | Resident memory (kernel + modules, no active inference) | < 150 MB RAM | 300 MB RAM |
| NFR-RES-004 | Peak memory during local inference | < 2 GB RAM | 4 GB RAM |
| NFR-RES-005 | Local storage for NOVA install (excl. user data & models) | < 50 MB | 100 MB |
| NFR-RES-006 | Default AI model package download size | < 1.5 GB | 4 GB |

**Rationale.** An assistant that drains the battery or hogs RAM gets uninstalled. On
mobile, battery is a trust KPI (Chapter 2, §2.4). The background drain target
(< 2%/hr) is achievable with a small always-on keyword spotter and efficient async
scheduling (ADR-0005).

---

## 4.3 Reliability & Availability

| ID | Requirement | Target | Hard Limit |
|----|-------------|--------|------------|
| NFR-REL-001 | App crash rate (unhandled panics) | < 0.1% of sessions | < 0.5% |
| NFR-REL-002 | Memory store data integrity (no corruption across restarts) | 100% | 100% (zero tolerance) |
| NFR-REL-003 | Search index consistency with memory store | 100% eventually consistent after any write | Within 30 s |
| NFR-REL-004 | Graceful degradation: module failure isolates to that module | 100% of tested failure modes | 100% |
| NFR-REL-005 | Backup/export integrity (export → import round-trip) | 100% data fidelity | 100% |

**Rationale.** Memory is sacred (Principle 4). Data corruption is the single worst
failure mode for a lifelong companion product — it destroys trust irreversibly. Zero
tolerance on NFR-REL-002 and NFR-REL-005 is intentional and non-negotiable.

---

## 4.4 Privacy & Security

| ID | Requirement | Binding Level |
|----|-------------|---------------|
| NFR-SEC-001 | All local stores MUST be encrypted at rest with keys not stored in plaintext | MUST |
| NFR-SEC-002 | Zero bytes leave the device without Egress Gate traversal and activity log entry | MUST (100%) |
| NFR-SEC-003 | Telemetry is disabled by default; opt-in only; zero PII in telemetry payloads | MUST |
| NFR-SEC-004 | Audio is not buffered before wake-word confirmation | MUST |
| NFR-SEC-005 | Plugin code runs in a sandboxed environment with declared, limited permissions | MUST |
| NFR-SEC-006 | All egress is over authenticated, encrypted channels (TLS 1.3+) | MUST |
| NFR-SEC-007 | User data export is encrypted with a user-controlled key | MUST |
| NFR-SEC-008 | Security-relevant configuration changes are logged to the activity trail | MUST |

**Rationale.** Security NFRs here are the engineering formalization of Principles 1, 2,
and 4. They are `MUST` with no exceptions — a violation of any of these is a
**product defect**, not a performance trade-off.

---

## 4.5 Scalability (Per-Device, Single-User)

NOVA's scalability axis is depth-per-user, not breadth-across-users. These NFRs
govern how NOVA scales with the user's own accumulating data over years.

| ID | Requirement | Target |
|----|-------------|--------|
| NFR-SCALE-001 | Memory store: must operate without degradation up to 1M memory entries | MUST |
| NFR-SCALE-002 | Search index: must return results within latency budget with 500K indexed documents | MUST |
| NFR-SCALE-003 | Storage growth: growth rate per year estimated and bounded at design time | < 10 GB/year (excl. media thumbnails) |
| NFR-SCALE-004 | Auto-compaction/pruning available (user opt-in) to manage growth | SHOULD |

---

## 4.6 Maintainability & Longevity

| ID | Requirement | Binding Level |
|----|-------------|---------------|
| NFR-MAINT-001 | All data formats are versioned; migration path exists for every major version bump | MUST |
| NFR-MAINT-002 | The core (Rust) builds on a fresh machine using only publicly available tools | MUST |
| NFR-MAINT-003 | No hard dependency on a single vendor whose exit kills the product | MUST (Principle 7) |
| NFR-MAINT-004 | Public API surface of the C-ABI FFI is versioned and backward-compatible | MUST |
| NFR-MAINT-005 | Automated test coverage for all kernel subsystems | SHOULD ≥ 80% line coverage |
| NFR-MAINT-006 | CI pipeline catches regressions before merge | MUST |

---

## 4.7 Accessibility & Internationalisation

| ID | Requirement | Target |
|----|-------------|--------|
| NFR-ACCESS-001 | UI meets WCAG 2.1 AA for all primary user flows | SHOULD (v1); MUST (v2) |
| NFR-ACCESS-002 | Voice interface functions as an accessibility channel (hands-free primary interaction) | MUST |
| NFR-ACCESS-003 | Text is localizable (strings externalized, no hardcoded UI copy) | MUST |
| NFR-ACCESS-004 | Date, number, and unit formats honour device locale | SHOULD |

---

## 4.8 Risks

| ID | Risk | Mitigation |
|----|------|------------|
| R4-1 | Local model performance insufficient on minimum hardware | Hardware benchmark gating in CI; model tiering (Ch11/Ch16) |
| R4-2 | Battery drain exceeds target on some device models | Per-device profiling; adaptive scheduling (ADR-0005) |
| R4-3 | NFR-REL-002 violated by storage bug | Transactional writes; integrity checksums on every write (Ch14) |
| R4-4 | NFR-SEC-002 violated by unreviewed code path | Architectural chokepoint enforcement; egress-gate test in CI |

---

## 4.9 Open Questions

- **OQ-4.1:** Exact minimum hardware spec (CPU, RAM, storage) to be locked before
  Ch16. *Default: a Qualcomm Snapdragon 700-class SoC with 6 GB RAM on Android;
  an Intel Core i5 10th-gen equivalent with 8 GB RAM on Windows.*
- **OQ-4.2:** Model tiering strategy (which model on which tier) owned by Ch11.

---

*End of Chapter 4.*
