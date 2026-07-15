use std::collections::HashMap;
use std::sync::Arc;

use nova_kernel::EventBus;
use nova_knowledge::{
    CombinedRanker, EntitySource, EntityType, InMemoryStorage, JsonFileStorage, KnowledgeEngine,
    KnowledgeEntity, KnowledgeEventPayload, KnowledgeGraph, KnowledgeStorage, RankWeights,
    RankedResult, Ranker,
};

fn make_engine() -> KnowledgeEngine {
    let engine = KnowledgeEngine::new();
    let bus = Arc::new(EventBus::new(1024));
    engine.set_event_bus(bus);
    engine
}

#[tokio::test]
async fn test_engine_extract_and_add_entity() {
    let engine = make_engine();
    let text = "Alice is working on a Rust project";
    let entities = engine.extract_entities_from_text(text, "test", EntitySource::Memory);
    assert!(!entities.is_empty());
    let alice = entities.iter().find(|e| e.name == "Alice");
    assert!(alice.is_some());
}

#[tokio::test]
async fn test_engine_add_entity_to_graph_and_retrieve() {
    let engine = make_engine();
    let entity = KnowledgeEntity {
        id: "test1".into(),
        name: "Alice".into(),
        entity_type: EntityType::Person,
        description: "A person".into(),
        aliases: vec!["Ali".into()],
        first_seen: 1000,
        last_seen: 1000,
        mention_count: 1,
        confidence: 0.9,
        source: EntitySource::Memory,
        metadata: HashMap::new(),
    };
    let graph_entity = engine.add_entity_to_graph(entity).unwrap();
    assert_eq!(graph_entity.name, "Alice");

    let found = engine.get_entity_by_name("Alice");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "Alice");
}

#[tokio::test]
async fn test_engine_add_duplicate_entity_merges() {
    let engine = make_engine();
    let e1 = KnowledgeEntity {
        id: "e1".into(),
        name: "Alice".into(),
        entity_type: EntityType::Person,
        description: "First".into(),
        aliases: vec![],
        first_seen: 1000,
        last_seen: 1000,
        mention_count: 1,
        confidence: 0.8,
        source: EntitySource::Memory,
        metadata: HashMap::new(),
    };
    engine.add_entity_to_graph(e1).unwrap();
    let e2 = KnowledgeEntity {
        id: "e2".into(),
        name: "Alice".into(),
        entity_type: EntityType::Person,
        description: "Second longer description".into(),
        aliases: vec!["Ali".into()],
        first_seen: 1000,
        last_seen: 2000,
        mention_count: 3,
        confidence: 0.9,
        source: EntitySource::Memory,
        metadata: HashMap::new(),
    };
    let merged = engine.add_entity_to_graph(e2).unwrap();
    assert_eq!(merged.mention_count, 4);
}

#[tokio::test]
async fn test_engine_relationships() {
    let engine = make_engine();
    let alice = engine
        .add_entity_to_graph(KnowledgeEntity {
            id: "a".into(),
            name: "Alice".into(),
            entity_type: EntityType::Person,
            description: "".into(),
            aliases: vec![],
            first_seen: 1000,
            last_seen: 1000,
            mention_count: 1,
            confidence: 0.9,
            source: EntitySource::Memory,
            metadata: HashMap::new(),
        })
        .unwrap();
    let rust = engine
        .add_entity_to_graph(KnowledgeEntity {
            id: "r".into(),
            name: "Rust".into(),
            entity_type: EntityType::Topic,
            description: "".into(),
            aliases: vec![],
            first_seen: 1000,
            last_seen: 1000,
            mention_count: 1,
            confidence: 0.9,
            source: EntitySource::Memory,
            metadata: HashMap::new(),
        })
        .unwrap();

    let rel_id = engine
        .add_relationship(&alice.id, &rust.id, "uses", 0.9, "test")
        .unwrap();
    assert!(!rel_id.is_empty());

    let connected = engine.get_connected_entities(&alice.id);
    assert_eq!(connected.len(), 1);
    assert_eq!(connected[0].name, "Rust");
}

#[tokio::test]
async fn test_engine_semantic_index_and_search() {
    let engine = make_engine();
    let entity = KnowledgeEntity {
        id: "e1".into(),
        name: "Rust Programming".into(),
        entity_type: EntityType::Topic,
        description: "A systems language".into(),
        aliases: vec![],
        first_seen: 1000,
        last_seen: 1000,
        mention_count: 1,
        confidence: 0.9,
        source: EntitySource::Memory,
        metadata: HashMap::new(),
    };
    engine.index_entity_for_search(&entity).await.unwrap();
    let results = engine.semantic_search("rust", 10, None).await.unwrap();
    assert!(!results.is_empty());
}

