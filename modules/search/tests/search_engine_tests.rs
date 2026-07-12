use nova_search::document::{IndexDocument, SearchQuery};
use nova_search::engine::SearchEngine;
use tempfile::tempdir;

fn create_test_engine() -> (SearchEngine, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_search.db");
    let engine = SearchEngine::open(&db_path).unwrap();
    (engine, dir)
}

fn sample_doc(id: &str, title: &str, content: &str, tags: Vec<&str>) -> IndexDocument {
    IndexDocument {
        source: "test".to_string(),
        source_id: id.to_string(),
        category: "note".to_string(),
        title: title.to_string(),
        content: content.to_string(),
        tags: tags.into_iter().map(|s| s.to_string()).collect(),
        source_field: "manual".to_string(),
        metadata: "{}".to_string(),
        created_at: 1000,
        updated_at: 2000,
        importance: 50,
        embedding: None,
    }
}

#[test]
fn test_insert_and_search() {
    let (engine, _dir) = create_test_engine();

    let doc = sample_doc(
        "1",
        "Hello World",
        "This is a test document",
        vec!["test", "hello"],
    );
    engine.insert(&doc).unwrap();

    let results = engine.search(&SearchQuery::partial("hello")).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document.title, "Hello World");
}

#[test]
fn test_update() {
    let (engine, _dir) = create_test_engine();

    let mut doc = sample_doc("1", "Original Title", "Original content", vec!["test"]);
    engine.insert(&doc).unwrap();

    doc.title = "Updated Title".to_string();
    doc.content = "Updated content".to_string();
    engine.update(&doc).unwrap();

    let results = engine.search(&SearchQuery::partial("updated")).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document.title, "Updated Title");
}

#[test]
fn test_delete() {
    let (engine, _dir) = create_test_engine();

    let doc = sample_doc("1", "To Delete", "This will be deleted", vec!["delete"]);
    engine.insert(&doc).unwrap();

    let results = engine.search(&SearchQuery::partial("delete")).unwrap();
    assert_eq!(results.len(), 1);

    engine.delete("test", "1").unwrap();

    let results = engine.search(&SearchQuery::partial("delete")).unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_duplicate_prevention() {
    let (engine, _dir) = create_test_engine();

    let doc = sample_doc("1", "Unique", "Content", vec!["unique"]);
    engine.insert(&doc).unwrap();
    engine.insert(&doc).unwrap(); // Second insert should not create duplicate

    let results = engine.search(&SearchQuery::partial("unique")).unwrap();
    assert_eq!(results.len(), 1);

    let stats = engine.stats().unwrap();
    assert_eq!(stats.total, 1);
}

#[test]
fn test_exact_match() {
    let (engine, _dir) = create_test_engine();

    let doc = sample_doc("1", "Exact Match", "Content here", vec![]);
    engine.insert(&doc).unwrap();

    let results = engine.search(&SearchQuery::exact("Exact Match")).unwrap();
    assert_eq!(results.len(), 1);

    let results = engine.search(&SearchQuery::exact("exact match")).unwrap();
    assert_eq!(results.len(), 1); // case insensitive by default
}

#[test]
fn test_prefix_match() {
    let (engine, _dir) = create_test_engine();

    let doc = sample_doc("1", "Prefix Test", "Content", vec![]);
    engine.insert(&doc).unwrap();

    let results = engine.search(&SearchQuery::prefix("Pref")).unwrap();
    assert_eq!(results.len(), 1);

    let results = engine.search(&SearchQuery::prefix("NonExistent")).unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_phrase_match() {
    let (engine, _dir) = create_test_engine();

    let doc = sample_doc("1", "Title", "This is a phrase match test", vec![]);
    engine.insert(&doc).unwrap();

    let results = engine.search(&SearchQuery::phrase("phrase match")).unwrap();
    assert_eq!(results.len(), 1);

    let results = engine.search(&SearchQuery::phrase("not a phrase")).unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_and_or_combine() {
    let (engine, _dir) = create_test_engine();

    engine
        .insert(&sample_doc("1", "Apple", "Red fruit", vec![]))
        .unwrap();
    engine
        .insert(&sample_doc("2", "Banana", "Yellow fruit", vec![]))
        .unwrap();
    engine
        .insert(&sample_doc("3", "Apple Pie", "Delicious dessert", vec![]))
        .unwrap();

    // AND: both terms must match
    let results = engine
        .search(&SearchQuery::partial("apple red").all_words())
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document.title, "Apple");

    // OR: either term matches
    let results = engine
        .search(&SearchQuery::partial("apple banana").any_word())
        .unwrap();
    // "apple" matches "Apple" and "Apple Pie", "banana" matches "Banana"
    assert_eq!(results.len(), 3);
}

#[test]
fn test_tag_filter() {
    let (engine, _dir) = create_test_engine();

    engine
        .insert(&sample_doc("1", "Doc 1", "Content", vec!["tag1", "tag2"]))
        .unwrap();
    engine
        .insert(&sample_doc("2", "Doc 2", "Content", vec!["tag2", "tag3"]))
        .unwrap();
    engine
        .insert(&sample_doc("3", "Doc 3", "Content", vec!["tag3"]))
        .unwrap();

    let results = engine.search(&SearchQuery::new().tag("tag1")).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document.source_id, "1");

    let results = engine.search(&SearchQuery::new().tag("tag2")).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_source_filter() {
    let (engine, _dir) = create_test_engine();

    let mut doc1 = sample_doc("1", "Doc 1", "Content", vec![]);
    doc1.source = "memory".to_string();
    engine.insert(&doc1).unwrap();

    let mut doc2 = sample_doc("2", "Doc 2", "Content", vec![]);
    doc2.source = "gallery".to_string();
    engine.insert(&doc2).unwrap();

    let results = engine.search(&SearchQuery::new().source("memory")).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document.source, "memory");
}

#[test]
fn test_category_filter() {
    let (engine, _dir) = create_test_engine();

    let mut doc1 = sample_doc("1", "Doc 1", "Content", vec![]);
    doc1.category = "note".to_string();
    engine.insert(&doc1).unwrap();

    let mut doc2 = sample_doc("2", "Doc 2", "Content", vec![]);
    doc2.category = "reminder".to_string();
    engine.insert(&doc2).unwrap();

    let results = engine.search(&SearchQuery::new().category("note")).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document.category, "note");
}

