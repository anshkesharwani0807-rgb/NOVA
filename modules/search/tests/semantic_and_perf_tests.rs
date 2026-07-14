//! Semantic (vector) search, source revocation, persistence, NL-parsing and a latency
//! benchmark for the Universal Search index (Milestone 5).

use nova_search::document::{IndexDocument, MatchMode, SearchQuery};
use nova_search::engine::{SearchEngine, EMBEDDING_DIM};
use std::time::Instant;
use tempfile::tempdir;

/// A deterministic unit embedding: all zeros except a single 1.0 at `seed % DIM`, so two
/// documents built from the same seed have cosine similarity 1.0 and different seeds 0.0.
fn embedding(seed: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; EMBEDDING_DIM];
    v[seed % EMBEDDING_DIM] = 1.0;
    v
}

fn embedded_doc(id: &str, title: &str, content: &str, seed: usize) -> IndexDocument {
    IndexDocument {
        source: "memory".to_string(),
        source_id: id.to_string(),
        category: "note".to_string(),
        title: title.to_string(),
        content: content.to_string(),
        tags: vec![],
        source_field: "manual".to_string(),
        metadata: "{}".to_string(),
        created_at: 1000,
        updated_at: 2000,
        importance: 0,
        embedding: Some(embedding(seed)),
    }
}

#[test]
fn semantic_search_ranks_nearest_first() {
    let dir = tempdir().unwrap();
    let engine = SearchEngine::open(&dir.path().join("s.db")).unwrap();

    engine
        .insert(&embedded_doc("1", "Alpha", "about cats", 1))
        .unwrap();
    engine
        .insert(&embedded_doc("2", "Beta", "about dogs", 2))
        .unwrap();
    engine
        .insert(&embedded_doc("3", "Gamma", "about birds", 3))
        .unwrap();

    let mut q = SearchQuery::new();
    q.embedding = Some(embedding(2)); // closest to doc "2"
    let results = engine.search(&q).unwrap();

    assert!(!results.is_empty());
    assert_eq!(results[0].document.source_id, "2");
}

#[test]
fn revocation_removes_vector_from_semantic_results() {
    let dir = tempdir().unwrap();
    let engine = SearchEngine::open(&dir.path().join("s.db")).unwrap();

    engine
        .insert(&embedded_doc("1", "Secret", "sensitive", 7))
        .unwrap();

    let mut q = SearchQuery::new();
    q.embedding = Some(embedding(7));
    assert_eq!(engine.search(&q).unwrap().len(), 1);

    // Revoke the source: the vector must actually leave the searchable set.
    engine.delete("memory", "1").unwrap();
    assert_eq!(engine.search(&q).unwrap().len(), 0);
}

#[test]
fn vectors_persist_across_reopen() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("s.db");

    {
        let engine = SearchEngine::open(&db).unwrap();
        engine
            .insert(&embedded_doc("1", "Persisted", "content", 5))
            .unwrap();
    } // engine dropped, connection closed

    // Reopen: the vector index is rebuilt from the embeddings stored in SQLite.
    let engine = SearchEngine::open(&db).unwrap();
    let mut q = SearchQuery::new();
    q.embedding = Some(embedding(5));
    let results = engine.search(&q).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document.source_id, "1");
}

#[test]
fn nl_parse_extracts_filters_and_phrase() {
    let q = SearchQuery::parse("source:memory category:note #urgent \"quarterly report\" budget");
    assert_eq!(q.source.as_deref(), Some("memory"));
    assert_eq!(q.category.as_deref(), Some("note"));
    assert!(q.tags.contains(&"urgent".to_string()));
    assert_eq!(q.mode, MatchMode::Phrase);
    let text = q.text.unwrap();
    assert!(text.contains("quarterly report"));
    assert!(text.contains("budget"));
}

#[test]
fn nl_parse_plain_text_is_partial() {
    let q = SearchQuery::parse("meeting notes friday");
    assert_eq!(q.mode, MatchMode::Partial);
    assert_eq!(q.text.as_deref(), Some("meeting notes friday"));
    assert!(q.source.is_none());
}

/// Latency benchmark (NFR-PERF-003): a lexical query over 1k documents should return
/// quickly. Bound is generous so it holds in an unoptimised debug build; the elapsed
/// time is printed (run with `--nocapture`) for tracking.
#[test]
fn search_latency_1k_docs() {
    let dir = tempdir().unwrap();
    let engine = SearchEngine::open(&dir.path().join("s.db")).unwrap();

    for i in 0..1000 {
        let doc = IndexDocument {
            source: "memory".to_string(),
            source_id: i.to_string(),
            category: "note".to_string(),
            title: format!("Document number {i}"),
            content: format!("This is the body of note {i} about topic {}", i % 20),
            tags: vec![format!("t{}", i % 10)],
            source_field: "manual".to_string(),
            metadata: "{}".to_string(),
            created_at: i as i64,
            updated_at: i as i64,
            importance: i % 100,
            embedding: None,
        };
        engine.insert(&doc).unwrap();
    }

    let start = Instant::now();
    let results = engine.search(&SearchQuery::partial("topic")).unwrap();
    let elapsed = start.elapsed();

    println!(
        "search over 1000 docs took {elapsed:?}, {} hits",
        results.len()
    );
    assert!(!results.is_empty());
    assert!(
        elapsed.as_millis() < 500,
        "search latency {elapsed:?} exceeded 500ms budget"
    );
}
