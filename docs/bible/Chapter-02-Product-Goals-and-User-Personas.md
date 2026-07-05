---
document: NOVA Bible
chapter: 2
title: Product Goals & User Personas
status: DRAFT
version: 1.0.0
last_updated: 2026-07-04
depends_on: [Chapter 1 v1.0.0, Phase 0 v1.0.0]
authority: Translates Chapter 1 vision into measurable goals and target users; subordinate to Chapter 1
---

# CHAPTER 2 — PRODUCT GOALS & USER PERSONAS

> **Conformance note.** This chapter translates the Chapter 1 vision into measurable
> goals and defines exactly who NOVA is for. It introduces **no** architecture and
> **no** implementation. Every goal, metric, and persona below is checked against
> Chapter 1's nine ordered principles and decisions D1–D8; nothing here may
> contradict them. Where a goal would strain a principle, the principle wins and the
> goal is re-scoped.

---

## 2.0 Purpose

The purpose of this chapter is to convert the philosophy of Chapter 1 — which is
deliberately abstract and value-level — into **concrete, measurable product
direction**: a ranked set of goals with success criteria, a set of key performance
indicators (KPIs) that make "are we succeeding?" answerable with evidence, and a
precise definition of the humans NOVA is being built for.

A vision without goals is a mood; goals without metrics are wishes; metrics without
personas measure the wrong things. This chapter supplies all three so that a future
engineering and product team can prioritize work, resolve trade-offs, and know when
they have actually built the right product — not merely a working one.

Critically, this chapter is where "a private, personal, on-device, memory-centric,
agentic companion" (Chapter 1) becomes a checklist a team can build against:
*what must be true, for whom, and how will we know.*

---

## 2.1 Scope

### 2.1.1 In scope

- Product goals, tiered into Primary, Secondary, Long-Term, and Stretch (5–10 year),
  each with description, reason for existence, user value, engineering impact,
  success criteria, and risks.
- Product success metrics (KPIs) with the rationale for why each matters.
- Eight complete target-user personas.
- Jobs To Be Done (JTBD) — the real-world jobs users hire NOVA to accomplish.
- User journeys across daily and situational contexts.
- User frustrations with existing tools that NOVA must resolve.
- Product boundaries (what NOVA should and should never do), consistent with the
  Chapter 1 anti-scope.
- Future personas (post-v5.0) and product-level risks and open questions.

### 2.1.2 Out of scope

- Architecture, modules, data flow, and any implementation (Chapters 5+). Where a
  persona or goal *implies* a capability, this chapter names the capability at the
  product level (e.g. "universal search") and defers its design to the owning
  chapter.
- Functional and non-functional requirements as formal specifications (Chapters 3–4).
  This chapter produces the *goals* those requirements will be derived from.
- Business-model specifics beyond the Chapter 1 prohibitions (tracked as OQ-3).

### 2.1.3 Definitions introduced here

- **Goal tier:** the priority band a goal sits in (Primary/Secondary/Long-Term/
  Stretch). Tier determines what gets built first and what may be cut under pressure.
- **KPI:** a Key Performance Indicator — a measurable quantity whose value tells us
  whether a goal is being met. Each KPI has a direction of "good" and, where
  possible, a target band.
- **Persona:** a concrete, named archetype of a real user, used to keep design
  decisions anchored to human needs rather than abstract "users."
- **JTBD (Job To Be Done):** a functional or emotional job a user is trying to get
  done, phrased from the user's point of view, independent of any feature.
- **Daily usefulness:** the property that a typical target user derives real value
  from NOVA on a typical day, offline-capable, without novelty wearing off. This is
  the single most important product outcome and recurs throughout the chapter.

---

## 2.2 Relationship with Chapter 1

Every goal in this chapter originates from a Chapter 1 principle or decision. This
section makes the derivation explicit so the traceability chain (Phase 0, §0.3) is
unbroken from principle → goal.

- **From Principle 1 (user sovereignty)** come the goals of *full user control*,
  *inspectable memory*, and *export/ownership* — NOVA must be demonstrably the
  user's, not the platform's.
- **From Principle 2 (privacy by default) and D3 (privileged egress)** come the goals
  of *offline-capable core*, *local-by-default data*, and *trust as a measured
  outcome* — privacy is not a feature but a baseline the metrics must confirm.
- **From Principle 3 (on-device first) and D1** comes the goal of *genuine daily
  usefulness with zero connectivity* and the KPI for offline capability.
- **From Principle 4 (memory is sacred) and D7** come the goals around *memory
  accuracy, recall, correction, and longevity* — the differentiator gets its own
  goals and metrics.
- **From Principle 5 (transparency)** comes the goal of *legible behavior* — users
  can always ask "why did you do that?" and get a real answer.
- **From Principle 6 (agency with consent) and D8** come the goals of *useful
  automation* bounded by *consent gating*, and the metric distinguishing helpful
  autonomy from overreach.
- **From Principle 7 (longevity/ownership)** come the goals of *multi-year retention*,
  *migration*, and *cross-device continuity* — NOVA must be keep-able.
- **From Principle 8 (coherence)** comes the discipline that *every goal must
  strengthen the core identity*; goals that merely add surface area are demoted or
  cut.
- **From Principle 9 (honesty about limits)** comes the goal that NOVA *calibrates
  and communicates uncertainty*, and the anti-goal of manufactured confidence.
- **From D2 (single-user) and D6 (Android+Windows first)** come the scoping of every
  persona and journey to a single user operating across Android and Windows devices.

If, at any point below, a stated goal cannot be traced to one of these, that goal is
out of scope by construction and must be removed or re-derived.

---

## 2.3 Product Goals

Goals are tiered. **Tier determines build order and what may be sacrificed under
pressure:** Primary goals are non-negotiable for a credible v1; Secondary goals make
v1 excellent; Long-Term goals define the mature product; Stretch goals (5–10 years)
are directional bets that must not distort near-term decisions.