#[test]
fn test_date_filter() {
    let (engine, _dir) = create_test_engine();

    let mut doc1 = sample_doc("1", "Old Doc", "Content", vec![]);
    doc1.created_at = 1000;
    engine.insert(&doc1).unwrap();

    let mut doc2 = sample_doc("2", "New Doc", "Content", vec![]);
    doc2.created_at = 5000;
    engine.insert(&doc2).unwrap();

    let results = engine
        .search(&SearchQuery::new().date_range(Some(2000), None))
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document.source_id, "2");

    let results = engine
        .search(&SearchQuery::new().date_range(None, Some(2000)))
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document.source_id, "1");
}

#[test]
fn test_pagination() {
    let (engine, _dir) = create_test_engine();

    for i in 1..=10 {
        engine
            .insert(&sample_doc(
                &i.to_string(),
                &format!("Doc {}", i),
                "Content",
                vec![],
            ))
            .unwrap();
    }

    let results = engine
        .search(&SearchQuery::new().limit(3).offset(0))
        .unwrap();
    assert_eq!(results.len(), 3);

    let results = engine
        .search(&SearchQuery::new().limit(3).offset(3))
        .unwrap();
    assert_eq!(results.len(), 3);

    let results = engine
        .search(&SearchQuery::new().limit(3).offset(6))
        .unwrap();
    assert_eq!(results.len(), 3);

    let results = engine
        .search(&SearchQuery::new().limit(3).offset(9))
        .unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_ranking() {
    let (engine, _dir) = create_test_engine();

    let mut doc1 = sample_doc("1", "Title Match", "Content", vec![]);
    doc1.importance = 10;
    engine.insert(&doc1).unwrap();

    let mut doc2 = sample_doc("2", "Other", "Title Match in content", vec![]);
    doc2.importance = 100;
    engine.insert(&doc2).unwrap();

    let mut doc3 = sample_doc("3", "Another", "Content", vec!["title"]);
    doc3.importance = 50;
    engine.insert(&doc3).unwrap();

    let results = engine.search(&SearchQuery::partial("title")).unwrap();
    assert_eq!(results.len(), 3);

    // Title match should rank highest (score 3.0 + importance)
    assert_eq!(results[0].document.source_id, "1");
}

#[test]
fn test_rebuild() {
    let (engine, _dir) = create_test_engine();

    engine
        .insert(&sample_doc("1", "Doc 1", "Content", vec![]))
        .unwrap();
    engine
        .insert(&sample_doc("2", "Doc 2", "Content", vec![]))
        .unwrap();

    let mut engine = engine;
    let docs = vec![
        sample_doc("3", "Doc 3", "Content", vec![]),
        sample_doc("4", "Doc 4", "Content", vec![]),
    ];
    let count = engine.rebuild(&docs).unwrap();
    assert_eq!(count, 2);

    let stats = engine.stats().unwrap();
    assert_eq!(stats.total, 2);

    let results = engine.search(&SearchQuery::new()).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_clear() {
    let (engine, _dir) = create_test_engine();

    engine
        .insert(&sample_doc("1", "Doc 1", "Content", vec![]))
        .unwrap();
    engine
        .insert(&sample_doc("2", "Doc 2", "Content", vec![]))
        .unwrap();

    engine.clear().unwrap();

    let stats = engine.stats().unwrap();
    assert_eq!(stats.total, 0);
}

#[test]
fn test_stats() {
    let (engine, _dir) = create_test_engine();

    engine
        .insert(&sample_doc("1", "Doc 1", "Content", vec![]))
        .unwrap();
    engine
        .insert(&sample_doc("2", "Doc 2", "Content", vec![]))
        .unwrap();

    let mut doc3 = sample_doc("3", "Doc 3", "Content", vec![]);
    doc3.source = "gallery".to_string();
    engine.insert(&doc3).unwrap();

    let stats = engine.stats().unwrap();
    assert_eq!(stats.total, 3);
    assert_eq!(stats.sources.len(), 2); // "test" and "gallery"
}

#[test]
fn test_case_insensitive() {
    let (engine, _dir) = create_test_engine();

    engine
        .insert(&sample_doc("1", "Hello World", "Content", vec![]))
        .unwrap();

    let results = engine.search(&SearchQuery::partial("HELLO")).unwrap();
    assert_eq!(results.len(), 1);

    let results = engine.search(&SearchQuery::partial("hello")).unwrap();
    assert_eq!(results.len(), 1);

    let results = engine
        .search(&SearchQuery::partial("Hello").case_sensitive())
        .unwrap();
    assert_eq!(results.len(), 1);

    let results = engine
        .search(&SearchQuery::partial("HELLO").case_sensitive())
        .unwrap();
    assert_eq!(results.len(), 0); // case sensitive, no match
}

#[test]
fn test_incremental_indexing() {
    let (engine, _dir) = create_test_engine();

    // Initial inserts
    engine
        .insert(&sample_doc("1", "First", "Content", vec![]))
        .unwrap();
    engine
        .insert(&sample_doc("2", "Second", "Content", vec![]))
        .unwrap();

    // Update one
    let mut doc = sample_doc("1", "First Updated", "Content", vec![]);
    doc.updated_at = 3000;
    engine.update(&doc).unwrap();

    // Add another
    engine
        .insert(&sample_doc("3", "Third", "Content", vec![]))
        .unwrap();

    // Delete one
    engine.delete("test", "2").unwrap();

    let results = engine.search(&SearchQuery::new()).unwrap();
    assert_eq!(results.len(), 2); // 1 and 3

    let results = engine.search(&SearchQuery::partial("updated")).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_schema_version() {
    let (engine, _dir) = create_test_engine();
    let version = engine.schema_version().unwrap();
    assert_eq!(version, 2);
}

#[test]
fn test_multi_word_search() {
    let (engine, _dir) = create_test_engine();

    engine
        .insert(&sample_doc("1", "Red Apple", "Green fruit", vec![]))
        .unwrap();
    engine
        .insert(&sample_doc("2", "Apple Pie", "Delicious", vec![]))
        .unwrap();
    engine
        .insert(&sample_doc("3", "Red Car", "Fast", vec![]))
        .unwrap();

    // Both words must appear (AND)
    let results = engine
        .search(&SearchQuery::partial("red apple").all_words())
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document.source_id, "1");

    // Either word (OR)
    let results = engine
        .search(&SearchQuery::partial("red apple").any_word())
        .unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn test_search_with_special_chars() {
    let (engine, _dir) = create_test_engine();

    engine
        .insert(&sample_doc("1", "Test%", "Content with % and _", vec![]))
        .unwrap();

    // Should match literally due to escaping
    let results = engine.search(&SearchQuery::partial("Test%")).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_empty_search() {
    let (engine, _dir) = create_test_engine();

    engine
        .insert(&sample_doc("1", "Doc", "Content", vec![]))
        .unwrap();

    let results = engine.search(&SearchQuery::new()).unwrap();
    assert_eq!(results.len(), 1);
}
