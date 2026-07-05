# ADR-0004 — Inter-Module Communication & the Internal Event Bus

- **Decision ID:** ADR-0004
- **Status:** Proposed
- **Date:** 2026-07-04
- **Governed by:** ADR-0003, Principle 5 (transparency/provenance), D3, D8.

## Context

Modules (ADR-0003) must communicate without tight coupling, and NOVA must be able to
show *why* it did something (Principle 5), which requires an observable flow of
messages. We must choose how modules talk to each other inside the core.

## Options Considered

1. **Direct method calls between modules.** Simple but tightly couples modules, defeats
   the plugin story, and makes flow hard to observe/trace.
2. **In-process asynchronous event bus (publish/subscribe + request/response).**
   Modules emit and consume typed events; the kernel routes them. Loose coupling,
   observable, plugin-friendly.
3. **Full external message broker (e.g. a networked queue).** Massive overkill on a
   single device; adds latency, battery, and dependencies. Rejected (Principle 3).
4. **Actor model (each module an actor with a mailbox).** Strong isolation and a good
   fit for Rust async; effectively a structured form of Option 2.

## Chosen Solution

**An in-process, asynchronous, typed Event Bus in the kernel (Option 2), with
actor-style module mailboxes (Option 4) as the module execution model.**

- **Two interaction styles:** (a) **publish/subscribe** for events (e.g. "memory
  captured", "query issued"), and (b) **request/response** for directed calls (e.g.
  "search: run query"), both typed and versioned.
- **Every message carries provenance metadata** (origin module, correlation id,
  timestamp, causing action) so the system can reconstruct "why" for Principle 5 and
  feed the activity trail (Ch7/Ch15).
- **The bus is the only inter-module channel.** Modules do not call each other's
  internals. Plugins interact *exclusively* through the bus under policy (ADR-0012).
- **Gate integration:** events that imply network egress or a consequential action are
  routed through the Egress Gate (D3) / Consent Gate (D8) before proceeding.

## Trade-offs

- **(-) Indirection** vs. direct calls — slightly harder to follow statically. *Mitigated*
  by typed events and correlation ids that make runtime flow *more* traceable.
- **(-) Serialization/copy overhead** for messages. *Mitigated* by zero-copy/borrowed
  payloads where safe (Rust) and by keeping hot paths direct within a module.
- **(+) Loose coupling** enables modules and plugins to evolve independently (Ch13,
  Principle 7).
- **(+) Observability by construction** — the bus is the natural place to record the
  provenance the transparency principle requires.

## Consequences

- The Event Bus is a core kernel facility, specified in Step 3 ("Event Bus" and
  "IPC / Inter-module Communication").
- Cross-*process* communication (Android service ↔ UI) bridges the bus over the C-ABI/
  IPC seam (ADR-0002) using the same typed, versioned message contracts.
- Provenance metadata is mandatory on messages — a requirement inherited by every
  module and by the logging spec (ADR-0009).