Each goal follows the mandated structure: Description, Why it exists, Expected user
value, Engineering impact, Success criteria, Risks.

### 2.3.1 Primary Goals (non-negotiable for v1)

#### PG-1 — Genuine daily usefulness, fully offline

- **Description.** A typical target user derives real, repeated value from NOVA on a
  typical day *without any network connection* — capturing, finding, organizing,
  reminding, and answering over their own data and context.
- **Why it exists.** Directly realizes Principle 3 and D1. If NOVA is only useful
  online, it is a thin client and the entire vision collapses. Offline usefulness is
  the proof that the local core is real.
- **Expected user value.** Reliability and independence: NOVA works on a plane, in a
  dead zone, during an outage, with instant response and no privacy exposure.
- **Engineering impact.** Forces a capable local core (models, memory, search) and a
  strict discipline that no core path requires egress. Shapes Chapters 5, 8, 9, 11,
  16 heavily.
- **Success criteria.** In offline mode, the target personas can complete their top
  three JTBD (§2.6) without functional loss beyond clearly-communicated,
  acceleration-only degradations; measured offline task-completion rate ≥ the online
  rate minus a small, documented delta.
- **Risks.** Local capability gap (R1 from Ch1); offline correctness is hard;
  temptation to quietly require the cloud for "just this one" feature (R2).

#### PG-2 — Memory that is accurate, recallable, and owned

- **Description.** NOVA durably remembers what matters about the user and can recall
  it accurately when relevant, while the user can inspect, correct, and delete any
  memory and export all of it.
- **Why it exists.** Realizes Principle 4 and D7; memory is the differentiator
  (Ch1 §1.3.2). Without trustworthy memory, NOVA is just another stateless assistant.
- **Expected user value.** Continuity — NOVA gets more useful over time, stops making
  the user repeat themselves, and never becomes an unaccountable dossier.
- **Engineering impact.** Requires the first-class Memory Engine (Ch8): durable,
  encrypted, inspectable, correctable, portable, versioned. Constrains Ch7, 14, 15.
- **Success criteria.** Measured memory recall precision/recall above target bands
  (§2.4); user correction of a memory is honored and propagated; full export produces
  a complete, re-importable archive; zero silent memory use (every use is
  explainable).
- **Risks.** False memories / wrong recall erode trust fast (R7); over-remembering
  drifts toward surveillance (violates "what matters," OQ-4); encryption + inspection
  are in tension and must be reconciled (Ch15).

#### PG-3 — Trust as an engineered, measured outcome

- **Description.** Users can verify that NOVA behaves as promised: data stays local by
  default, egress is visible and consented, and every action/inference is explainable.
- **Why it exists.** Realizes Principles 1, 2, 5. Trust is the entire relationship
  (Ch1 §1.2.3); it must be *demonstrable*, not asserted.
- **Expected user value.** Confidence to entrust NOVA with intimate data because the
  user can *see* what it does with it.
- **Engineering impact.** Requires the privileged-egress chokepoint (D3), provenance
  on actions/inferences, an inspectable activity trail, and consent flows. Constrains
  Ch7, 11, 13, 15.
- **Success criteria.** 100% of egress events are logged and attributable; users can,
  for any NOVA action, retrieve a plain-language "why"; measured user-reported trust
  (§2.4) above target and rising with tenure.
- **Risks.** Transparency features that themselves leak data (must be local, R2);
  "explanation theater" that looks legible but isn't faithful to actual behavior
  (Principle 5 requires faithful provenance, not plausible-sounding rationalizations).

#### PG-4 — Fast, natural interaction (voice + text)

- **Description.** NOVA responds quickly and understands natural language and voice,
  so interacting feels like talking to a capable assistant, not operating software.
- **Why it exists.** Realizes the "artificial intelligence" and "acts on your behalf"
  clauses of the vision; low latency is a core felt benefit of on-device (PG-1).
- **Expected user value.** Frictionless capture and retrieval; the assistant fits into
  the flow of life rather than interrupting it.
- **Engineering impact.** Requires local ASR/TTS and NLU (Ch10, 11) meeting latency
  NFRs (Ch4, 16). Constrains resource budgets on device.
- **Success criteria.** Voice-response and search latencies within target bands
  (§2.4) on the minimum supported hardware tier; natural-language intent success rate
  above target.
- **Risks.** Latency vs. capability trade-off on low-end devices (R4); voice accuracy
  across accents/noise; always-listening privacy concerns (Ch10, Ch15).

#### PG-5 — Useful, consented automation (agency without overreach)

- **Description.** NOVA takes real actions on the user's behalf, autonomously for
  low-stakes/reversible tasks and with confirmation for high-stakes/irreversible ones.
- **Why it exists.** Realizes Principle 6 and D8; agency is what separates NOVA from
  an oracle (Ch1 §1.7.7).
- **Expected user value.** Time saved and cognitive load reduced, without the fear
  that NOVA will do something irreversible and wrong.
- **Engineering impact.** Requires the Consequence/Consent Gate (Ch6, 11, 15) that
  classifies stakes and reversibility; autonomy is a user-configurable dial defaulting
  conservative.
- **Success criteria.** Measured "helpful automation rate" high and "unwanted action
  rate" near zero (§2.4); no irreversible action ever taken without consent; autonomy
  earned over time per user.
- **Risks.** A single bad autonomous action can destroy trust (R7); mis-classifying
  stakes; users over- or under-trusting the dial.

### 2.3.2 Secondary Goals (make v1 excellent)

#### SG-1 — Cross-device continuity for the same user

- **Description.** The same user's NOVA feels like one system across their Android and
  Windows devices — capture on one, recall on the other — over the multi-device seam
  (D2), without introducing multi-*user* complexity.
