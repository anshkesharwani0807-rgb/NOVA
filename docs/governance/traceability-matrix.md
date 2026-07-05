# NOVA Requirements Traceability Matrix

Mandated by Phase 0 section 0.3: every requirement must trace up to a goal and a
principle, and down to the module(s) that satisfy it. This matrix is seeded from
Chapters 1-2 and will be extended as Chapters 3+ define formal requirements.

## Columns

- **Principle** (Ch1) -> **Goal** (Ch2) -> **JTBD** (Ch2) -> **Requirement** (Ch3, TBD)
  -> **Module** (Ch6, provisional) -> **Verification** (Ch19, TBD)

## Seed rows (principle -> goal -> JTBD; downstream columns pending)

| Principle | Goal | JTBD | Requirement (Ch3) | Module (Ch6) | Verification (Ch19) |
|---|---|---|---|---|---|
| P3 On-device first | PG-1 Offline usefulness | JTBD-9 Be there offline | TBD | Core / all | TBD |
| P4 Memory sacred | PG-2 Accurate owned memory | JTBD-2 Remember for me | TBD | Memory Engine | TBD |
| P1/P2/P5 | PG-3 Trust measured | JTBD-5 Keep private | TBD | Consent Gate / Egress | TBD |
| AI vision clause | PG-4 Fast natural interaction | JTBD-1 Find what I forgot | TBD | Voice / AI Engine / Search | TBD |
| P6 Agency w/ consent | PG-5 Consented automation | JTBD-3 Automate, no surprises | TBD | Consent Gate / AI Engine | TBD |
| P7 Longevity | SG-1 Cross-device continuity | JTBD-4 One system | TBD | Device Communication | TBD |
| Fragmentation problem | SG-2 Universal search | JTBD-1 Find what I forgot | TBD | Universal Search | TBD |
| P5 Transparency | SG-3 Legible behavior | JTBD-7 Tell me why | TBD | (cross-cutting) | TBD |
| Multimodal need | SG-4 Multimodal input | JTBD-6 Understand my materials | TBD | AI Engine (multimodal) | TBD |
| P7 Longevity | LG-1 Compounds over years | JTBD-8 Grow with me | TBD | Memory / DB | TBD |

> **How to maintain:** as Chapter 3 assigns requirement IDs, fill the Requirement
> column; as Chapter 6 ratifies modules, fill the Module column; as Chapter 19 defines
> tests, fill Verification. A requirement with no upstream principle/goal is out of
> scope by construction and must be removed or re-derived.
