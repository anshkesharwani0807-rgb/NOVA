---
document: NOVA Bible
chapter: 3
title: Functional Requirements Specification
status: DRAFT
version: 1.0.0
last_updated: 2026-07-05
depends_on: [Chapter 1 v1.0.0, Chapter 2 v1.0.0, Phase 0 v1.0.0]
authority: Translates Chapter 2 goals into traceable engineering requirements; subordinate to Chapters 1 and 2
---

# CHAPTER 3 — FUNCTIONAL REQUIREMENTS SPECIFICATION

> **Conformance note.** Every requirement in this chapter traces to at least one goal
> in Chapter 2 and one principle in Chapter 1. Nothing here introduces new goals or
> overrides existing ones. Where a requirement would strain a principle, the principle
> wins and the requirement is re-scoped. RFC-2119 modal verbs (MUST / SHOULD / MAY)
> apply throughout from this chapter onward.

---

## 3.0 Purpose

This chapter converts the Chapter 2 goals and personas into a **complete, traceable,
engineering-grade functional requirements specification** — the contract between
product intent and engineering implementation.

A requirement at this level answers: *what must the system do, for whom, under what
conditions, and what is the acceptance criterion?* It does not answer *how* (that is
architecture, Chapters 5+). It does answer *what* precisely enough that an engineer
can build it and a QA engineer can test it.

---

## 3.1 Scope

### 3.1.1 In Scope

- All functional (behaviour) requirements for NOVA v1 (Primary + Secondary goals).
- Traceability markers linking each requirement to Chapter 2 goals (PG-x, SG-x) and
  Chapter 1 principles.
- Acceptance criteria sufficient to drive Chapter 19 (Testing/QA).
- Long-term and stretch requirements are flagged but not fully specified here (Ch20).

### 3.1.2 Out of Scope

- Non-functional requirements: performance, scalability, battery — Chapter 4.
- Architecture and module design — Chapters 5–14.
- Security mechanisms and key management — Chapter 15.
- UI/UX specifics beyond required behaviour — Chapter 17.

### 3.1.3 Requirement ID Convention

Requirements are identified as `FR-<domain>-<seq>`. Domains:

| Code | Domain |
|------|---------|
| `CORE` | Kernel, lifecycle, configuration, logging |
| `MEM` | Memory Engine |
| `SRCH` | Universal Search |
| `VOICE` | Voice & Wake-Word System |
| `AI` | AI Engine & Inference |
| `DEV` | Device Communication & Sync |
| `AUTO` | Automation & Consent Gate |
| `VIS` | Visual Intelligence |
| `PLUG` | Plugin System |
| `SEC` | Security & Privacy gates |
| `EXP` | Export, Backup & Portability |

---

## 3.2 Definitions

- **Capture**: any act of NOVA ingesting user content (photo, note, file, conversation
  snippet) into its memory or index.
- **Recall**: any act of NOVA surfacing a previously captured memory or indexed item
  in response to a query or trigger.
- **Egress Gate**: the mandatory kernel chokepoint that MUST be traversed before any
  data leaves the device; logs every traversal.
- **Consent Gate**: the kernel chokepoint that classifies action stakes and reversibility
  and requires user confirmation for high-stakes or irreversible actions (D8).
- **Offline mode**: the state in which no network interface is available or all remote
  acceleration is disabled; NOVA MUST remain operational.
- **Acceleration seam**: the opt-in, consent-gated path to remote compute or larger
  models when the user enables it and network is available.

---

## 3.3 Core Kernel Requirements (FR-CORE)

### FR-CORE-001 — Kernel bootstrap
**Requirement.** The kernel MUST initialize successfully on first launch, loading the
layered configuration (default → user → device/runtime), initializing the logger, and
starting the event bus, within a defined boot-time budget (specified in Ch4/Ch16).

**Acceptance.** Kernel starts without error; configuration is loaded and validated;
event bus is accepting subscriptions; activity trail records the boot event.

**Traces to:** PG-1, PG-3, Principle 3.

---

### FR-CORE-002 — Layered configuration with private-first defaults
**Requirement.** The configuration system MUST apply settings in the order: built-in
defaults (private-and-conservative) → user overrides → device/runtime overrides.
Security-relevant settings (egress policy, autonomy dial, plugin permissions) MUST
default to the most restrictive value and require explicit user action to widen.

**Acceptance.** A fresh install with no user config has all egress off, telemetry off,
and autonomy level "conservative". Any deviation from these defaults is recorded in the
activity trail.

**Traces to:** Principle 2, Principle 6, D3, D8, ADR-0008.

