use std::collections::HashMap;

use chrono::Utc;
use uuid::Uuid;

use crate::analysis::{EntityType, ExtractedEntity};
use crate::graph::EntityType as GraphEntityType;
use crate::graph::{GraphEntity, KnowledgeGraph, Relationship};

pub struct RelationshipEngine {
    known_patterns: Vec<RelationshipPattern>,
}

struct RelationshipPattern {
    source_type: EntityType,
    target_type: EntityType,
    relationship_type: String,
}

impl Default for RelationshipEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RelationshipEngine {
    pub fn new() -> Self {
        let known_patterns = vec![
            RelationshipPattern {
                source_type: EntityType::Person,
                target_type: EntityType::Project,
                relationship_type: "works_on".to_string(),
            },
            RelationshipPattern {
                source_type: EntityType::Person,
                target_type: EntityType::Idea,
                relationship_type: "had_idea".to_string(),
            },
            RelationshipPattern {
                source_type: EntityType::Project,
                target_type: EntityType::Technology,
                relationship_type: "uses".to_string(),
            },
            RelationshipPattern {
                source_type: EntityType::Person,
                target_type: EntityType::Task,
                relationship_type: "assigned".to_string(),
            },
            RelationshipPattern {
                source_type: EntityType::Place,
                target_type: EntityType::Person,
                relationship_type: "visited".to_string(),
            },
            RelationshipPattern {
                source_type: EntityType::Document,
                target_type: EntityType::Project,
                relationship_type: "documents".to_string(),
            },
        ];
        Self { known_patterns }
    }

    pub fn detect_relationships(
        &self,
        entities: &[ExtractedEntity],
        graph: &KnowledgeGraph,
    ) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        let now = Utc::now().timestamp_millis();

        for i in 0..entities.len() {
            for j in (i + 1)..entities.len() {
                let a = &entities[i];
                let b = &entities[j];

                for pattern in &self.known_patterns {
                    if (a.entity_type == pattern.source_type
                        && b.entity_type == pattern.target_type)
                        || (a.entity_type == pattern.target_type
                            && b.entity_type == pattern.source_type)
                    {
                        let (source, target) = if a.entity_type == pattern.source_type {
                            (a, b)
                        } else {
                            (b, a)
                        };

                        let source_entity = graph.find_entity_by_name(&source.name);
                        let target_entity = graph.find_entity_by_name(&target.name);

                        if let (Some(src), Some(tgt)) = (source_entity, target_entity) {
                            let exists = graph.get_relationships(&src.id).iter().any(|r| {
                                r.target_id == tgt.id
                                    && r.relationship_type == pattern.relationship_type
                            });
                            if !exists {
                                relationships.push(Relationship {
                                    id: Uuid::new_v4().to_string(),
                                    source_id: src.id.clone(),
                                    target_id: tgt.id.clone(),
                                    relationship_type: pattern.relationship_type.clone(),
                                    strength: (a.confidence + b.confidence) / 2.0,
                                    first_seen: now,
                                    last_seen: now,
                                    metadata: HashMap::new(),
                                });
                            }
                        }
                    }
                }
            }
        }
        relationships
    }

    pub fn infer_entity_from_memory(&self, content: &str, title: &str) -> Vec<GraphEntity> {
        let mut entities = Vec::new();
        let text = format!("{} {}", title, content);
        let lower = text.to_lowercase();
        let now = Utc::now().timestamp_millis();

        if text.contains("NOVA") && !lower.contains("nova") {
            entities.push(GraphEntity {
                id: Uuid::new_v4().to_string(),
                name: "NOVA".to_string(),
                entity_type: GraphEntityType::Project,
                description: "NOVA personal AI assistant project".to_string(),
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                metadata: HashMap::new(),
            });
        }
        if lower.contains("gallery") {
            entities.push(GraphEntity {
                id: Uuid::new_v4().to_string(),
                name: "Gallery".to_string(),
                entity_type: GraphEntityType::Project,
                description: "Gallery app/project".to_string(),
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                metadata: HashMap::new(),
            });
        }
        if text.contains("Rust") {
            entities.push(GraphEntity {
                id: Uuid::new_v4().to_string(),
                name: "Rust".to_string(),
                entity_type: GraphEntityType::Technology,
                description: "Rust programming language".to_string(),
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                metadata: HashMap::new(),
            });
        }

        entities
    }
}
