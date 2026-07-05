# Changelog

All notable changes to NOVA are documented here.
Format based on [Keep a Changelog]; versioning follows the rules in
`docs/bible/Chapter-00-Phase-0-Planning.md` (§0.6) for documentation and will
follow the software-versioning policy defined in Chapter 19 once written.

## [Unreleased]

### Added
- **Milestone 2 — Consent Gate + Egress Gate.** Kernel-level `ConsentManager`
  (Allow Once / Allow for Session / Always Allow / Always Deny) and `EgressGate`
  through which every network/plugin/AI/sync/cloud/external request must pass.
  Policies: Offline Only / Local Network Only / Internet Allowed / Blocked
  (policy overrides consent). Every decision is logged to the Activity Trail and
  Egress Log with timestamp, destination, reason, consent state, and correlation
  id (D3/D8). Wired into `Kernel::bootstrap` with a privacy-first default policy.
  17 kernel unit tests + 6 integration tests (granted/denied/expired/blocked/
  policy-override). Demo (`nova_demo`) shows a denied→allowed egress flow.
- **Milestone 1 — Kernel foundation.** Cargo workspace; microkernel (error model,
  layered config, tokio event bus with provenance, three-plane logging, bootstrap);
  module skeletons; C-ABI FFI seam; runnable `nova_demo`.
- ADRs 0001–0010 (stack, cross-platform, architecture, event bus, concurrency,
  storage, inference runtime, configuration, logging, error handling).
- Repository skeleton (documentation-independent scaffold): folder hierarchy,
  standard project files, Git initialization, issue/PR templates, CI workflow
  placeholders.
- NOVA Bible: Phase 0 (Planning & Governance), Chapter 1 (Product Vision &
  Philosophy), Chapter 2 (Product Goals & User Personas).
- Baseline engineering audit report (`docs/audits/`).

### Notes
- Architecture-dependent folders (`src`, `modules`, `plugins`, `sdk`, `api`,
  `db`) are PROVISIONAL placeholders pending Chapters 5, 6, 13, 14.
- Open decisions OQ-1 (single vs. multi-user) and OQ-2 (concrete stack) are
  proceeding on Chapter 1 defaults and remain amendable.

## [0.0.0-genesis] - 2026-07-04
- Project genesis: vision, goals, governance, and skeleton established.