- **Why it exists.** Realizes Principle 7 (continuity) and the D2 seam; addresses the
  fragmentation problem (Ch1 §1.3.1) at the device level.
- **Expected user value.** "Every device feels like one system" (a core JTBD, §2.6).
- **Engineering impact.** Requires consent-gated, privacy-preserving sync (Ch12, 14,
  15) that does not violate local-by-default. Explicitly a *seam*, not a cloud hub.
- **Success criteria.** A memory/item created on device A is retrievable on device B
  within a target sync window when the user has enabled sync; sync is end-to-end
  protected; disabling sync fully isolates devices.
- **Risks.** Sync is a classic privacy/consistency minefield; conflict resolution;
  the temptation to become cloud-first through the sync backdoor (R2).

#### SG-2 — Universal search over the user's world

- **Description.** One search surface that finds anything the user has entrusted to
  NOVA — notes, files, memories, messages the user has shared with it, media — by
  meaning, not just keyword.
- **Why it exists.** Directly attacks fragmentation (Ch1 §1.3.1) and the "I need to
  find something I forgot" JTBD.
- **Expected user value.** The end of re-finding; one place to ask "where is…".
- **Engineering impact.** Requires the Universal Search engine (Ch9) with local
  semantic + lexical retrieval over heterogeneous local data. Constrains Ch8, 14, 16.
- **Success criteria.** Search precision/recall and latency within target bands
  (§2.4) offline; users report they "usually find it in NOVA."
- **Risks.** Indexing cost on device (battery/storage, R4); semantic search quality
  with small local models; scope creep into content NOVA shouldn't hold.

#### SG-3 — Legible, correctable behavior everywhere

- **Description.** Beyond PG-3's baseline, every surface exposes "why" and "edit/undo"
  affordances, so the user is never confused about what NOVA knows or did.
- **Why it exists.** Deepens Principles 1 and 5 into the everyday UX.
- **Expected user value.** A sense of control that makes intimate data-sharing feel
  safe.
- **Engineering impact.** UX and provenance requirements across Ch7, 17.
- **Success criteria.** Every action is undoable or explicitly marked irreversible
  before it happens; every memory is one tap from inspect/correct/delete.
- **Risks.** Explanation overhead cluttering the UX; performance cost of provenance.

#### SG-4 — Multimodal input beyond text/voice

- **Description.** NOVA can accept images, documents, and files as inputs to
  understand, organize, and retrieve — within privacy constraints.
- **Why it exists.** Real digital lives are multimodal; supports personas like the
  Photographer, Researcher, and Content Creator (§2.5).
- **Expected user value.** NOVA understands the user's actual materials, not just
  typed text.
- **Engineering impact.** Local multimodal understanding (Ch11) within device budgets
  (Ch16); careful egress rules for heavy media (D3).
- **Success criteria.** Users can add an image/document and later find or ask about it
  offline; media never leaves the device without consent.
- **Risks.** Heavy compute/storage (R4); model quality on device; privacy of sensitive
  media.

### 2.3.3 Long-Term Goals (the mature product)

#### LG-1 — A companion that compounds over years

- **Description.** NOVA becomes materially more valuable the longer it is used, as
  owned memory and personalization deepen — across device generations and OS changes.
- **Why it exists.** Realizes the "lifelong companion" clause and Principle 7.
- **Expected user value.** A relationship that pays compounding dividends and can be
  kept for a decade.
- **Engineering impact.** Long-horizon data durability, migration, and versioning
  (Ch14, 18, 20); backward compatibility as a standing constraint.
- **Success criteria.** Multi-year retention above target (§2.4); successful data
  migration across major device/OS transitions with zero memory loss.
- **Risks.** Format rot; migration complexity; multi-year backward-compat burden.

#### LG-2 — Proactive helpfulness (anticipation)

- **Description.** NOVA anticipates needs — surfacing the right memory, reminder, or
  action at the right moment — always within consent and transparency limits.
- **Why it exists.** Extends agency (Principle 6) from reactive to proactive, a
  natural maturity of the companion role.
- **Expected user value.** NOVA helps *before* being asked, reducing cognitive load.
- **Engineering impact.** Context engine + prediction (Ch7, 11) gated by consent
  (D8); proactivity must never become nagging or surveillance.
- **Success criteria.** Measured "helpful proactive suggestion rate" positive and
  "interruption annoyance" low (§2.4).
- **Risks.** Proactivity is the easiest place to violate consent/transparency (R7);
  the line between helpful and creepy is thin and personal.

#### LG-3 — Platform breadth (Linux, macOS)

- **Description.** Extend beyond Android + Windows to Linux and macOS, preserving the
  single-user, on-device-first, portable core.
- **Why it exists.** Serves users with mixed device ecosystems; realizes portability
  designed into the core (D6 defers, does not forbid).
- **Expected user value.** True cross-ecosystem continuity for the same user.
- **Engineering impact.** Portability work (Ch5, 12, 20); expanded device-diversity
  support (R4).
- **Success criteria.** Feature and continuity parity for the same user across all
  four platforms at target quality.
- **Risks.** Support-matrix explosion; resource dilution from breadth (Principle 8
  tension).

### 2.3.4 Stretch Goals (5–10 years; directional bets)

> Stretch goals set direction. They **must not** distort near-term prioritization,
> and each is constrained by the Chapter 1 principles exactly as strongly as any
> other goal. A stretch goal that would require weakening a principle is not a
> stretch goal; it is out of scope.

#### XG-1 — Fully local, near-frontier reasoning

- **Description.** As on-device models improve, close the capability gap so NOVA's
  local reasoning approaches frontier quality without the acceleration seam.
- **Why it exists.** Would make PG-1 (offline usefulness) dominant and R1
  (capability gap) obsolete.
- **Expected user value.** Frontier intelligence with zero privacy trade-off.
- **Engineering impact.** Model-management and hardware-acceleration investment
  (Ch11, 16); depends heavily on external model/hardware progress.
