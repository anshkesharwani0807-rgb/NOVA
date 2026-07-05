# ADR-0008 — Configuration System

- **Decision ID:** ADR-0008
- **Status:** Proposed
- **Date:** 2026-07-04
- **Governed by:** Principle 1 (user sovereignty/inspectable), Principle 2 (privacy
  defaults), D8 (autonomy dial), Principle 5 (transparency). **Builds on:** ADR-0003.

## Context

NOVA needs configuration for: privacy/egress defaults, the autonomy dial (D8), model/
runtime selection, resource budgets per device tier (Ch16), feature toggles, and plugin
permissions. Configuration is **security-relevant** (it governs egress and consent) and
must default to the private/conservative end (Principles 2, 6) and be **inspectable and
correctable** by the user (Principle 1).

## Options Considered

1. **Ad-hoc scattered settings.** Rejected: unauditable, inconsistent defaults — unsafe
   for a privacy product.
2. **Centralized, layered, typed configuration** managed by the kernel: schema-defined
   defaults → user overrides → per-device/runtime overrides, with validation.
3. **Remote/managed configuration.** Rejected outright: would imply egress and external
   control over a single-user private device (violates D1/D2/D3).

## Chosen Solution

**A centralized, layered, typed configuration service in the kernel (Option 2).**

- **Layering (highest precedence last):** (1) secure built-in **defaults** that are
  private-and-conservative by construction (Principles 2, 6); (2) **user settings**
  (sovereign overrides — Principle 1); (3) **device/runtime** adjustments (e.g. resource
  budgets per tier). No remote layer.
- **Typed & validated:** configuration has a defined schema; invalid values are rejected
  with clear errors (feeds ADR-0010). Security-relevant keys (egress, autonomy dial,
  plugin permissions) are explicitly marked and audited.
- **Inspectable & correctable:** every setting is visible and editable by the user
  (Principle 1); changes to security-relevant settings are recorded in the activity
  trail (Principle 5).
- **Local & private:** configuration lives in the encrypted local store (ADR-0006);
  secrets never sit in plaintext or in the repo (SECURITY.md, .gitignore).

## Trade-offs

- **(-) Upfront schema/validation effort.** *Accepted:* configuration governs egress and
  consent; it must be rigorous, not ad-hoc.
- **(-) Layering complexity.** *Mitigated* by a single kernel service as the one source
  of resolved configuration.
- **(+) Safe-by-default** guarantees the privacy/conservative posture ships to every
  user (audit §7 rationale; Principle 2).
- **(+) Auditable** security-relevant changes support the trust KPI (Principle 5).

## Consequences

- The Step-3 "Configuration System" spec details the schema shape, layering, validation,
  and the security-relevant-key audit rule.
- The autonomy dial (D8/OQ-5) and egress defaults (D3) are configuration-owned and must
  default conservative.
- Plugin permissions (ADR-0012) are expressed through this system.
