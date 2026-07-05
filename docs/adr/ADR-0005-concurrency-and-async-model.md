# ADR-0005 — Concurrency & Asynchronous Execution Model

- **Decision ID:** ADR-0005
- **Status:** Proposed
- **Date:** 2026-07-04
- **Governed by:** ADR-0001/0003/0004, Principle 3 (offline/low-latency), Ch16
  (battery/thermal budgets), PG-4 (fast interaction).

## Context

NOVA runs latency-sensitive work (voice, search) alongside heavy background work
(indexing, inference) on battery-constrained devices. The event bus (ADR-0004) needs a
concurrency substrate. We must choose the execution model for the Rust core.

## Options Considered

1. **OS threads only (blocking).** Simple mental model but poor at many concurrent I/O
   tasks; thread-per-task is memory-heavy on mobile.
2. **Async/await with a runtime (task-based).** Efficient for many concurrent I/O-bound
   tasks; cooperative scheduling; good fit for an event bus.
3. **Async runtime + a bounded blocking/compute thread pool for CPU-heavy work**
   (inference, indexing, crypto). Hybrid: async for orchestration/I/O, dedicated pool
   for CPU-bound tasks so they don't stall the async scheduler.
4. **Manual thread pools + queues (no async).** Flexible but reinvents scheduling.

## Chosen Solution

**A single async runtime for orchestration and I/O, plus a bounded, prioritized
compute thread pool for CPU-heavy work (Option 3).**

- **Async runtime:** drives the event bus, module mailboxes, and all I/O (storage,
  network via the Egress Gate, IPC). Cooperative and memory-efficient for many tasks.
- **Compute pool:** CPU-bound jobs (model inference, embedding, indexing, encryption)
  run on a **bounded** pool sized to the device tier (Ch16), with **priority classes**
  so interactive work (voice/search) preempts background work (bulk indexing).
- **Backpressure & cancellation** are first-class: queues are bounded, tasks are
  cancellable, and low-priority background work yields under thermal/battery pressure
  (Ch16). This directly protects PG-4 latency and battery KPIs.

## Trade-offs

- **(-) Two schedulers to reason about** (async + compute pool). *Mitigated* by a clear
  rule: I/O and orchestration are async; anything CPU-bound and long goes to the pool.
- **(-) Async complexity** (lifetimes, cancellation). *Accepted:* it is the right model
  for a many-task, I/O-heavy, battery-sensitive core; Rust's async is mature enough.
- **(+) Interactive latency is protected** from background load via priorities and
  preemption — essential for the "fast, natural interaction" goal (PG-4).
- **(+) Battery/thermal adaptivity** via backpressure aligns with Ch16 budgets.

## Consequences

- The Core Engine and Event Bus specs (Step 3) assume this model.
- Every module must classify its work as interactive vs. background and honor priority/
  backpressure — a cross-cutting requirement into Ch16.
- The specific async runtime library is an implementation choice (kept behind the core's
  own abstractions to preserve Principle 7 optionality); the *model* is what is fixed here.
