# NOVA Glossary

Canonical definitions of NOVA terms. Terms are defined once in the earliest chapter
that needs them and aggregated here. Seeded from Appendix 1.B and Chapter 2.

## Vision-critical terms (fixed by Chapter 1, Appendix 1.B)

- **On-device-first** — the property that NOVA's core intelligence, memory, and primary
  functionality run on the user's device and remain fully useful without a network.
- **Acceleration seam** — the clean, consent-gated interface through which optional
  remote compute may enhance, never replace, the local core.
- **Owned memory** — memory that is durable, inspectable, correctable, portable, and
  encrypted, over which the user has full sovereignty.
- **Privileged egress** — any network operation that sends data off-device, treated as
  a single, logged, consent-gated chokepoint.
- **Consequence/Consent gate** — the mechanism that classifies an action by stakes and
  reversibility and decides whether it may proceed autonomously or requires confirmation.
- **Vision amendment** — a deliberate, versioned change to Chapter 1, required before any
  downstream decision may contradict a non-negotiable set there.

## Product terms (Chapter 2)

- **Goal tier** — the priority band a goal sits in (Primary/Secondary/Long-Term/Stretch).
- **KPI** — a Key Performance Indicator; a measurable quantity indicating whether a goal
  is met, with a direction of good and, where possible, a target band.
- **Persona** — a concrete, named archetype of a real user.
- **JTBD (Job To Be Done)** — a functional or emotional job a user is trying to get done,
  phrased from the user's point of view, independent of any feature.
- **Daily usefulness** — the property that a typical target user derives real value from
  NOVA on a typical day, offline-capable, without novelty wearing off.

## Governance terms (Phase 0)

- **Decision ID** — identifier for a first-class decision (D<n>, or D<chapter>.<n>).
- **Spine chapters** — Chapters 1-5; the non-negotiable foundation.
- **Cross-cutting chapters** — Chapters 15 and 16; depend on and constrain many others.
- **RFC-2119 modal verbs** — MUST / MUST NOT (binding), SHOULD / SHOULD NOT (strong
  recommendation with documented exceptions), MAY (optional).

> **How to maintain:** when a chapter introduces a term in its Definitions section, add
> it here under the owning chapter. Never redefine a term already fixed above.
