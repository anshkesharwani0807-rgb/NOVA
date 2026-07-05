# ADR-0003 — Application Architecture Pattern (Modular Monolith / Microkernel Core)

- **Decision ID:** ADR-0003
- **Status:** Proposed
- **Date:** 2026-07-04
- **Governed by:** D1, D2 (single-user), D7 (memory first-class), D8 (consent gate),
  Principle 8 (coherence), Principle 3. **Builds on:** ADR-0001/0002.

## Context

NOVA is a single-user (D2), on-device (D1) application, not a distributed multi-tenant
system. Its scalability axis is per-device resources over years, not server fan-out
(audit §9). Yet it must be **modular** (Memory, Search, Voice, AI, Device Comms,
Consent Gate, Plugin host) and **extensible** (Ch13). We must choose an architecture
pattern for the core.

## Options Considered

1. **Distributed microservices.** Rejected outright: NOVA runs on one device for one
   user; process/network overhead, complexity, and battery cost are all unjustified
   (violates Principle 3/8). Multi-tenant scale is explicitly out of scope (D2).
2. **Monolith (unstructured).** Simple but becomes an unmaintainable ball of mud;
   fails Principle 8 (coherence) at scale and blocks the plugin story (Ch13).
3. **Modular monolith with a microkernel core.** A small kernel (lifecycle, event bus,
   config, DI/registry, consent/egress gates) hosts well-isolated modules that
   communicate via the event bus; plugins load into the same host under a sandbox.
4. **Multi-process on-device (separate processes per subsystem).** Stronger isolation
   but heavy IPC/battery cost; reserved only where the OS *requires* it (e.g. Android
   background service vs. UI process).

## Chosen Solution

**A modular monolith organized around a microkernel core (Option 3), using multi-
process boundaries (Option 4) only where the platform mandates them.**

- **Microkernel ("NOVA Core / Kernel"):** owns process lifecycle, the internal event
  bus (ADR-0004), configuration (ADR-0008), the module/service registry and composition
  (ADR-0011), logging (ADR-0009), error handling (ADR-0010), and the two mandatory
  gates — the **Consent/Consequence Gate** (D8) and the **Egress Gate** (D3). Nothing
  reaches the network except through the Egress Gate.
- **Modules:** Memory Engine, Universal Search, Voice, AI Engine, Device Communication,
  and the Plugin Host register with the kernel and interact only via the event bus and
  well-defined interfaces — never by reaching into each other's internals.
- **Process boundaries:** kept minimal. Where the OS separates UI and background work
  (Android service), the C-ABI/IPC seam (ADR-0002, IPC spec) bridges them. Plugins may
  later be isolated in a sandboxed process/WASM boundary (ADR-0012) for security.

## Trade-offs

- **(-) A monolith shares a failure domain** (a crash can take down modules). *Mitigated*
  by Rust safety (ADR-0001), strict error handling (ADR-0010), and module isolation via
  the event bus; the single-user/on-device context makes this acceptable.
- **(-) Less physical isolation than microservices.** *Accepted:* isolation needs are
  met by module boundaries + plugin sandboxing, not process sprawl.
- **(+) Low latency, low battery, simple deployment** — exactly what on-device-first
  demands (Principle 3).
- **(+) Coherent, inspectable structure** with mandatory central gates for consent and
  egress — the architecture *enforces* Principles 2 and 6 structurally.

## Consequences

- The kernel is the home of the Step-3 "Core Engine" spec; the event bus, config,
  logging, error handling, DI, and IPC specs all describe kernel facilities.
- Every network-capable feature MUST route through the Egress Gate; every action MUST
  pass the Consent Gate. These are non-optional kernel chokepoints (D3, D8).
- Module boundaries defined here seed Bible Chapter 6.
