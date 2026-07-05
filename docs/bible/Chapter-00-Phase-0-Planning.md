---
document: NOVA Bible
chapter: 0
title: Phase 0 — Planning, Standards & Governance
status: DRAFT
version: 1.0.0
last_updated: 2026-07-04
authority: GOVERNANCE — defines how the Bible is written; subordinate to Chapter 1 on matters of product principle
---

# PHASE 0 — PLANNING, STANDARDS & GOVERNANCE

> **Authority note.** Phase 0 governs *how the NOVA Bible is written and maintained*.
> It is subordinate to Chapter 1 on any matter of product principle. Where a
> writing-standard here would ever conflict with a Chapter 1 principle, Chapter 1
> wins. Phase 0 may be amended more freely than Chapter 1, but always via version bump.

---

## 0.1 Complete Table of Contents

The NOVA Bible consists of a governance preamble (this Phase 0) followed by twenty
chapters, followed by four closing reviews. Chapter titles are frozen; only their
*contents* evolve under versioning.

- **Phase 0 — Planning, Standards & Governance** (this document)
- **Chapter 1 — Product Vision & Philosophy** *(complete, v1.0.0)*
- **Chapter 2 — Product Goals & User Personas**
- **Chapter 3 — Functional Requirements Specification**
- **Chapter 4 — Non-Functional Requirements**
- **Chapter 5 — High-Level System Architecture**
- **Chapter 6 — Module Architecture**
- **Chapter 7 — Data Flow & Event Flow**
- **Chapter 8 — Memory Engine**
- **Chapter 9 — Universal Search Engine**
- **Chapter 10 — Voice & Wake Word System**
- **Chapter 11 — AI Engine**
- **Chapter 12 — Device Communication**
- **Chapter 13 — Plugin System**
- **Chapter 14 — Database Architecture**
- **Chapter 15 — Security & Privacy**
- **Chapter 16 — Performance & Scalability**
- **Chapter 17 — UI/UX Design System**
- **Chapter 18 — Development Standards**
- **Chapter 19 — Testing, QA & Release Strategy**
- **Chapter 20 — Future Roadmap**
- **Closing A — Final Consistency Review**
- **Closing B — Cross-Chapter Dependency Review**
- **Closing C — Architecture Validation Checklist**
- **Closing D — Implementation Readiness Report**

---

## 0.2 Chapter Dependency Graph

Each chapter *consumes* decisions from earlier chapters and *produces* constraints
for later ones. The graph below records the primary directed dependencies (A → B
means "B depends on A"). This graph is the tool used to (a) decide the writing
order, and (b) detect, in the Closing reviews, whether any later chapter silently
contradicted an upstream decision.

```
Ch1 Vision/Philosophy
  └─→ Ch2 Goals/Personas
        └─→ Ch3 Functional Requirements
              └─→ Ch4 Non-Functional Requirements
                    └─→ Ch5 High-Level Architecture
                          ├─→ Ch6 Module Architecture
                          │     ├─→ Ch7 Data/Event Flow
                          │     ├─→ Ch8 Memory Engine
                          │     ├─→ Ch9 Universal Search
                          │     ├─→ Ch10 Voice/Wake Word
                          │     ├─→ Ch11 AI Engine
                          │     ├─→ Ch12 Device Communication
                          │     └─→ Ch13 Plugin System
                          └─→ Ch14 Database Architecture
Ch1 ─(privacy/sovereignty)─→ Ch15 Security & Privacy ──→ (constrains Ch8,11,12,13,14)
Ch4 NFRs ─→ Ch16 Performance & Scalability ─→ (constrains Ch8,9,10,11)
Ch2 Personas ─→ Ch17 UI/UX ─→ (consumes Ch6,7,10)
Ch5/Ch6 ─→ Ch18 Development Standards ─→ (governs Ch19)
Ch3/Ch4/Ch18 ─→ Ch19 Testing/QA/Release
All chapters ─→ Ch20 Future Roadmap
All chapters ─→ Closing A/B/C/D
```

**Cross-cutting chapters.** Chapters 15 (Security & Privacy) and 16 (Performance &
Scalability) are *cross-cutting*: they both depend on and constrain many other
chapters. They are written after the subsystems they constrain are outlined (so
they have concrete surfaces to secure and to bound), but their *requirements*
originate in Chapters 1 and 4 respectively.

**Foundational spine.** Chapters 1 → 2 → 3 → 4 → 5 form the non-negotiable spine.
Nothing downstream may contradict the spine without a versioned amendment to the
relevant spine chapter.

---

## 0.3 Documentation Strategy

1. **One coherent architecture, start to finish.** The Bible describes exactly one
   NOVA. Where a chapter presents alternatives, exactly one is chosen as canonical
   and all later chapters build on the canonical choice. Alternatives are recorded
   for context, never left "open" in a way that forks the architecture.
2. **Decisions are first-class objects.** Every material decision gets an ID
   (Chapter 1 uses D1–D8; later chapters continue with chapter-scoped IDs, e.g.
   D5.3 = the third decision in Chapter 5). Decisions are referenced by ID across
   chapters so dependencies are traceable.
3. **Traceability.** Every functional requirement (Ch3) traces up to a goal (Ch2)
   and a principle (Ch1), and traces down to the module(s) that satisfy it (Ch6+).
   The Closing reviews verify this chain is unbroken.