---

### FR-CORE-003 — Activity trail (user-facing transparency log)
**Requirement.** Every material action taken by NOVA MUST generate a human-readable
entry in the activity trail carrying: timestamp, responsible module, action description,
reason/provenance, and correlation ID. The trail MUST be inspectable by the user at
any time. Entries MUST NOT contain raw PII unless it is the user explicitly viewing
their own data.

**Acceptance.** After any NOVA action the user can open the activity trail and see what
was done and why. PII scrubbing passes a review of 100 random entries.

**Traces to:** Principle 5, PG-3, SG-3, ADR-0009.

---

### FR-CORE-004 — Egress gate: 100% network egress attribution
**Requirement.** No data MUST leave the device except through the Egress Gate. The
Egress Gate MUST log every traversal (destination, purpose, data size, consent status,
correlation ID) before allowing the call. Blocked egress MUST also be logged. The egress
log MUST be inspectable by the user.

**Acceptance.** A network-intercepting test proxy sees zero outbound requests that are
not in the egress log. The egress log is visible to the user from the settings UI.

**Traces to:** Principle 2, D3, PG-3, ADR-0009.

---

### FR-CORE-005 — Graceful degradation; never brick
**Requirement.** If any module fails (model fails to load, storage is full, network is
unavailable), the kernel MUST degrade gracefully: disable the affected capability,
notify the user honestly, and continue operating with remaining capabilities. A module
failure MUST NOT crash unrelated modules or make the application unlaunchable.

**Acceptance.** Simulated module crash (via fault injection in Ch19) leaves other
modules operational. User sees a clear, honest "X is unavailable because Y" message.

**Traces to:** Principle 3, Principle 9, ADR-0010.

---

## 3.4 Memory Engine Requirements (FR-MEM)

### FR-MEM-001 — Durable, encrypted local memory store
**Requirement.** The Memory Engine MUST store all captured memories in an encrypted,
locally-stored database. The database MUST persist across app restarts, device reboots,
and OS updates. Encryption MUST be at rest; keys are managed per ADR-0013.

**Acceptance.** After restart, all memories created before restart are intact and
retrievable. Storage passes an at-rest encryption audit (Ch15).

**Traces to:** Principle 4, D7, PG-2, ADR-0006.

---

### FR-MEM-002 — Memory capture (text, structured events, file references)
**Requirement.** The Memory Engine MUST capture: (a) explicit user-entered memories
(notes, tags, labels), (b) structured life-events (meetings, contacts, locations with
consent), and (c) indexed file references (for Universal Search). Capture MUST be
permission-gated: the user explicitly enables each category.

**Acceptance.** Capture of each category requires a one-time user permission. Disabling
a permission stops new captures of that type immediately. Existing captures are not
deleted unless the user requests it.

**Traces to:** Principle 1, Principle 2, Principle 6, PG-2, D7.

---

### FR-MEM-003 — Memory inspection and correction
**Requirement.** The user MUST be able to view the full list of memories NOVA holds,
inspect the content of any individual memory, correct (edit) any memory, and delete any
memory or all memories. Correction MUST be applied immediately and propagated to any
derived index (e.g. the search vector index).

**Acceptance.** A user edits a memory's content; the change is reflected immediately in
search results. A user deletes a memory; it is irrecoverably removed from all stores
within a defined SLA (Ch4).

**Traces to:** Principle 1, Principle 4, PG-2, SG-3.

---

### FR-MEM-004 — Memory must not be silently used
**Requirement.** Every time NOVA uses a memory to answer a query or take an action,
the system MUST record which memory was used, with what inference, in the activity trail
and MUST be able to show "why" on user request.

**Acceptance.** After any NOVA response that used a memory, the user can tap "why" and
see which memory was referenced and how it influenced the answer.

**Traces to:** Principle 5, FR-CORE-003, PG-3.

---

## 3.5 Universal Search Requirements (FR-SRCH)

### FR-SRCH-001 — Natural-language search over indexed content
**Requirement.** Universal Search MUST accept free-text natural-language queries and
return ranked results from the local index (files, memories, notes, app data that the
user has permitted NOVA to index), entirely offline on the minimum supported hardware
tier.

**Acceptance.** On a minimum-tier device with no network, a natural-language query
("show my birthday photos from 2019") returns results within the latency budget (Ch4).

**Traces to:** PG-1, SG-2, PG-4, Principle 3.

---