#[tokio::test]
async fn test_engine_hybrid_search() {
    let engine = make_engine();
    let entity = KnowledgeEntity {
        id: "e1".into(),
        name: "Rust".into(),
        entity_type: EntityType::Topic,
        description: "Programming language".into(),
        aliases: vec![],
        first_seen: 1000,
        last_seen: 1000,
        mention_count: 1,
        confidence: 0.9,
        source: EntitySource::Memory,
        metadata: HashMap::new(),
    };
    engine.add_entity_to_graph(entity).unwrap();
    let results = engine.hybrid_search("Rust", 10).await.unwrap();
    assert!(!results.is_empty());
}

#[tokio::test]
async fn test_engine_reasoning() {
    let engine = make_engine();
    let alice = engine
        .add_entity_to_graph(KnowledgeEntity {
            id: "a".into(),
            name: "Alice".into(),
            entity_type: EntityType::Person,
            description: "".into(),
            aliases: vec![],
            first_seen: 1000,
            last_seen: 1000,
            mention_count: 1,
            confidence: 0.9,
            source: EntitySource::Memory,
            metadata: HashMap::new(),
        })
        .unwrap();
    let bob = engine
        .add_entity_to_graph(KnowledgeEntity {
            id: "b".into(),
            name: "Bob".into(),
            entity_type: EntityType::Person,
            description: "".into(),
            aliases: vec![],
            first_seen: 1000,
            last_seen: 1000,
            mention_count: 1,
            confidence: 0.9,
            source: EntitySource::Memory,
            metadata: HashMap::new(),
        })
        .unwrap();
    let project = engine
        .add_entity_to_graph(KnowledgeEntity {
            id: "p".into(),
            name: "ProjectX".into(),
            entity_type: EntityType::Topic,
            description: "".into(),
            aliases: vec![],
            first_seen: 1000,
            last_seen: 1000,
            mention_count: 1,
            confidence: 0.9,
            source: EntitySource::Memory,
            metadata: HashMap::new(),
        })
        .unwrap();

    engine
        .add_relationship(&alice.id, &bob.id, "knows", 0.9, "test")
        .unwrap();
    engine
        .add_relationship(&alice.id, &project.id, "works_on", 0.95, "test")
        .unwrap();

    let paths = engine.find_paths(&alice.id, &project.id, 5).unwrap();
    assert!(!paths.is_empty());

    let context = engine.build_knowledge_context("ProjectX", 5);
    assert!(!context.entities.is_empty());

    let reason = engine.reason("Who knows Alice?", &[alice.id], 3).unwrap();
    assert!(!reason.citations.is_empty());
}

