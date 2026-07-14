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
    },
    RelationshipCreated {
        source_entity: String,
        target_entity: String,
        relationship_type: String,
    },
    KnowledgeUpdated {
        entity_id: String,
        change: String,
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
}
