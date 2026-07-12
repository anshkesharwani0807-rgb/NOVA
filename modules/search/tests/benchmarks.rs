//! Search Engine Performance Benchmarks (NFR-PERF-003).
//!
//! This benchmark suite measures:
//! 1. Indexing latency (Initial and Incremental).
//! 2. Deletion latency.
//! 3. Search latency across different dataset sizes.

use nova_search::{IndexDocument, SearchEngine, SearchQuery};
use std::time::Instant;
use tempfile::tempdir;

/// Generates a set of dummy documents for benchmarking.
fn generate_docs(count: usize) -> Vec<IndexDocument> {
    (0..count)
        .map(|i| IndexDocument {
            source: "benchmark".to_string(),
            source_id: format!("doc_{}", i),
            category: "general".to_string(),
            title: format!("Document Title {}", i),
            content: format!("This is the content for document number {}. It contains some keywords for searching.", i),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            source_field: "benchmark_field".to_string(),
            metadata: "{}".to_string(),
            created_at: 1600000000 + i as i64,
            updated_at: 1600000000 + i as i64,
            importance: (i % 10) as i32,
            embedding: Some(vec![0.1f32; 384]),
        })
        .collect()
}

#[test]
fn test_performance_benchmarks() {
    let dir = tempdir().expect("Failed to create temp dir");
    let db_path = dir.path().join("bench_search.db");
    let engine = SearchEngine::open(&db_path).expect("Failed to open engine");

    println!("--- M5 Performance Benchmarks (NFR-PERF-003) ---");

    // 1. Indexing Time
    let doc_count = 1000;
    let docs = generate_docs(doc_count);
    let start = Instant::now();
    for doc in &docs {
        engine.insert(doc).expect("Insert failed");
    }
    let duration = start.elapsed();
    println!(
        "Indexing {} records: {:?} (avg {:?}/record)",
        doc_count,
        duration,
        duration / doc_count as u32
    );

    // 2. Incremental Update Time
    let update_doc = IndexDocument {
        source: "benchmark".to_string(),
        source_id: "doc_0".to_string(),
        category: "updated".to_string(),
        title: "Updated Title".to_string(),
        content: "Updated content".to_string(),
        tags: vec!["updated".to_string()],
        source_field: "benchmark_field".to_string(),
        metadata: "{}".to_string(),
        created_at: 1600000000,
        updated_at: 1700000000,
        importance: 5,
        embedding: Some(vec![0.2f32; 384]),
    };
    let start = Instant::now();
    engine.update(&update_doc).expect("Update failed");
    let duration = start.elapsed();
    println!("Incremental update (1 record): {:?}", duration);

    // 3. Delete Time
    let start = Instant::now();
    engine.delete("benchmark", "doc_1").expect("Delete failed");
    let duration = start.elapsed();
    println!("Delete (1 record): {:?}", duration);

    // 4. Search Latency
    let dataset_sizes = [100, 1000, 10000];
    for size in dataset_sizes {
        // Clear and prepare dataset
        engine.clear().expect("Clear failed");
        let docs = generate_docs(size);
        for doc in &docs {
            engine.insert(doc).expect("Insert failed");
        }

        let query = SearchQuery::partial("Document Title 5");
        let start = Instant::now();
        let results = engine.search(&query).expect("Search failed");
        let duration = start.elapsed();

        println!(
            "Search latency ({} records): {:?} [Results: {}]",
            size,
            duration,
            results.len()
        );
    }

    println!("--------------------------------------------------");
}