### FR-SRCH-002 — Hybrid semantic + lexical retrieval
**Requirement.** Search results MUST combine semantic (embedding-based, meaning-aware)
and lexical (keyword) retrieval. Neither alone is sufficient: semantic covers paraphrase
and concept-matching; lexical covers exact names, identifiers, and code.

**Acceptance.** A query for "invoice from March" finds a file named "march-invoice.pdf"
(lexical) AND a file whose content says "payment summary for quarter 1" (semantic).

**Traces to:** SG-2, PG-4, ADR-0006 (vector store).

---

### FR-SRCH-003 — Permissioned scope
**Requirement.** Search MUST only index content from sources the user has explicitly
permitted. Adding a new source (folder, app, cloud connection) requires a one-time
user grant. Revoking a grant MUST remove the source's indexed data.

**Acceptance.** A folder not in the permitted scope returns zero results. After
permission revocation, re-running the same query returns zero results from that source.

**Traces to:** Principle 1, Principle 2, FR-MEM-002.

---

## 3.6 Voice System Requirements (FR-VOICE)

### FR-VOICE-001 — Wake word detection
**Requirement.** The voice system MUST support always-on wake-word detection using a
small, locally-running keyword spotting model. Detection MUST function offline.
Audio MUST NOT be recorded or retained before the wake word is confirmed detected.

**Acceptance.** Wake word triggers NOVA in a silent room within the false-rejection
latency budget (Ch4). No audio buffer exists before wake word confirmation
(verifiable by memory inspection).

**Traces to:** PG-4, Principle 2, Principle 3.

---

### FR-VOICE-002 — Local speech-to-text (ASR)
**Requirement.** The voice system MUST transcribe user speech to text using an
on-device ASR model, offline. The transcript MUST be available to the AI Engine for
intent parsing within the response-latency budget.

**Acceptance.** On minimum hardware, offline, end-to-end voice→response latency within
Ch4 budget. Word error rate within the target band for standard English.

**Traces to:** PG-4, PG-1, Principle 3.

---

### FR-VOICE-003 — Text-to-speech (TTS) response
**Requirement.** NOVA MUST be able to deliver responses via synthesized speech using
an on-device TTS model, offline.

**Acceptance.** Responses are voiced within the latency budget; user can disable voice
output in settings.

**Traces to:** PG-4, Principle 3.

---

## 3.7 AI Engine Requirements (FR-AI)

### FR-AI-001 — On-device language understanding and generation
**Requirement.** The AI Engine MUST support natural-language understanding and
generation using a quantized local language model, offline, on all supported hardware
tiers. The system MUST degrade to reduced capability (shorter context, simpler model)
on lower tiers rather than failing.

**Acceptance.** Intent parsing and basic response generation work offline on minimum
hardware. The user is informed when operating in a reduced-capability mode.

**Traces to:** PG-1, PG-4, Principle 3, Principle 9, ADR-0007.

---

### FR-AI-002 — Local embedding generation (for search & memory)
**Requirement.** The AI Engine MUST generate semantic embeddings locally for content
ingested by Memory and Universal Search. Embedding generation MUST be offline.

**Acceptance.** New captured content is embedded and searchable without network.

**Traces to:** FR-SRCH-002, FR-MEM-001, Principle 3.

---

### FR-AI-003 — Uncertainty surfacing (Principle 9)
**Requirement.** When the AI Engine's confidence is below a defined threshold, it MUST
surface uncertainty explicitly in its response ("I'm not certain, but...") rather than
presenting a confident but wrong answer. "I don't know" MUST be a first-class, valid
outcome.

**Acceptance.** Test cases with ambiguous queries trigger explicit uncertainty
expressions. No calibration test produces a confident answer with measured confidence
below the threshold.

**Traces to:** Principle 9, PG-3, PG-2.

---

### FR-AI-004 — Consent-gated remote acceleration seam
**Requirement.** When the user has enabled it and network is available, the AI Engine
MAY route requests to a remote backend for higher-capability inference. This MUST go
through the Egress Gate (FR-CORE-004) and MUST be disabled by default.

**Acceptance.** Remote calls only occur when the user has explicitly enabled the seam.
Every remote call appears in the egress log. Disabling the seam reverts to local-only
immediately, with no queued outbound calls.

**Traces to:** D3, Principle 2, Principle 3, ADR-0007.

---

## 3.8 Device Communication Requirements (FR-DEV)

### FR-DEV-001 — Opt-in cross-device sync for the same user
**Requirement.** The Device Communication module MUST enable the same user to
synchronise memory and search index across their approved Android and Windows devices.
Sync MUST be end-to-end encrypted, disabled by default, and require explicit user
activation per device pair. Disabling sync MUST fully isolate devices.

