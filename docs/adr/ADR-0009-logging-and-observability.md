# ADR-0009 — Logging & Observability (Privacy-Preserving)

- **Decision ID:** ADR-0009
- **Status:** Proposed
- **Date:** 2026-07-04
- **Governed by:** Principle 2 (privacy), Principle 5 (transparency/provenance), D3
  (egress), Principle 1 (inspectable). **Builds on:** ADR-0003/0004.

## Context

NOVA needs observability for debugging, for the user-facing **activity trail** (why did
NOVA do X — Principle 5), and for the **egress log** (D3). But logs are a classic privacy
leak: they often contain sensitive user data and are frequently shipped off-device. For
a privacy-first product, logging must be reconciled with Principles 1 and 2.

## Options Considered

1. **Conventional logging that ships to a remote service.** Rejected outright — violates
   D1/D3; no silent egress of user data, ever.
2. **Local-only structured logging with severity tiers and explicit data-classification**,
   plus a separate user-facing activity/egress trail derived from event-bus provenance.
3. **Minimal/no logging.** Rejected — undermines debuggability and the transparency the
   product promises.

## Chosen Solution

**Local-only, structured, data-classified logging (Option 2), split into three planes:**

- **Diagnostic log (developer-facing):** structured, severity-tiered, **on-device only**.
  Fields are tagged by data-classification; anything user-sensitive is redacted or
  hashed by default. Never leaves the device except via an explicit, consent-gated
  export through the Egress Gate (D3) — e.g. a user choosing to share a bug report.
- **Activity trail (user-facing):** a human-readable record of what NOVA did and **why**,
  derived from event-bus provenance (ADR-0004). Satisfies Principle 5 and JTBD-7. Fully
  inspectable by the user (Principle 1).
- **Egress log (user-facing):** the mandatory record of every network egress event —
  what left, where, why, under what consent (D3). Target: 100% egress attributable
  (the egress-transparency KPI).

- **No telemetry by default.** Any optional, aggregate, privacy-preserving metric is
  opt-in and routed through the Egress Gate; off by default (Principle 2, audit §7).

## Trade-offs

- **(-) Redaction/classification effort** on every log site. *Accepted:* it is the price
  of not leaking user data; enforced by lint/review (Ch18).
- **(-) Local-only logs complicate remote debugging.** *Mitigated* by consent-gated
  export of redacted diagnostics when the user chooses.
- **(+) Transparency by construction** — activity and egress trails make the privacy
  promise demonstrable, not asserted.

## Consequences

- The Step-3 "Logging Framework" spec defines the three planes, severity tiers, the
  data-classification tagging rule, and redaction defaults.
- The activity/egress trails are user-facing surfaces (Ch7, Ch15, Ch17).
- The "no telemetry by default" rule binds every module and plugin.
