# Changelog

All notable changes to NOVA are documented here.
Format based on [Keep a Changelog]; versioning follows the rules in
`docs/bible/Chapter-00-Phase-0-Planning.md` (§0.6) for documentation and will
follow the software-versioning policy defined in Chapter 19 once written.

## [Unreleased]

### Added
- **Milestone 4 — Encrypted Memory Engine (persistent, local, offline).** NOVA's
  first persistent store: a local SQLite database (bundled, offline-only — no cloud)
  with automatic creation and versioned schema migration (`PRAGMA user_version`, v1).
  Sensitive fields (title, content, tags, source) are encrypted at rest with
  AES-256-GCM via a keychain-ready `KeyProvider` seam (OS keystore integration later;
  SQLCipher can drop in behind the same abstraction). `MemoryRecord` carries full
  metadata (uuid, category, title, content, tags, created/updated, importance, source,
  device_id, correlation_id, version, deleted). 13 `MemoryCategory` enum variants.
  Clean `MemoryEngine` API: initialize/open/close/health/insert/update/delete/find/
  find_by_id/search/count/exists/transaction/backup/restore/vacuum, plus restore/
  purge for soft-deleted records. Local search: exact/contains/prefix, tag + category
  filters, case-insensitivity, sorting, limit/offset pagination. Soft-delete with
  recovery; permanent purge is a separate explicit step. Thread-safe via a mutex
  (safe concurrent reads/writes, no races). Every operation logs through the existing
  Activity Trail (operation, record id, category, correlation id). Implements the
  `KernelModule` lifecycle (opens the DB on initialize, closes on shutdown, reports
  record count as health). 18 tests (crypto 3 + engine 15) covering init, CRUD, soft
  delete/restore, search/pagination/tags/categories, transactions + rollback,
  concurrent access, restart persistence, at-rest encryption, wrong-key rejection,
  migration, health, backup and restore. Demo shows store → restart → reload → search
  → update → delete → restore → health.
- **Milestone 3 — Module Registry + Dependency Injection + Lifecycle.** A
  `KernelModule` trait (`module_id`/`version`/`dependencies`/`initialize`/`start`/
  `stop`/`shutdown`/`health`) and a thread-safe `ModuleRegistry` (register,
  unregister, lookup, list, health report, topological dependency resolution).
  Lifecycle states Boot→Initialized→Ready→Running→Stopping→Shutdown, brought up in
  dependency order and torn down in reverse. Wired into `Kernel::bootstrap` (the
  kernel owns the registry; the composition root registers modules). All six modules
  (memory, search, ai, voice, comms, plugin_host) now implement `KernelModule`
  without behavior change; FFI + demo register and drive them through the registry.
  6 unit tests + 4 integration tests (registration, duplicate protection, dependency
  resolution, missing-dep/cycle errors, lifecycle transitions, health, shutdown order).
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