- **Success criteria.** Local-only task quality within a small delta of the best
  available acceleration path.
- **Risks.** Depends on exogenous progress; hardware diversity; energy/thermal limits.

#### XG-2 — Deep, consented life-automation

- **Description.** NOVA orchestrates larger multi-step real-world workflows on the
  user's behalf across their tools, always under the consent gate.
- **Why it exists.** The mature expression of "acts on your behalf."
- **Expected user value.** Meaningful reclamation of time on repetitive life-admin.
- **Engineering impact.** Robust workflow orchestration + plugin ecosystem (Ch13)
  under strict egress and consent policy.
- **Success criteria.** Multi-step workflows completed reliably with correct
  consent gating and full reversibility/auditing where possible.
- **Risks.** Every added integration widens attack surface (Ch15); overreach (R7).

#### XG-3 — User-portable "AI self" across hardware generations

- **Description.** The user can move their entire NOVA — memory, personalization,
  settings — to new hardware seamlessly, treating NOVA as a truly owned, portable
  possession over a lifetime.
- **Why it exists.** The fullest expression of Principle 7 (ownership/longevity).
- **Expected user value.** NOVA as a lifelong, inheritable digital companion the user
  genuinely owns.
- **Engineering impact.** Rock-solid export/import, encryption key custody, and
  migration (Ch14, 15, 20).
- **Success criteria.** Complete, verifiable migration to new hardware with zero loss
  and preserved privacy.
- **Risks.** Key management and recovery (R5); long-horizon format stability.

---

## 2.4 Product Success Metrics (KPIs)

KPIs make the goals falsifiable. Each KPI states what it measures, its "good"
direction, an indicative target band (to be finalized against real hardware in
Chapters 4/16), and **why it matters** — its tie to a goal and principle. Targets are
indicative product intent, not committed engineering specs; Chapters 4 and 16 own the
committed numbers.

| KPI | What it measures | Good direction | Indicative target | Why it matters |
|---|---|---|---|---|
| **Search latency** | Time from query to first useful result, offline, on min hardware | Lower | Sub-second for typical local queries | Realizes PG-4/SG-2; slow search kills daily usefulness (PG-1). |
| **Memory recall accuracy** | Precision & recall of relevant memories when they matter | Higher | High precision prioritized over recall | Wrong memories destroy trust faster than missing ones (PG-2, R7). |
| **Voice response time** | Time from end-of-speech to NOVA's response onset, offline | Lower | Perceptibly immediate on min hardware | Latency is the felt advantage of on-device (PG-4, Principle 3). |
| **User trust (measured)** | Self-reported trust + behavioral proxies (data entrusted over time) | Higher, rising with tenure | Rising trend per user | Trust is the whole relationship (PG-3, Principle 1/2/5). |
| **Offline capability** | Fraction of top JTBD completable with zero connectivity | Higher | Near-parity with online, minus documented deltas | Direct proof of D1/Principle 3 (PG-1). |
| **Battery / energy usage** | Energy cost of NOVA's background + active work | Lower | Within a strict daily budget per tier | An assistant that drains the battery gets uninstalled (R4, Ch16). |
| **Daily usefulness** | Fraction of target users deriving real value on a typical day | Higher | Majority of active users, sustained | The single most important outcome (§2.1.3); novelty must not decay. |
| **Long-term retention** | Users still actively using NOVA after 1/2/5 years | Higher | Strong multi-year retention | Realizes "lifelong companion" (LG-1, Principle 7). |
| **Cross-device experience** | Continuity quality for the same user across devices | Higher | Fast, correct, private sync when enabled | Realizes SG-1; attacks fragmentation. |
| **Unwanted-action rate** | Autonomous actions the user did not want / had to undo | Lower | Near zero, especially irreversible | Guards Principle 6/D8; one bad action erodes trust (PG-5, R7). |
| **Explainability coverage** | Fraction of actions/inferences with a faithful "why" | Higher | 100% of material actions | Realizes Principle 5/PG-3; explanations must be faithful, not theater. |
| **Egress transparency** | Fraction of network egress events logged & attributable | Higher | 100% | Enforces D3; the technical substrate of the privacy promise. |
| **Correction honoring** | Fraction of user memory corrections correctly applied | Higher | 100% | Realizes Principle 1 (sovereignty over memory), PG-2. |

**Why these and not vanity metrics.** Deliberately absent are engagement-maximizing
metrics (time-on-app, session count, notification click-through). Per Principle 8 and
the anti-scope (Ch1 §1.5.2), NOVA is not an engagement machine; optimizing those
metrics would corrupt the product. NOVA measures *usefulness, trust, privacy,
accuracy, and retention-through-value* — the outcomes that mean the vision is real.

---

## 2.5 Target Users (Personas)

Each persona is a single user (per D2) operating primarily across Android and Windows
(per D6). Personas keep the team anchored to real humans. Each includes: Background,
Daily workflow, Pain points, Digital habits, Devices, Technical knowledge, Privacy
expectations, Why NOVA helps, and Most-valuable NOVA modules (module names anticipate
Chapter 6 and are used here only as product-level labels).

### 2.5.1 Persona 1 — The Developer ("Arjun")

- **Background.** Mid-career software engineer; juggles multiple repos, tickets,
  docs, and side projects.
- **Daily workflow.** Context-switches constantly between code, terminals, tickets,
  design docs, and chats; captures snippets, TODOs, and links all day; loses track of
  "where did I put that command / note / decision."
- **Pain points.** Fragmented notes; forgotten context on returning to a project;
  re-finding a command or config; scattered bookmarks.
- **Digital habits.** Heavy keyboard user; lives in editor + terminal + browser;
  keeps many notes but poorly organized.
- **Devices.** Windows workstation (primary), Android phone (secondary).
- **Technical knowledge.** Expert.
- **Privacy expectations.** High and informed; wary of cloud tools ingesting code and
  proprietary context; values local-first strongly.
