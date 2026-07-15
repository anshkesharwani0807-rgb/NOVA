use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KnowledgeEventPayload {
    MemoryAnalyzed {
        memory_id: String,
        category: String,
        tags: Vec<String>,
        importance: i32,
    },
    MemoryLinked {
        source_id: String,
        target_id: String,
        link_type: String,
    },
    EntityCreated {
        entity_id: String,
        entity_type: String,
        name: String,
        source: String,
    },
    EntityUpdated {
        entity_id: String,
        entity_type: String,
        name: String,
        change: String,
    },
    EntityDeleted {
        entity_id: String,
    },
    RelationshipCreated {
        source_entity: String,
        target_entity: String,
        relationship_type: String,
        strength: f64,
    },
    RelationshipDeleted {
        source_entity: String,
        target_entity: String,
        relationship_type: String,
    },
    KnowledgeUpdated {
        entity_id: String,
        change: String,
    },
    KnowledgeIndexed {
        entity_count: usize,
        relationship_count: usize,
        duration_ms: u64,
    },
    KnowledgeSearchCompleted {
        query: String,
        result_count: usize,
        duration_ms: u64,
    },
    KnowledgeReasoningCompleted {
        query: String,
        path_count: usize,
        duration_ms: u64,
    },
    TimelineGenerated {
        granularity: String,
        entry_count: usize,
        time_range: String,
    },
    SummaryCreated {
        summary_type: String,
        target_id: String,
        length: usize,
    },
    RecallCompleted {
        query: String,
        result_count: usize,
    },
    GraphUpdated {
        entities_added: usize,
        relationships_added: usize,
    },
    KnowledgeFailed {
        operation: String,
        error: String,
    },
}

impl KnowledgeEventPayload {
    pub fn action_name(&self) -> &'static str {
        match self {
            Self::MemoryAnalyzed { .. } => "knowledge.memory_analyzed",
            Self::MemoryLinked { .. } => "knowledge.memory_linked",
            Self::EntityCreated { .. } => "knowledge.entity_created",
            Self::EntityUpdated { .. } => "knowledge.entity_updated",
            Self::EntityDeleted { .. } => "knowledge.entity_deleted",
            Self::RelationshipCreated { .. } => "knowledge.relationship_created",
            Self::RelationshipDeleted { .. } => "knowledge.relationship_deleted",
            Self::KnowledgeUpdated { .. } => "knowledge.updated",
            Self::KnowledgeIndexed { .. } => "knowledge.indexed",
            Self::KnowledgeSearchCompleted { .. } => "knowledge.search",
            Self::KnowledgeReasoningCompleted { .. } => "knowledge.reasoning",
            Self::TimelineGenerated { .. } => "knowledge.timeline_generated",
            Self::SummaryCreated { .. } => "knowledge.summary_created",
            Self::RecallCompleted { .. } => "knowledge.recall_completed",
            Self::GraphUpdated { .. } => "knowledge.graph_updated",
            Self::KnowledgeFailed { .. } => "knowledge.failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_action_names() {
        let cases = vec![
            (
                KnowledgeEventPayload::MemoryAnalyzed {
                    memory_id: "".into(),
                    category: "".into(),
                    tags: vec![],
                    importance: 0,
                },
                "knowledge.memory_analyzed",
            ),
            (
                KnowledgeEventPayload::MemoryLinked {
                    source_id: "".into(),
                    target_id: "".into(),
                    link_type: "".into(),
                },
                "knowledge.memory_linked",
            ),
            (
                KnowledgeEventPayload::EntityCreated {
                    entity_id: "".into(),
                    entity_type: "".into(),
                    name: "".into(),
                    source: "".into(),
                },
                "knowledge.entity_created",
            ),
            (
                KnowledgeEventPayload::EntityUpdated {
                    entity_id: "".into(),
                    entity_type: "".into(),
                    name: "".into(),
                    change: "".into(),
                },
                "knowledge.entity_updated",
            ),
            (
                KnowledgeEventPayload::EntityDeleted {
                    entity_id: "".into(),
                },
                "knowledge.entity_deleted",
            ),
            (
                KnowledgeEventPayload::RelationshipCreated {
                    source_entity: "".into(),
                    target_entity: "".into(),
                    relationship_type: "".into(),
                    strength: 0.0,
                },
                "knowledge.relationship_created",
            ),
            (
                KnowledgeEventPayload::RelationshipDeleted {
                    source_entity: "".into(),
                    target_entity: "".into(),
                    relationship_type: "".into(),
                },
                "knowledge.relationship_deleted",
            ),
            (
                KnowledgeEventPayload::KnowledgeUpdated {
                    entity_id: "".into(),
                    change: "".into(),
                },
                "knowledge.updated",
            ),
            (
                KnowledgeEventPayload::KnowledgeIndexed {
                    entity_count: 0,
                    relationship_count: 0,
                    duration_ms: 0,
                },
                "knowledge.indexed",
            ),
            (
                KnowledgeEventPayload::KnowledgeSearchCompleted {
                    query: "".into(),
                    result_count: 0,
                    duration_ms: 0,
                },
                "knowledge.search",
            ),
            (
                KnowledgeEventPayload::KnowledgeReasoningCompleted {
                    query: "".into(),
                    path_count: 0,
                    duration_ms: 0,
                },
                "knowledge.reasoning",
            ),
            (
                KnowledgeEventPayload::TimelineGenerated {
                    granularity: "".into(),
                    entry_count: 0,
                    time_range: "".into(),
                },
                "knowledge.timeline_generated",
            ),
            (
                KnowledgeEventPayload::SummaryCreated {
                    summary_type: "".into(),
                    target_id: "".into(),
                    length: 0,
                },
                "knowledge.summary_created",
            ),
            (
                KnowledgeEventPayload::RecallCompleted {
                    query: "".into(),
                    result_count: 0,
                },
                "knowledge.recall_completed",
            ),
            (
                KnowledgeEventPayload::GraphUpdated {
                    entities_added: 0,
                    relationships_added: 0,
                },
                "knowledge.graph_updated",
            ),
            (
                KnowledgeEventPayload::KnowledgeFailed {
                    operation: "".into(),
                    error: "".into(),
                },
                "knowledge.failed",
            ),
        ];
        for (payload, expected) in cases {
            assert_eq!(
                payload.action_name(),
                expected,
                "mismatch for {:?}",
                payload
            );
        }
    }

    #[test]
    fn test_event_payload_serialization() {
        let payload = KnowledgeEventPayload::EntityCreated {
            entity_id: "e1".into(),
            entity_type: "person".into(),
            name: "Alice".into(),
            source: "memory".into(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: KnowledgeEventPayload = serde_json::from_str(&json).unwrap();
        match deserialized {
            KnowledgeEventPayload::EntityCreated {
                entity_id,
                entity_type,
                name,
                source,
            } => {
                assert_eq!(entity_id, "e1");
                assert_eq!(entity_type, "person");
                assert_eq!(name, "Alice");
                assert_eq!(source, "memory");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_knowledge_search_event_fields() {
        let payload = KnowledgeEventPayload::KnowledgeSearchCompleted {
            query: "rust".into(),
            result_count: 5,
            duration_ms: 42,
        };
        assert_eq!(payload.action_name(), "knowledge.search");
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("rust"));
        assert!(json.contains("5"));
        assert!(json.contains("42"));
    }
}
