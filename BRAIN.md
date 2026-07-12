# BRAIN.md — NOVA project handoff for any AI / engineer

> **Read order for any coding agent:** `BRAIN.md` → `AI_CONTEXT.md` → the relevant
> `tasks/M<n>.md` — *before making any changes.*
>
> This is the single-page mental model of NOVA: what it is, how it is built, what exists,
> the hard rules, and how to continue safely. Deep reasoning lives in `docs/bible/` (the
> "NOVA Bible") and `docs/adr/` (decisions).

---

## 1. What NOVA is

NOVA is a **single-user, on-device-first, privacy-first personal AI assistant** ("digital
brain") for **Android + Windows** (Linux/macOS later). It remembers, searches, and acts
on the user's data locally. Cloud is an **optional, consent-gated accelerant — never
required**. See `docs/bible/Chapter-01-Product-Vision-and-Philosophy.md`.

**The nine ordered principles** (lower number wins on conflict):
1 user sovereign · 2 privacy by default · 3 on-device first · 4 memory is sacred ·
5 transparency over magic · 6 agency with consent · 7 longevity/ownership ·
8 coherence over features · 9 honesty about limits.

---

## 2. Architecture diagram

```
                 User
                  │
                  ▼
      Android UI  /  Windows UI        (future — no UI yet)
                  │
                  ▼
                 FFI                    (api/ffi — C-ABI seam, nova_ffi)
                  │
                  ▼
                Kernel                  (src/kernel — nova_kernel)
                  │
      ┌─────┬─────┼─────┬─────┬─────┐
      ▼     ▼     ▼     ▼     ▼     ▼
   Memory Search Voice   AI  Comms Plugins   (modules/*)
```

The **Kernel** owns cross-cutting facilities (event bus, config, logging, consent, egress
gate, module registry). **Modules** plug into the kernel and talk to each other only
through the **event bus** — never by constructing peers directly (dependency injection).
The **composition root** (FFI / demo) constructs and registers modules, because the kernel
crate must not depend on module crates (would be circular).

---

## 3. Module dependency graph

Crate-level (Cargo) dependencies as they exist **today**:

```
Kernel (nova_kernel)        depends on: nothing internal
Memory (nova_memory)        └── Kernel
Search (nova_search)        ├── Kernel
                            └── Memory          (indexes memory records)
Voice  (nova_voice)         └── Kernel          (skeleton)
AI     (nova_ai)            └── Kernel          (skeleton)
Comms  (nova_comms)         └── Kernel          (skeleton)
Plugin Host (nova_plugin_host) └── Kernel       (skeleton)
FFI    (nova_ffi)           └── Kernel + all modules  (composition root)
Demo   (nova_demo)          └── Kernel + all modules  (composition root)
```

Planned future edges (NOT yet implemented — do not add until their milestone):
`AI → Memory, Search, Voice`. Keep the graph **acyclic**; the kernel depends on no module.

---

## 4. Where things live (all on the **D: drive**, never C:)

Project root: `D:\Ansh Kesharwani\Documents\NOVA`

- `BRAIN.md` (this), `AI_CONTEXT.md` (live state), `README.md`, `CHANGELOG.md`, `roadmap/ROADMAP.md`
- `tasks/M1.md … M6.md` — per-milestone briefs (read the current one before coding)
- `docs/bible/` — source-of-truth design (Phase 0 + Chapters 1–4 written; 5–20 TBD)
- `docs/adr/` — Architecture Decision Records ADR-0001..0010 (**read before changing architecture**)
- `docs/governance/` — decision log, traceability matrix, glossary
- `src/kernel/` — `nova_kernel` (the microkernel, §6)
- `modules/{memory,search,voice,ai,comms,plugin_host}/` — one crate each
- `api/ffi/` — `nova_ffi` C-ABI seam for the future UI shells
- `apps/nova-demo/` — `nova_demo` runnable smoke test (NOT the product UI)
- `.nova-runtime/` — local runtime data (DBs, keys, logs); gitignored; created on D: at runtime

GitHub: private repo `https://github.com/anshkesharwani0807-rgb/NOVA` (branch `main`).

---

## 5. Toolchain & how to build/test (IMPORTANT)

- Language **Rust**, Cargo **workspace** at the repo root.
- **Use the MSVC toolchain** (the default gnu one fails on `dlltool`/OpenSSL). If you hit
  those errors, prefix commands with the msvc toolchain:

  ```bash
  TC=+stable-x86_64-pc-windows-msvc
  cargo $TC fmt --all -- --check
  cargo $TC clippy --workspace --all-targets -- -D warnings
  cargo $TC test --workspace
  cargo $TC run -p nova_demo
  ```

- **CI** (`.github/workflows/ci.yml`, windows-latest) runs those exact three gates.
  Every commit MUST pass all three locally first.
- SQLite via `rusqlite` feature `bundled` (builds fine with MSVC `cl.exe`).
  **Do NOT use SQLCipher `vendored-openssl`** — it does not build here (msys Perl lacks
  `Locale::Maketext::Simple`). At-rest encryption uses a pure-Rust `aes-gcm` layer behind a
  `KeyProvider` seam instead (see M4).

---

## 6. The kernel in one screen (nova_kernel, src/kernel)

- `error` — `NovaError { category, code, message, correlation_id }`, `Result<T>`. **No panics in lib code.**
- `event_bus` — tokio broadcast (pub/sub) + mpsc/oneshot (request/response); `NovaEvent`
  carries `EventMetadata` (origin, correlation id, causing_action) + `Arc<dyn Any>` payload.
- `config` — layered `NovaConfig`; **defaults private/conservative** (local_by_default=true,
  telemetry off, autonomy "conservative").
- `logger` — three planes: diagnostic; **Activity Trail** (`log_activity`, user-facing "why");
  **Egress Log** (`log_egress`). Local-only.
- `consent` + `egress` (M2) — `ConsentManager` (Allow Once/Session/Always/Deny) and
  `EgressGate` (Offline/LocalNetwork/Internet/Blocked). **All outbound goes through the gate;
  policy overrides consent.**
- `module` (M3) — `KernelModule` trait (`module_id/version/dependencies/initialize/start/
  stop/shutdown/health`) + `ModuleRegistry` (register/lookup/list/health/topo-resolve/
  bring_up/tear_down).
- `Kernel` — owns `event_bus`, `consent`, `egress_gate`, `registry`; created in `Kernel::bootstrap`.

---

## 7. Milestone status (what actually works)

- ✅ **M1 Kernel foundation** — workspace, error/event-bus/config/logging/bootstrap, FFI seam, demo.
- ✅ **M2 Consent + Egress Gate** — D3/D8 enforced; every decision logged.
- ✅ **M3 Module Registry + DI + Lifecycle** — all 6 modules implement `KernelModule`.
- ✅ **M4 Encrypted Memory Engine** (`nova_memory`) — local encrypted SQLite (AES-256-GCM +
  `KeyProvider`); `MemoryRecord`, 13 `MemoryCategory`; full API; persists across restarts;
  publishes `MemoryEvent` (Created/Updated/Deleted) on the event bus.
- 🔄 **M5 Universal Search Index** (`nova_search`) — IN PROGRESS. `SearchEngine` (SQLite index):
  insert/update/delete/search/rebuild/clear/stats; query types exact/partial/prefix/phrase,
  AND/OR, tag/source/category/date filters, ranking, pagination; auto-indexes memory via
  `MemoryEvent`; future seams for vector/semantic/image/OCR/face/object search (trait stubs).
  Depends on `memory`. **See `AI_CONTEXT.md` for exact in-progress files.**
- ⏳ **M6+** — AI runtime, plugin sandbox, voice, then remaining capabilities.

`voice`, `ai`, `comms`, `plugin_host` are working `KernelModule` **skeletons** (start/stop
cleanly, no real work yet).

---

## 8. NEVER do this (DON'T-DO list)

**NEVER**
- rewrite or "improve" the overall architecture
- replace Rust with another language
- remove, rewrite, or ignore ADRs (`docs/adr/`)
- replace or bypass the Kernel
- bypass the **Consent** manager or the **Egress** gate for any outbound action
- change a module's **public API** without a stated, necessary reason
- rename crates, modules, or files
- introduce a **circular dependency** (esp. kernel → module)
- add cloud/network calls by default (privacy-by-default; egress is opt-in + gated)
- use SQLCipher `vendored-openssl` (does not build here)
- `panic!` in library code (return `NovaError`)
- commit runtime data / secrets (`.nova-runtime/`, keys) — they are gitignored for a reason
- break passing tests, or commit without the three green CI gates

Following this list prevents hallucinated refactors and wasted work.

---

## 9. How to continue a milestone (the loop)

1. Read `BRAIN.md` → `AI_CONTEXT.md` → `tasks/M<n>.md` + relevant ADR/Bible chapter.
2. Implement **additively**; keep each module's SQLite DB local under `.nova-runtime/`.
3. Add comprehensive tests (unit + integration).
4. Run: `cargo $TC fmt --all` → `clippy --workspace --all-targets -- -D warnings` → `test --workspace`.
5. Run `cargo $TC run -p nova_demo` and confirm the new capability is visible.
6. Update `CHANGELOG.md`, `roadmap/ROADMAP.md`, **and `AI_CONTEXT.md`**.
7. Commit (detailed message + co-author line) and push to `main`. CI must be green.

---

## 10. Gotchas learned the hard way

- The auto-formatter/linter reformats files after writes — **Read before Edit** if a prior write may have been reformatted.
- Event-bus delivery is async — integration tests relying on a published event should wait briefly / poll.
- SQL: inline only integers (safe); always **bind** user strings (injection safety). Use `params_from_iter` for dynamic params.
- Two SQLite DBs exist: the **encrypted memory** DB and the **plaintext derived search index**. The search index is a
  cache; its at-rest hardening is deferred to the same future whole-DB encryption path as memory.

*Keep this file (and `AI_CONTEXT.md`) updated at the end of every milestone.*