- **Why NOVA helps.** Local, private capture and semantic recall of technical
  context; "find that thing I noted" offline; consented automation of repetitive
  admin; trusts NOVA precisely because it's on-device.
- **Most-valuable modules.** Universal Search (Ch9), Memory Engine (Ch8), Plugin
  System (Ch13), AI Engine (Ch11).

### 2.5.2 Persona 2 — The Student ("Meera")

- **Background.** University student across several courses; lectures, readings,
  assignments, group projects.
- **Daily workflow.** Takes notes across apps and paper photos; collects PDFs and
  slides; scrambles before deadlines to find "that one thing the professor said."
- **Pain points.** Information scattered across apps and formats; hard to find past
  notes; deadline tracking; re-reading to locate a fact.
- **Digital habits.** Phone-first; screenshots and photos of boards/slides; mixes
  handwritten and digital notes.
- **Devices.** Android phone (primary), a Windows laptop (secondary).
- **Technical knowledge.** Moderate.
- **Privacy expectations.** Moderate but increasingly aware; dislikes the idea of
  personal study data being mined.
- **Why NOVA helps.** Captures notes/photos/PDFs and makes them findable by meaning
  offline; reminders for deadlines with consented automation; recalls "what did the
  lecture say about X."
- **Most-valuable modules.** Universal Search, Memory Engine, Voice (Ch10),
  Multimodal input (Ch11).

### 2.5.3 Persona 3 — The Content Creator ("Kabir")

- **Background.** Independent creator (video/writing/social); produces regularly and
  manages ideas, drafts, assets, and schedules.
- **Daily workflow.** Captures ideas on the go (voice + text); collects reference
  media; drafts and iterates; juggles a content calendar.
- **Pain points.** Ideas lost before capture; assets scattered; re-finding a past
  idea/reference; repetitive posting/admin.
- **Digital habits.** Voice notes constantly; large media libraries; multiple
  devices; always mid-project.
- **Devices.** Android phone (capture), Windows PC (production).
- **Technical knowledge.** Moderate–high for their tools.
- **Privacy expectations.** Moderate; protective of unreleased work and ideas.
- **Why NOVA helps.** Instant private voice capture; semantic recall of ideas/assets;
  consented automation of repetitive posting/admin; cross-device continuity from
  phone capture to PC production.
- **Most-valuable modules.** Voice, Memory Engine, Universal Search, Device
  Communication (Ch12), Plugin System.

### 2.5.4 Persona 4 — The Business Professional ("Sara")

- **Background.** Manager/consultant; back-to-back meetings, decisions, follow-ups,
  and travel.
- **Daily workflow.** Captures action items and decisions; needs the right context
  before each meeting; tracks follow-ups; lives in calendar + email + docs.
- **Pain points.** Losing action items; walking into meetings without context;
  follow-up slippage; information across many tools.
- **Digital habits.** Calendar-driven; email-heavy; mixes phone and laptop through
  the day; frequently traveling/offline.
- **Devices.** Windows laptop (primary), Android phone (constant companion).
- **Privacy expectations.** High; handles sensitive business information; strong
  aversion to cloud tools ingesting confidential context.
- **Technical knowledge.** Moderate.
- **Why NOVA helps.** Private, offline capture and recall of decisions and action
  items; pre-meeting context assembled locally; consented follow-up automation;
  trustworthy because sensitive data stays on device.
- **Most-valuable modules.** Memory Engine, Universal Search, AI Engine, Consent Gate
  (Ch6/15), Device Communication.

### 2.5.5 Persona 5 — The Researcher ("Dr. Nadia")

- **Background.** Academic/industry researcher; deep literature, experiments, notes,
  and long-running projects spanning years.
- **Daily workflow.** Reads and annotates many papers; keeps detailed research notes;
  connects ideas across a large personal corpus; returns to old threads.
- **Pain points.** Enormous personal corpus that's hard to search by meaning; losing
  the thread across long time spans; connecting related-but-scattered ideas.
- **Digital habits.** Large document libraries; meticulous but overwhelmed notes;
  values depth and accuracy over speed.
- **Devices.** Windows workstation (primary), Android tablet/phone for reading.
- **Privacy expectations.** High; unpublished research and IP; strong local-first
  preference.
- **Technical knowledge.** High in domain, moderate–high in tools.
- **Why NOVA helps.** Local semantic search over a large private corpus; durable
  multi-year memory that connects ideas across time; accurate recall with provenance;
  everything stays private.
- **Most-valuable modules.** Universal Search, Memory Engine (longevity), Multimodal
  (documents), AI Engine.

### 2.5.6 Persona 6 — The Photographer ("Leo")

- **Background.** Professional/serious-hobbyist photographer; large image libraries;
  shoots, culls, edits, and delivers.
- **Daily workflow.** Imports large batches; needs to find specific shots by content
  ("the sunset shots from the coast trip"); manages assets and client deliverables.
- **Pain points.** Finding specific images in huge libraries; organizing by content
  not just folders/dates; privacy of client and personal images in cloud tools.
- **Digital habits.** Storage-heavy; multiple devices; cares intensely about not
  uploading private/client images to opaque clouds.
- **Devices.** Windows PC (editing/storage), Android phone (capture/review).
- **Privacy expectations.** Very high for client and personal images; explicitly
  wants images to *not* leave the device.
