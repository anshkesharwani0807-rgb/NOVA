# ADR-0010 — Error-Handling Strategy

- **Decision ID:** ADR-0010
- **Status:** Proposed
- **Date:** 2026-07-04
- **Governed by:** Principle 3 (graceful degradation, never brick), Principle 9 (honesty
  about limits), Principle 5 (transparency), ADR-0001 (Rust). **Builds on:** ADR-0003/0005.

## Context

On-device-first means NOVA must degrade gracefully when a subsystem fails (a model won't
load, storage is full, the acceleration seam is unreachable) — it must never become a
brick (Principle 3) and must be honest about what failed (Principle 9). We need a uniform
error model across the Rust core and across the C-ABI/IPC seam.

## Options Considered

1. **Exceptions / panics for control flow.** Rust discourages this; panics should be for
   truly unrecoverable states. Rejected as the primary mechanism.
2. **Typed, explicit result values (Result-style) with a structured error taxonomy**,
   plus a small set of well-defined recovery strategies (retry, fallback, degrade,
   surface-to-user).
3. **Error codes only (C-style).** Loses context and structure across a large core.

## Chosen Solution

**Typed, explicit results with a structured error taxonomy and defined recovery
strategies (Option 2).**

- **Recoverable vs. fatal:** ordinary failures are values (Result-style), handled
  explicitly; panics are reserved for genuinely unrecoverable invariants and are caught
  at module boundaries so one module's panic cannot brick the whole app (bounds ADR-0003's
  shared-failure-domain risk).
- **Error taxonomy:** every error carries a category (e.g. Storage, Inference, Egress-
  Denied, Consent-Required, Config-Invalid, Plugin), a stable code, a user-safe message,
  and provenance (correlation id from ADR-0004). No sensitive data in error text
  (aligns with ADR-0009 redaction).
- **Recovery strategies (explicit per call site):** **retry** (transient), **fallback**
  (e.g. seam unreachable → local backend, per ADR-0007), **degrade** (reduced capability,
  clearly communicated — Principle 3/9), or **surface-to-user** (honest "I couldn't do X
  because Y" — Principle 9). Silent failure is prohibited.
- **Across the FFI/IPC seam:** errors are mapped to a stable, versioned representation so
  shells receive structured, localizable errors, not opaque codes.

## Trade-offs

- **(-) Verbosity** of explicit error handling. *Accepted:* it is the Rust-idiomatic,
  safe approach and forces conscious handling of every failure — appropriate for a trust
  product.
- **(+) No silent failures; graceful degradation** — directly serves Principles 3 and 9
  and the "daily usefulness offline" goal (PG-1).

## Consequences

- The Step-3 "Error Handling" spec defines the taxonomy, the recovery-strategy set, the
  boundary-panic-catch rule, and the FFI/IPC error mapping.
- "Degrade, never brick" and "surface honestly" become testable requirements (Ch19).
- Ties to ADR-0007 (seam→local fallback) and ADR-0009 (no sensitive data in errors).
