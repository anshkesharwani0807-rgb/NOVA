# ADR-0006 — Local Persistence & Storage Engine

- **Decision ID:** ADR-0006
- **Status:** Proposed
- **Date:** 2026-07-04
- **Governed by:** D1 (on-device), D4/D7 (owned, encrypted, durable, portable memory),
  Principle 2 (privacy), Principle 7 (longevity/export), SG-2 (search). **Feeds:** Ch14.

## Context

NOVA must durably store, on-device, for years: (a) **structured data** (memories,
metadata, settings, provenance/activity trail) and (b) **semantic vectors** for
meaning-based Universal Search (SG-2). Storage must be **encrypted at rest** (Principle
2, D7), **portable/exportable** (Principle 7), and reliable across device migrations.

## Options Considered

**Structured store:**
1. **Embedded SQL database (SQLite-class), encrypted.** Ubiquitous, durable, decades of
   longevity, transactional, portable single-file, huge tooling. Encryption via an
   encrypted-SQLite variant or app-level encryption.
2. **Embedded key-value store (LMDB/RocksDB-class).** Fast, but less queryable; we would
   rebuild relational/query features ourselves.
3. **Custom file format.** Maximum control; maximum risk to longevity/reliability — bad
   fit for Principle 7. Rejected.

**Vector store:**
4. **Embedded vector index (HNSW-class) persisted locally**, alongside the SQL store.
5. **Vector features inside the SQL engine** (SQL extension for vectors).
6. **Separate heavyweight vector database.** Overkill on-device. Rejected.

## Chosen Solution

- **Structured: an embedded, encrypted SQL database (SQLite-class) [Option 1]** as the
  system of record for memories, metadata, settings, and the provenance/activity trail.
  Chosen for durability, portability (single-file export → Principle 7), transactional
  integrity, and unmatched longevity.
- **Vectors: an embedded local vector index (HNSW-class) [Option 4]**, kept in sync with
  the SQL store, for semantic search. An in-engine vector extension (Option 5) is a
  sanctioned alternative if it simplifies consistency without hurting performance.
- **Encryption at rest:** all stores are encrypted; keys are managed per ADR-0013
  (key management) and never leave the device (Principle 2). Encryption is mandatory,
  not optional.
- **Export/portability:** a documented, versioned export produces a complete,
  re-importable archive of all stores (Principle 7, XG-3) — the technical basis of
  "owned memory."

## Trade-offs

- **(-) Two stores to keep consistent** (SQL + vector index). *Mitigated* by treating
  SQL as the source of truth and the vector index as a derived, rebuildable artifact;
  a corrupt index can always be regenerated from SQL.
- **(-) Encrypted SQLite adds complexity/latency.** *Accepted:* encryption at rest is
  non-negotiable for a privacy-first product (Principle 2).
- **(+) Longevity & portability** — SQLite is among the most durable, portable formats
  in computing; ideal for a decade-plus companion (Principle 7).
- **(+) Rebuildable derived data** keeps the durability guarantee simple.

## Consequences

- Bible Chapter 14 details schemas (design-level, not implementation — Phase 0 C0-1),
  migration, and the export format.
- The Memory Engine (Ch8) and Universal Search (Ch9) build on these two stores.
- Storage growth over years (audit SC-1) requires pruning/compaction policies, owned by
  Ch14/Ch16.
- Key management (ADR-0013) is a hard dependency of this ADR.
