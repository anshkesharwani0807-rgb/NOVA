# CHANGELOG

## [0.1.0] - 2026-07-11

### Added
- **Natural Language Query Parser:** Implemented `QueryParser` in `modules/search/src/parser.rs` to allow filtering via `tag:`, `cat:`, and `src:`.
- **Performance Benchmarking Suite:** Created `modules/search/tests/benchmarks.rs` to measure search, indexing, and update latency.
- **Search Query Enhancements:** Added `SearchQuery::text()` method for flexible query building.

### Fixed
- **Search Engine SQLite Error:** Fixed `ESCAPE expression must be a single character` by updating `like_escape` to use backslash (``) instead of an empty string.
- **Schema Version Test:** Corrected `test_schema_version` to expect version 2.
- **Clippy Warnings:** Removed unused imports and needless borrows in the search module.

### Performance
- Verified search latency meets NFR-PERF-003 (< 800ms) for datasets up to 10,000 records.