4. **No implementation.** The Bible stops at architecture, specification, and
   engineering decisions. It never contains source code, concrete API signatures,
   or implementation-level schema. It describes *what* and *why*, and *shapes* of
   *how*, but not literal *how*.
5. **Written for a future team.** The audience is an engineering team that did not
   attend any meeting. Ambiguity is a defect. Where a reasonable engineer could
   pick two different paths, the Bible either decides or explicitly flags an Open
   Question with a stated default.
6. **Honest costs.** Every chapter states disadvantages and risks, not only
   advantages. A chapter with no stated downsides is presumed incomplete.

---

## 0.4 Writing Standards

- **Chapter template (mandatory sections, in order):** Purpose; Scope; Definitions;
  Engineering Rationale; Design Decisions; Alternatives Considered; Risks; Future
  Expansion; Open Questions; Recommendations. Chapters may add sections between
  these (e.g. Chapter 2 adds Personas, JTBD, Journeys) but may not omit a mandatory
  section.
- **Depth target:** 4,000–8,000 words per chapter where the material warrants it.
  Depth is never padded and never compressed to fit a response; a chapter that
  cannot fit continues in the next response from the exact stopping point.
- **Every decision carries reasoning.** No comparative claim ("X is better") appears
  without an explicit "because…". No decision appears without at least one
  Alternative Considered and the reason it was rejected.
- **Voice:** internal engineering handbook. Precise, plain, and complete. No
  marketing language except where explicitly quoting the product's own promises.
- **No forward contradiction.** A chapter may *refine* an earlier decision only by
  narrowing it within the earlier decision's stated envelope. Widening or reversing
  requires a versioned amendment to the earlier chapter.

---

## 0.5 Terminology Standards

- Terms fixed in **Appendix 1.B** (on-device-first, acceleration seam, owned memory,
  privileged egress, consequence/consent gate, vision amendment) carry their
  Chapter 1 meanings everywhere and are never redefined.
- Each chapter has a **Definitions** section for terms it introduces. A term is
  defined once, in the earliest chapter that needs it, and referenced thereafter.
- **Canonical spellings:** "NOVA" (all caps) for the product; "on-device-first"
  (hyphenated); "acceleration seam" (two words). Module names, once coined in
  Chapter 6, are capitalized consistently (e.g. Memory Engine, Universal Search,
  AI Engine, Consent Gate).
- **RFC-2119-style modal verbs.** MUST / MUST NOT = binding requirement; SHOULD /
  SHOULD NOT = strong recommendation with allowable, documented exceptions; MAY =
  optional. These meanings are fixed and used deliberately from Chapter 3 onward.

---

## 0.6 Versioning Rules

- **Semantic versioning per chapter.** `MAJOR.MINOR.PATCH`.
  - **MAJOR** — a decision changed in a way that can contradict downstream chapters
    (triggers a Cross-Chapter Dependency Review of dependents).
  - **MINOR** — new material added that does not contradict existing decisions.
  - **PATCH** — clarifications, typos, wording, non-semantic edits.
- **Amendments are additive and recorded.** A superseded decision is struck through
  or moved to an "Amended/Superseded" note, never silently deleted, so the rationale
  history survives (Principle 7, longevity).
- **The `supersedes` / `superseded_by` frontmatter fields** record chapter-level
  lineage. Decision-level changes are recorded in the chapter's own change log.
- **A MAJOR bump to a spine chapter (1–5) forces a re-validation pass** of every
  chapter that depends on it, per the dependency graph (0.2).

---

## 0.7 Assumptions (documentation-level)

These are assumptions about the *documentation project*, distinct from product
assumptions (which live in the relevant chapters).

- **A0-1.** Chapter 1's defaults for OQ-1 (single-user-first) and OQ-2 (concrete
  stack with alternatives) hold unless the reader amends them. All later chapters
  are written on these defaults; if either is later reversed, the affected chapters
  take a MAJOR bump.
- **A0-2.** The confirmed platform constraint is **Android + Windows first** (D6).
  All platform-specific reasoning targets these two; Linux/macOS appear only as
  future-expansion notes until Chapter 20.
- **A0-3.** The Bible is a *living* document set. It is expected to be revised as
  reality intrudes; the versioning rules exist precisely to make revision safe.
- **A0-4.** No external stakeholder sign-off is modeled inside the Bible. Where the
  real project would need legal, security, or executive review, the Bible flags it
  as an Open Question rather than inventing an approval.

---

## 0.8 Constraints (documentation-level)

- **C0-1.** No source code, no concrete API contracts, no implementation-level
  database schemas. Architecture and specification only.
- **C0-2.** No contradiction of Chapter 1's nine ordered principles anywhere. The
  principles are the top of the authority stack for the entire document.
- **C0-3.** Every chapter must be independently readable (self-contained Purpose,
  Scope, Definitions) yet globally consistent (no re-litigation of settled
  decisions).
- **C0-4.** Alternatives must be recorded, not merely asserted-away. A future team
  must be able to see what was considered and why it lost.
- **C0-5.** The document must remain vendor-independent at the level of *hard
  dependency*: it may recommend concrete technologies (per D5) but must not bind
  NOVA's survival to any single external provider (Principle 7).

*End of Phase 0.*