- **Technical knowledge.** High in domain tools, moderate broadly.
- **Why NOVA helps.** On-device content-based image understanding and search ("find
  X") with images never leaving the device; local organization; cross-device recall.
- **Most-valuable modules.** Multimodal (Ch11), Universal Search, Memory Engine,
  Security/Privacy (Ch15).

### 2.5.7 Persona 7 — The Family User ("Ravi")

- **Background.** Non-technical adult managing a busy household; appointments,
  reminders, documents, photos, and everyday logistics.
- **Daily workflow.** Juggles family schedules, bills, documents, and reminders;
  wants simple help without learning complex tools.
- **Pain points.** Forgetting appointments/tasks; losing important documents; finding
  a photo/receipt; intimidated by complex apps.
- **Digital habits.** Phone-first; prefers voice; low tolerance for complexity;
  values simplicity and reliability.
- **Devices.** Android phone (primary), occasional Windows PC.
- **Privacy expectations.** Moderate but values not having family life mined; trusts
  "it stays on my phone" framing strongly.
- **Technical knowledge.** Low.
- **Why NOVA helps.** Simple voice-first capture and reminders; find documents/photos
  by asking; reliable, private, and forgiving; genuinely useful offline.
- **Most-valuable modules.** Voice, Memory Engine, Universal Search, UI/UX simplicity
  (Ch17). **Note on D2:** Ravi is still modeled as a *single user*; genuine
  multi-member household support is a Future Persona (§2.10), gated on OQ-1.

### 2.5.8 Persona 8 — The Power User ("Tanvi")

- **Background.** Highly organized enthusiast who pushes tools to their limits;
  optimizes their whole digital life; early adopter.
- **Daily workflow.** Automates everything; maintains elaborate systems; wants deep
  control, extensibility, and transparency into what tools do.
- **Pain points.** Tools that are closed, opaque, or non-extensible; cloud lock-in;
  inability to customize or inspect behavior.
- **Digital habits.** Runs many devices; builds custom workflows; reads the docs;
  wants plugins and configurability.
- **Devices.** Multiple Android + Windows devices.
- **Privacy expectations.** Very high and very informed; demands local-first,
  transparency, and ownership; will audit claims.
- **Technical knowledge.** Expert.
- **Why NOVA helps.** Extensible via plugins; transparent and inspectable; local-first
  and owned; a configurable autonomy dial; portable and exportable.
- **Most-valuable modules.** Plugin System (Ch13), Universal Search, Memory Engine,
  Consent Gate, Security/Privacy — essentially all of NOVA, deeply.

### 2.5.9 Persona synthesis — what they share

Across all eight, four needs recur and define NOVA's center: (1) **find what I
entrusted, by meaning, instantly, offline**; (2) **remember what matters and let me
control it**; (3) **help me act, without doing something I didn't want**; (4) **keep
my data mine.** Every Primary Goal (§2.3.1) maps onto one or more of these shared
needs — confirming the goals are persona-grounded, not invented.

---

## 2.6 Jobs To Be Done (JTBD)

JTBD are phrased from the user's point of view and are feature-independent. They are
the durable "jobs" NOVA is hired for; features (later chapters) are merely the
current best way to do them.

- **JTBD-1 — "Help me find something I forgot."** The user knows they *have* it
  (a note, a photo, a file, a decision, a fact someone told them) but not *where*.
  NOVA must let them describe it in natural language and retrieve it by meaning,
  instantly, offline. *Origin: fragmentation (Ch1 §1.3.1); serves SG-2, PG-1.*
- **JTBD-2 — "Remember this for me — and let me trust that memory."** The user hands
  NOVA something worth keeping and expects it recalled accurately at the right time,
  while retaining the power to inspect and correct it. *Serves PG-2; Principle 4.*
- **JTBD-3 — "Automate this repetitive work — but never surprise me."** The user wants
  drudgery handled autonomously when safe, and confirmation when consequential.
  *Serves PG-5; Principle 6.*
- **JTBD-4 — "Make every device feel like one system."** The user moves between phone
  and PC and wants continuity of memory and context without friction. *Serves SG-1;
  Principle 7.*
- **JTBD-5 — "Keep my private life private."** The user wants intelligence *without*
  paying the privacy tax; they want to know, and control, what leaves the device.
  *Serves PG-3; Principles 1, 2.*
- **JTBD-6 — "Understand what I'm actually working with."** The user's world is
  multimodal (images, docs, voice); they want NOVA to understand those materials, not
  just typed text. *Serves SG-4.*
- **JTBD-7 — "Tell me why."** When NOVA acts or recalls, the user wants a faithful
  explanation. *Serves SG-3; Principle 5.*
- **JTBD-8 — "Grow with me."** The user expects NOVA to become more useful over years
  and to be keep-able across devices and time. *Serves LG-1; Principle 7.*
- **JTBD-9 — "Be there when I'm offline."** The user expects real usefulness with no
  connectivity. *Serves PG-1; Principle 3.*

The Primary Goals exist precisely to satisfy JTBD-1, 2, 3, 5, and 9 — the jobs users
will pay for and stay for. Secondary/Long-Term goals satisfy the rest.

---

## 2.7 User Journey

The same NOVA fits naturally into the user's day and situations. Each scenario below
describes how NOVA participates *without violating any principle* — note how offline
capability, privacy, and consent recur.

- **Morning.** The user wakes and asks NOVA (by voice) what matters today; NOVA
  assembles reminders, follow-ups, and relevant memories *locally*. Any suggestion to
  act (e.g. send a follow-up) is offered, not executed, unless it's low-stakes and
  autonomy is enabled (D8).
- **Afternoon (work/office).** Between meetings, the user captures action items and
  decisions by voice/text; NOVA files them into owned memory and can, pre-meeting,
  surface the relevant context — all on-device, safe for confidential material
  (Persona Sara/Arjun).
- **Evening (home).** The user asks NOVA to find a document, a photo, or "that thing I
  noted"; universal search retrieves by meaning, offline. Household logistics get
  captured as reminders (Persona Ravi).
- **Travel.** On a plane or in a dead zone, NOVA is fully useful: capture, recall,
  search, reminders — because the core is on-device (PG-1). This is a signature
  moment that proves the vision.
- **Office (sensitive context).** Handling confidential data, the user relies on
  local-by-default and the visible egress log; nothing leaves the device without
  consent (Principle 2, D3).
- **Emergency (time-critical recall).** The user urgently needs a specific piece of
  information they entrusted to NOVA; fast, accurate, offline recall is the difference
  between useful and useless (PG-2, PG-4).
- **Offline.** The default-capable state: everything core works. NOVA clearly
  communicates only the *acceleration-only* features that are temporarily unavailable,
  never failing silently or bricking (PG-1).
- **Online.** With connectivity and consent, NOVA may use the acceleration seam for
  heavier tasks and enable cross-device sync — always as an *enhancement* over the
  working local baseline, always visible in the egress log (D1, D3, SG-1).

The through-line: NOVA is present, useful, and trustworthy in every scenario, and its
behavior in each is a direct expression of the Chapter 1 principles.

---

## 2.8 User Frustrations (with existing tools)

Stated as problems to solve, not criticisms. Each entry names the frustration and
exactly what NOVA must solve — feeding Chapter 3's requirements.

- **Google Photos / cloud photo tools.** *Frustration:* powerful search, but images
  live in the cloud under third-party control; content-based finding requires
  uploading private/client images. *NOVA must solve:* on-device, content-based image
  understanding and search with images never leaving the device unless consented
  (Persona Leo; SG-4, PG-3).
- **File managers.** *Frustration:* organize by folder/name/date, not by *meaning*;
  finding "the thing about X" means remembering where you filed it. *NOVA must solve:*
  meaning-based retrieval across the user's files, so the user asks instead of digs
  (SG-2, JTBD-1).
- **Search (in-app and system).** *Frustration:* keyword-bound, siloed per app, no
  cross-cutting semantic search over the user's whole world. *NOVA must solve:* one
  universal, semantic, offline search surface (SG-2).
- **AI assistants (chat-style).** *Frustration:* stateless per session, no durable
  owned memory, cloud-bound, incentives misaligned with the user. *NOVA must solve:*
  durable, owned, private memory with aligned incentives and offline capability
  (PG-1, PG-2, PG-3).
- **Voice assistants.** *Frustration:* shallow, cloud-dependent, forgetful, and
  privacy-suspect (always-listening sent to servers). *NOVA must solve:* capable,
  on-device voice with private wake-word handling and durable memory (PG-4, Ch10, 15).
- **Cloud storage.** *Frustration:* convenient sync at the cost of handing data to a
  provider; lock-in; no true ownership. *NOVA must solve:* local-by-default storage
  with *optional*, consent-gated, end-to-end-protected sync the user owns (SG-1, D3).
- **Cross-device workflows.** *Frustration:* devices feel like separate islands;
  context doesn't follow the user; syncing is either absent or privacy-costly. *NOVA
  must solve:* same-user continuity over a private seam so devices feel like one
  system (SG-1, JTBD-4).

Every frustration above traces to a Chapter 1 problem (§1.3) and to a goal here,
confirming NOVA is solving real, felt pain rather than inventing features.

---

## 2.9 Product Boundaries

Derived directly from the Chapter 1 anti-scope (§1.5) and principles. Binding.

### 2.9.1 What NOVA SHOULD do

- Deliver real, offline-first daily usefulness for a single user (PG-1).
- Remember what matters, accurately, under full user control (PG-2).
- Make trust and privacy demonstrable, with visible/consented egress (PG-3).
- Interact naturally and fast via voice and text (PG-4).
- Act on the user's behalf within the consent gate (PG-5).
- Provide universal, meaning-based search over the user's entrusted world (SG-2).
- Offer same-user cross-device continuity over a private seam (SG-1).
- Explain its behavior faithfully and let the user correct/undo (SG-3, Principle 5).
- Be owned, exportable, migratable, and built to last (Principle 7).

### 2.9.2 What NOVA SHOULD NEVER do

- **Never** send data off-device without a visible reason and, where material,
  consent (Principle 2, D3). No silent phone-home, ever.
- **Never** monetize, mine, profile-for-third-parties, or sell user data (anti-scope;
  Principles 1–2).
- **Never** take an irreversible or high-stakes action without explicit consent
  (Principle 6, D8).
- **Never** become an engagement/attention machine or optimize time-on-app
  (anti-scope, Principle 8).
- **Never** require the cloud for core functionality (Principle 3, D1).
- **Never** hide what it did with what it knows (Principle 5); no "explanation theater."
- **Never** manufacture confidence; it must be honest about uncertainty and limits
  (Principle 9).
- **Never** (in v1) become a multi-user/team product without a deliberate vision
  amendment (D2; OQ-1).
- **Never** lock the user in; data must remain exportable and NOVA independent of any
  single fatal vendor dependency (Principle 7, C0-5).

---

## 2.10 Future Personas (post-v5.0)

These personas are *deliberately deferred* because serving them requires capabilities
or scope expansions gated on open questions (chiefly OQ-1). They are recorded so the
architecture can leave clean seams, not so they distort v1.

- **The Multi-Member Household.** Genuine support for several distinct users sharing a
  household context with per-user privacy boundaries. *Gated on OQ-1 (multi-user is a
  vision amendment).* The single-user architecture must leave a clean seam, not
  pre-build this.
- **The Small Team / Professional Practice.** A tightly-scoped, consented shared
  context for a small trusted group. *Gated on OQ-1 and a business-model decision
  (OQ-3).* Must never compromise the single-user privacy model for existing users.
- **The Accessibility-First User.** A user whose primary interface is fully
  voice/assistive due to a disability, requiring NOVA to be a primary life-interface.
  *Partially served by good v1 UX (Ch17); elevated to a first-class persona later.*
- **The Developer-as-Extender.** A third-party developer building plugins for others
  (not just for themselves). *Gated on the anti-scope decision about becoming a
  platform (Ch1 §1.5.2); only via deliberate amendment.*
- **The Long-Horizon Inheritor.** A user (or their heir) treating NOVA as a decades-
  long, inheritable archive. *Served directionally by XG-3; formalized post-v5.*

Recording these here discharges Principle 7's foresight duty without violating
Principle 8's coherence discipline: we design *seams*, not *features*, for them.

---

## 2.11 Risks

Product-level risks (implementation risks live in later chapters). Each ties to a
Chapter 1 risk where applicable.

- **Product risk — usefulness decay.** Novelty fades and NOVA fails to become a
  *daily* habit. *Mitigation:* relentless focus on the "daily usefulness" KPI and the
  top JTBD; cut anything that doesn't serve them (Principle 8). *(Extends R1/R3.)*
- **Business risk — constrained monetization.** No ads/data-sales narrows revenue
  options (OQ-3 unresolved). *Mitigation:* resolve OQ-3 early with a disciplined paid
  model built *on* the privacy/ownership premium. *(R6.)*
- **User-adoption risk — capability perception.** Users judge NOVA against frontier
  cloud assistants and find the local baseline lacking. *Mitigation:* set honest
  expectations (Principle 9), make memory/context/offline the felt wins, and use the
  acceleration seam for hard tasks. *(R1.)*
- **Privacy risk — trust breach or perception thereof.** Any leak, or even the
  appearance of one, is existential for a privacy-first product. *Mitigation:* egress
  transparency KPI at 100%, security hardening (Ch15), and provable local-by-default.
  *(R7.)*
- **Expectation risk — over-promising agency.** Users expect more autonomy than is
  safe, or blame NOVA for consented actions. *Mitigation:* conservative default dial,
  faithful explanations, clear reversibility, and the unwanted-action KPI near zero.
  *(R7, PG-5.)*
- **Scope risk — persona sprawl.** Serving eight personas tempts feature sprawl that
  dilutes coherence. *Mitigation:* the shared-needs synthesis (§2.5.9) keeps the team
  building the *four common needs*, not eight bespoke products. *(Principle 8, R3.)*
- **Adoption risk — ownership-as-burden.** True ownership (keys, backups) can feel
  heavy or lead to data loss for non-technical personas (Ravi, Meera). *Mitigation:*
  careful UX and recovery that doesn't betray privacy (Ch15, 17). *(R5.)*

---

## 2.12 Open Questions

Recorded, not answered. Each is routed to the chapter that must resolve it.

- **OQ2-1.** Exact target bands for every KPI in §2.4 (finalized in Ch4/16 against
  real minimum-hardware measurements). *Default: the indicative bands stated.*
- **OQ2-2.** How to *measure* "user trust" rigorously (self-report + which behavioral
  proxies?) without itself violating privacy. *Routed to Ch15/Ch19.*
- **OQ2-3.** Which JTBD constitute each persona's "top three" for the PG-1 offline
  success criterion. *Routed to Ch3.*
- **OQ2-4.** The positive business model (inherits Ch1 OQ-3). *Blocks nothing here;
  must resolve before pricing/packaging decisions.*
- **OQ2-5.** Whether the Family User (Ravi) reveals enough latent multi-user demand to
  reconsider OQ-1 earlier than v5. *Routed to Ch20 roadmap review.*
- **OQ2-6.** The precise default position of the autonomy dial per action class
  (inherits Ch1 OQ-5). *Routed to Ch6/Ch11.*
- **OQ2-7.** Scope of "the user's entrusted world" — which data classes NOVA indexes by
  default vs. only on explicit add. *Routed to Ch8/Ch9 and Ch15 (privacy).*

---

## 2.13 Final Recommendation

**Adopt the tiered goal set (PG-1…5 as non-negotiable v1, SG-1…4 for excellence,
LG-1…3 and XG-1…3 as direction), the persona-grounded KPI set of §2.4, and the eight
personas synthesized to four shared needs, as the binding product direction for
NOVA.**

The recommendation is justified by a single line of reasoning that runs straight back
to Chapter 1: **NOVA's vision is a private, personal, on-device, memory-centric,
agentic companion; the goals above are exactly the measurable commitments that make
that vision real for real people, and the KPIs are exactly the evidence that would
prove it.** Nothing in this chapter adds ambition beyond Chapter 1 — it *operational-
izes* Chapter 1. Every goal traces to a principle (§2.2); every persona shares the
four core needs (§2.5.9); every frustration maps to a Chapter 1 problem (§2.8); every
boundary is the anti-scope made concrete (§2.9).

**Alternatives considered for the product direction, and why rejected:**

- *Breadth-first (serve the widest audience with the most features).* Rejected: it
  violates Principle 8 (coherence) and the depth-over-breadth rationale (Ch1 §1.7.2);
  it would produce an incoherent assistant that wins nowhere.
- *Engagement-optimized direction (maximize usage metrics).* Rejected: it violates the
  anti-scope and Principle 8; it corrupts the product's soul. This is why §2.4
  deliberately omits engagement KPIs.
- *Cloud-capability-first direction (chase the frontier by leaning on the cloud).*
  Rejected: it violates Principles 2–3 and D1; it re-introduces the very privacy tax
  NOVA exists to abolish. The capability gap is instead addressed by XG-1 (improving
  local models) and the consent-gated acceleration seam.
- *Feature-parity-with-incumbents direction.* Rejected: chasing incumbents' feature
  lists dilutes identity (Principle 8) and cedes the initiative; NOVA competes on
  privacy, ownership, memory, and offline usefulness — its defensible ground.

**Recommended next step (per Autonomous Documentation Mode):** proceed directly to
**Chapter 3 — Functional Requirements Specification**, deriving formal, testable
requirements from the goals (§2.3), KPIs (§2.4), personas (§2.5), and JTBD (§2.6)
established here, strictly within the Chapter 1 boundaries and using the RFC-2119
modal verbs fixed in Phase 0 (§0.5).

*End of Chapter 2.*
