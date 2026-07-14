# M5 Tasks: Universal Search Index

- [x] Lexical full-text search layer (SQLite)
- [x] Integration with Memory Engine (Event-based sync)
- [x] Local vector index implementation (VectorStore — exact cosine KNN, SQLite-backed)
- [x] Integrate VectorStore into SearchEngine (with rebuild-on-open persistence)
- [x] Implement Hybrid retrieval logic (normalized lexical + semantic fusion)
- [x] Permission-scoped indexing (source revocation actually removes vectors)
- [x] Natural language query interface (`SearchQuery::parse`: tag/source/category/phrase)
- [x] Search latency benchmarks (NFR-PERF-003)

> Note: the M5 vector index uses an exact brute-force cosine store rather than an HNSW
> ANN index. At single-user on-device scale it is fast, gives perfect recall, and — unlike
> the `hnsw_rs` crate evaluated earlier — supports true removal (required for revocation)
> and restart persistence (rebuilt from the authoritative embeddings in SQLite). An ANN
> backend can replace it behind the same `VectorStore` API if corpus size ever demands it.