#[tokio::test]
async fn test_engine_persistence_roundtrip() {
    let dir = std::env::temp_dir().join(format!("nova-kg-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();

    let engine = make_engine();
    engine
        .add_entity_to_graph(KnowledgeEntity {
            id: "e1".into(),
            name: "Alice".into(),
            entity_type: EntityType::Person,
            description: "".into(),
            aliases: vec![],
            first_seen: 1000,
            last_seen: 1000,
            mention_count: 1,
            confidence: 0.9,
            source: EntitySource::Memory,
            metadata: HashMap::new(),
        })
        .unwrap();
    engine.set_storage(Arc::new(InMemoryStorage::new()));
    engine.save().await.unwrap();
    engine.load().await.unwrap();
    let found = engine.get_entity_by_name("Alice");
    assert!(found.is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_engine_extract_from_multiple_sources() {
    let engine = make_engine();
    let text_entities = engine.extract_entities_from_text(
        "Meeting about Rust at Google",
        "title",
        EntitySource::Memory,
    );
    assert!(!text_entities.is_empty());

    let ocr_entities = engine.extract_entities_from_ocr("Visit https://rust-lang.org");
    assert!(ocr_entities
        .iter()
        .any(|e| e.entity_type == EntityType::Website));

    let conv_entities = engine.extract_entities_from_conversation("Hello from Alice", Some("Bob"));
    assert!(conv_entities.iter().any(|e| e.name == "Bob"));
}

#[tokio::test]
async fn test_engine_permission_check() {
    let engine = make_engine();
    assert!(engine.check_permission("knowledge.read"));
    assert!(engine.check_permission("knowledge.write"));
    assert!(engine.check_permission("knowledge.reason"));
    assert!(engine.check_permission("knowledge.index"));
    assert!(!engine.check_permission("unknown.permission"));
}

#[test]
fn test_combined_ranker_custom_weights() {
    let weights = RankWeights {
        entity_relevance: 0.5,
        graph_distance: 0.0,
        embedding_score: 0.5,
        keyword_score: 0.0,
        recency: 0.0,
        confidence: 0.0,
    };
    let ranker = CombinedRanker::with_weights(weights);
    let results = vec![
        RankedResult {
            entity_id: "1".into(),
            name: "Low".into(),
            score: 0.0,
            entity_relevance: 0.1,
            graph_distance: 0.5,
            embedding_score: 0.1,
            keyword_score: 0.1,
            recency_score: 0.1,
            confidence_score: 0.1,
            details: HashMap::new(),
        },
        RankedResult {
            entity_id: "2".into(),
            name: "High".into(),
            score: 0.0,
            entity_relevance: 0.9,
            graph_distance: 0.5,
            embedding_score: 0.9,
            keyword_score: 0.1,
            recency_score: 0.1,
            confidence_score: 0.1,
            details: HashMap::new(),
        },
    ];
    let ranked = ranker.rank(results);
    assert_eq!(ranked[0].name, "High");
    assert!(ranked[0].score > ranked[1].score);
}

#[test]
fn test_event_payload_clone_and_debug() {
    let payload = KnowledgeEventPayload::EntityCreated {
        entity_id: "e1".into(),
        entity_type: "person".into(),
        name: "Alice".into(),
        source: "memory".into(),
    };
    let cloned = payload.clone();
    assert!(format!("{:?}", cloned).contains("EntityCreated"));
}

#[tokio::test]
async fn test_engine_search_entities_in_graph() {
    let engine = make_engine();
    engine
        .add_entity_to_graph(KnowledgeEntity {
            id: "e1".into(),
            name: "Rust Programming".into(),
            entity_type: EntityType::Topic,
            description: "".into(),
            aliases: vec![],
            first_seen: 1000,
            last_seen: 1000,
            mention_count: 1,
            confidence: 0.9,
            source: EntitySource::Memory,
            metadata: HashMap::new(),
        })
        .unwrap();
    let results = engine.search_entities_in_graph("rust");
    assert!(!results.is_empty());
    assert!(results.iter().any(|e| e.name.contains("Rust")));
}

#[test]
fn test_knowledge_graph_clone() {
    let mut g = KnowledgeGraph::new();
    let now = chrono::Utc::now().timestamp_millis();
    g.add_entity(nova_knowledge::GraphEntity {
        id: "e1".into(),
        name: "Test".into(),
        entity_type: EntityType::Topic,
        description: "".into(),
        aliases: vec![],
        first_seen: now,
        last_seen: now,
        mention_count: 1,
        confidence: 0.9,
        metadata: HashMap::new(),
    })
    .unwrap();
    let cloned = g.clone();
    assert_eq!(cloned.entity_count(), 1);
}

#[tokio::test]
async fn test_engine_empty_graph_operations() {
    let engine = make_engine();
    let entity = engine.get_entity_by_name("nonexistent");
    assert!(entity.is_none());

    let connected = engine.get_connected_entities("nonexistent");
    assert!(connected.is_empty());

    let results = engine.semantic_search("anything", 10, None).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_engine_event_publication() {
    let engine = make_engine();
    let bus = Arc::new(EventBus::new(1024));
    engine.set_event_bus(bus.clone());

    let entity = KnowledgeEntity {
        id: "e1".into(),
        name: "Test Entity".into(),
        entity_type: EntityType::Topic,
        description: "".into(),
        aliases: vec![],
        first_seen: 1000,
        last_seen: 1000,
        mention_count: 1,
        confidence: 0.9,
        source: EntitySource::Memory,
        metadata: HashMap::new(),
    };

    let graph_entity = engine.add_entity_to_graph(entity).unwrap();
    assert_eq!(graph_entity.name, "Test Entity");
}

#[tokio::test]
async fn test_json_storage_file_creation() {
    let dir = std::env::temp_dir().join(format!("nova-json-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let storage = JsonFileStorage::new(&dir);

    let mut graph = KnowledgeGraph::new();
    let now = chrono::Utc::now().timestamp_millis();
    graph
        .add_entity(nova_knowledge::GraphEntity {
            id: "e1".into(),
            name: "Test".into(),
            entity_type: EntityType::Topic,
            description: "".into(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            metadata: HashMap::new(),
        })
        .unwrap();
    storage.save_graph(&graph).await.unwrap();

    let graph_path = dir.join("knowledge_graph.json");
    assert!(graph_path.exists());

    let loaded = storage.load_graph().await.unwrap();
    assert_eq!(loaded.entity_count(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}