**Acceptance.** A memory created on Device A appears on Device B within the sync-window
SLA (Ch4) when sync is enabled. With sync disabled, no data flows between devices (verifiable by egress log).

**Traces to:** SG-1, Principle 2, Principle 3, D3.

---

### FR-DEV-002 — No cloud hub; direct-or-local sync only
**Requirement.** Sync MUST NOT route through a persistent cloud intermediary that
retains user data. The sync channel is either direct device-to-device or through a
transit relay that is zero-knowledge with respect to content.

**Acceptance.** Architecture review confirms no unencrypted user data touches a server
log. Transit relay (if used) receives only blobs it cannot decrypt.

**Traces to:** Principle 2, Principle 3, D1.

---

## 3.9 Automation & Consent Gate Requirements (FR-AUTO)

### FR-AUTO-001 — Consequence classification for every action
**Requirement.** Before executing any action on behalf of the user, the Consent Gate
MUST classify the action's stakes (low/medium/high) and reversibility
(reversible/irreversible). The classification MUST be based on a defined, auditable
ruleset, not ad-hoc per-feature logic.

**Acceptance.** A complete ruleset exists and is tested (Ch19). Actions are classified
reproducibly: the same action type always receives the same classification unless
the ruleset is updated.

**Traces to:** Principle 6, D8, PG-5.

---

### FR-AUTO-002 — Autonomous execution for low-stakes/reversible actions
**Requirement.** Actions classified as low-stakes AND reversible MAY be executed
autonomously (without user confirmation) when the user's autonomy dial is set to
"moderate" or "autonomous". In "conservative" mode, ALL actions require confirmation.

**Acceptance.** In "conservative" mode, no action is taken without a user tap/confirm.
In "moderate" mode, file-read operations are autonomous; file-delete operations require
confirmation.

**Traces to:** Principle 6, D8, PG-5.

---

### FR-AUTO-003 — Mandatory confirmation for irreversible/high-stakes actions
**Requirement.** Actions classified as irreversible OR high-stakes MUST require explicit
user confirmation regardless of the autonomy dial setting. The confirmation dialog MUST
state plainly what will happen and that it cannot be undone.

**Acceptance.** A destructive-action test (file delete, send message) always produces a
confirmation dialog, even in "autonomous" mode. Confirmation dialogs include the phrase
"this cannot be undone" where applicable.

**Traces to:** Principle 1, Principle 6, D8, PG-5.

---

## 3.10 Export & Portability Requirements (FR-EXP)

### FR-EXP-001 — Full memory export
**Requirement.** The user MUST be able to export the complete NOVA memory and
configuration as a portable, documented archive. The archive MUST be re-importable on
a new device or a fresh NOVA install without data loss.

**Acceptance.** Export → wipe → import cycle preserves 100% of memories and
configuration. Export format is documented (Ch14).

**Traces to:** Principle 7, Principle 1, PG-2, XG-3.

---

### FR-EXP-002 — Selective deletion and right-to-be-forgotten
**Requirement.** The user MUST be able to delete any subset of memories, or all
memories, permanently. Deletion MUST propagate to all derived stores (search index,
embeddings) within a defined SLA. Deleted data MUST be irrecoverable from local storage.

**Acceptance.** After deletion, a search for the deleted content returns zero results.
Storage forensics on the device (Ch19 security tests) finds no recoverable remnants
after the SLA window.

**Traces to:** Principle 1, Principle 4, FR-MEM-003.

---

## 3.11 Risks

| ID | Risk | Mitigation |
|----|------|------------|
| R3-1 | Requirements creep broadening scope | Every FR must cite a Chapter 2 goal; uncited requirements are rejected |
| R3-2 | Acceptance criteria too vague to test | Ch19 review gate: every FR must have at least one automated test |
| R3-3 | Privacy gates omitted in feature implementation | FR-CORE-004 is a hard contract; architectural enforcement via gate chokepoints |
| R3-4 | Offline requirements traded away for features | Principle 3 overrules convenience; Ch4 NFRs encode offline targets as hard limits |

---

## 3.12 Open Questions

- **OQ-3.1:** Should the consent classification ruleset be user-configurable, or is it
  hardcoded with only the autonomy-dial as the user lever? *Default: hardcoded ruleset,
  dial is the only user variable. Revisit in Ch6.*
- **OQ-3.2:** What is the exact export format for the portable archive? *Deferred to
  Ch14 (Database Architecture) which owns schema and export format.*

---

*End of Chapter 3.*
