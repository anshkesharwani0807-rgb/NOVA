# Contributing to NOVA

Thank you for your interest in NOVA. This document defines *how* to contribute.
Detailed engineering standards will be formalized in **Chapter 18 (Development
Standards)** and **Chapter 19 (Testing, QA & Release)** of the NOVA Bible; this file
is the working baseline until then.

## The prime directive

**The NOVA Bible (`docs/bible/`) is the only source of truth.** Every contribution
must conform to it, especially the **nine ordered principles** in Chapter 1. A change
that conflicts with a Bible decision is not accepted until the Bible is formally
amended (Phase 0 §0.6).

## Before you start

- Read Chapter 1 (Vision & Philosophy) and Chapter 2 (Goals & Personas).
- Check the open decisions: **OQ-1** (single vs. multi-user) and **OQ-2** (stack).
  These are unresolved; do not hardcode assumptions that would pre-empt them.
- Note that `src/`, `modules/`, `plugins/`, `sdk/`, `api/`, `db/` are **provisional**
  until their defining chapters (5, 6, 13, 14) are written.

## Workflow (baseline — final rules in Ch18)

1. Open or claim an issue (use the templates in `.github/ISSUE_TEMPLATE/`).
2. Create a branch from the default branch. Do **not** commit directly to it.
3. Make focused, well-described changes. Match surrounding style.
4. Open a pull request using the PR template; link the issue.
5. Ensure the change is traceable: principle → goal → requirement → change.

## Documentation contributions

- Bible chapters follow the mandatory section template and versioning in Phase 0.
- Never compress or summarize Bible content to fit; depth over brevity.
- Update `CHANGELOG.md` for any notable change.

## What we will not accept (per Chapter 1 anti-scope)

- Anything that sends user data off-device without visible reason + consent.
- Data monetization, profiling-for-third-parties, or engagement-maximizing features.
- Features that require the cloud for core functionality.
- Irreversible/high-stakes autonomous actions without a consent gate.

## Code of Conduct

All participation is governed by [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

## Security

Never file security issues publicly. Follow [`SECURITY.md`](SECURITY.md).
