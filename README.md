# NOVA

> A private, personal artificial intelligence that belongs entirely to its user —
> a lifelong digital companion that remembers what matters, understands context,
> acts on the user's behalf, and runs first and foremost on the user's own devices,
> under the user's own control.

**Status:** Genesis (documentation + skeleton). **Version:** see [`VERSION`](VERSION).
**Platforms (initial):** Android + Windows. Linux/macOS later.

---

## What NOVA is

NOVA is a **single-user, on-device-first personal AI assistant**. Its intelligence
and durable memory live on the user's own devices; the cloud is an optional,
consent-gated accelerant, never a precondition. The full rationale, principles, and
non-negotiables live in the **NOVA Bible**.

## The NOVA Bible (source of truth)

All product and engineering decisions derive from the Bible in
[`docs/bible/`](docs/bible/). Start there:

- **Phase 0** — Planning, Standards & Governance
- **Chapter 1** — Product Vision & Philosophy *(foundational; everything conforms to it)*
- **Chapter 2** — Product Goals & User Personas
- *Chapters 3–20 + Closing Reviews — in progress*

> The Bible is the **only source of truth**. If code and Bible disagree, the Bible
> wins until formally amended (see Phase 0 §0.6 versioning rules).

## The nine principles (strictly ordered — lower number wins on conflict)

1. The user is sovereign
2. Privacy is the default
3. On-device first
4. Memory is sacred
5. Transparency over magic
6. Agency with consent
7. Longevity and ownership
8. Coherence over feature count
9. Honesty about limits

## Repository layout

| Path | Purpose |
|---|---|
| `docs/` | All documentation, including the Bible, audits, and governance |
| `src/` | Application source *(PROVISIONAL — awaits Ch5/Ch6 + stack decision OQ-2)* |
| `modules/` | Module implementations *(PROVISIONAL — awaits Ch6)* |
| `plugins/` | Plugin implementations *(PROVISIONAL — awaits Ch13)* |
| `sdk/` | Plugin/extension SDK *(PROVISIONAL — awaits Ch13)* |
| `api/` | Internal API surfaces *(PROVISIONAL — awaits Ch5/Ch6)* |
| `db/` | Data/storage design artifacts *(PROVISIONAL — awaits Ch14)* |
| `config/` | Configuration *(schema pending Ch14/Ch18)* |
| `tests/` | Test suites: unit / integration / e2e / performance / security *(Ch19)* |
| `scripts/` | Development & automation scripts |
| `assets/` | Brand, diagrams, static assets |
| `examples/` | Usage examples |
| `roadmap/` | Roadmap artifacts *(Ch20)* |
| `design/` | UX/design artifacts *(Ch17)* |
| `research/` | Spikes, model/hardware research, references |
| `.github/` | Issue/PR templates and CI workflow placeholders |

## Contributing & policies

- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md)
- [`SECURITY.md`](SECURITY.md) — **read this: privacy & security are the product's core**
- [`CHANGELOG.md`](CHANGELOG.md)

## License

[MIT](LICENSE).
