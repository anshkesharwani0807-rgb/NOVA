use std::collections::HashMap;

use chrono::Utc;
use uuid::Uuid;

use crate::analysis::{EntityType as OldEntityType, ExtractedEntity};
use crate::entity::EntityType;
use crate::graph::{GraphEntity, KnowledgeGraph, KnowledgeRelationship};

pub struct RelationshipEngine {
    known_patterns: Vec<RelationshipPattern>,
}

struct RelationshipPattern {
    source_type: OldEntityType,
    target_type: OldEntityType,
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
                source_type: OldEntityType::Person,
                target_type: OldEntityType::Project,
                relationship_type: "works_on".to_string(),
            },
            RelationshipPattern {
                source_type: OldEntityType::Person,
                target_type: OldEntityType::Idea,
                relationship_type: "had_idea".to_string(),
            },
            RelationshipPattern {
                source_type: OldEntityType::Project,
                target_type: OldEntityType::Technology,
                relationship_type: "uses".to_string(),
            },
            RelationshipPattern {
                source_type: OldEntityType::Person,
                target_type: OldEntityType::Task,
                relationship_type: "assigned".to_string(),
            },
            RelationshipPattern {
                source_type: OldEntityType::Place,
                target_type: OldEntityType::Person,
                relationship_type: "visited".to_string(),
            },
            RelationshipPattern {
                source_type: OldEntityType::Document,
                target_type: OldEntityType::Project,
                relationship_type: "documents".to_string(),
            },
        ];
        Self { known_patterns }
    }

    pub fn detect_relationships(
        &self,
        entities: &[ExtractedEntity],
        graph: &KnowledgeGraph,
    ) -> Vec<KnowledgeRelationship> {
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
                                relationships.push(KnowledgeRelationship {
                                    id: Uuid::new_v4().to_string(),
                                    source_id: src.id.clone(),
                                    target_id: tgt.id.clone(),
                                    relationship_type: pattern.relationship_type.clone(),
                                    strength: (a.confidence + b.confidence) / 2.0,
                                    confidence: 0.8,
                                    first_seen: now,
                                    last_seen: now,
                                    provenance: "analysis".to_string(),
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
                entity_type: EntityType::Topic,
                description: "NOVA personal AI assistant project".to_string(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.9,
                metadata: HashMap::new(),
            });
        }
        if lower.contains("gallery") {
            entities.push(GraphEntity {
                id: Uuid::new_v4().to_string(),
                name: "Gallery".to_string(),
                entity_type: EntityType::Topic,
                description: "Gallery app/project".to_string(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.7,
                metadata: HashMap::new(),
            });
        }
        if text.contains("Rust") {
            entities.push(GraphEntity {
                id: Uuid::new_v4().to_string(),
                name: "Rust".to_string(),
                entity_type: EntityType::Topic,
                description: "Rust programming language".to_string(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.9,
                metadata: HashMap::new(),
            });
        }

        entities
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityType as NewEntityType;

    fn make_extracted(name: &str, etype: OldEntityType, confidence: f64) -> ExtractedEntity {
        ExtractedEntity {
            name: name.to_string(),
            entity_type: etype,
            confidence,
        }
    }

    fn make_graph_entity(id: &str, name: &str, etype: NewEntityType) -> GraphEntity {
        let now = chrono::Utc::now().timestamp_millis();
        GraphEntity {
            id: id.to_string(),
            name: name.to_string(),
            entity_type: etype,
            description: String::new(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_detect_relationships_person_project() {
        let mut graph = KnowledgeGraph::new();
        graph
            .add_entity(make_graph_entity("e1", "Alice", NewEntityType::Person))
            .unwrap();
        graph
            .add_entity(make_graph_entity("e2", "ProjectX", NewEntityType::Topic))
            .unwrap();

        let engine = RelationshipEngine::new();
        let entities = vec![
            make_extracted("Alice", OldEntityType::Person, 0.9),
            make_extracted("ProjectX", OldEntityType::Project, 0.8),
        ];
        let rels = engine.detect_relationships(&entities, &graph);
        assert!(rels.iter().any(|r| r.relationship_type == "works_on"));
    }

    #[test]
    fn test_detect_relationships_person_place() {
        let mut graph = KnowledgeGraph::new();
        graph
            .add_entity(make_graph_entity("e1", "Paris", NewEntityType::Place))
            .unwrap();
        graph
            .add_entity(make_graph_entity("e2", "Alice", NewEntityType::Person))
            .unwrap();

        let engine = RelationshipEngine::new();
        let entities = vec![
            make_extracted("Paris", OldEntityType::Place, 0.7),
            make_extracted("Alice", OldEntityType::Person, 0.9),
        ];
        let rels = engine.detect_relationships(&entities, &graph);
        assert!(rels.iter().any(|r| r.relationship_type == "visited"));
    }

    #[test]
    fn test_relationship_patterns_count() {
        let engine = RelationshipEngine::new();
        // known_patterns field is private but we can verify behavior
        let entities = vec![
            make_extracted("Alice", OldEntityType::Person, 0.9),
            make_extracted("Idea1", OldEntityType::Idea, 0.8),
        ];
        let mut graph = KnowledgeGraph::new();
        graph
            .add_entity(make_graph_entity("e1", "Alice", NewEntityType::Person))
            .unwrap();
        graph
            .add_entity(make_graph_entity("e2", "Idea1", NewEntityType::Topic))
            .unwrap();
        let rels = engine.detect_relationships(&entities, &graph);
        assert!(rels.iter().any(|r| r.relationship_type == "had_idea"));
    }

    #[test]
    fn test_detect_relationships_duplicate_skipped() {
        let mut graph = KnowledgeGraph::new();
        graph
            .add_entity(make_graph_entity("e1", "Alice", NewEntityType::Person))
            .unwrap();
        graph
            .add_entity(make_graph_entity("e2", "ProjectX", NewEntityType::Topic))
            .unwrap();

        let engine = RelationshipEngine::new();
        let entities = vec![
            make_extracted("Alice", OldEntityType::Person, 0.9),
            make_extracted("ProjectX", OldEntityType::Project, 0.8),
        ];
        // First call should create relationship
        let rels1 = engine.detect_relationships(&entities, &graph);
        assert_eq!(rels1.len(), 1);

        // Add the relationship to the graph
        let rel = rels1[0].clone();
        graph.add_relationship(rel).unwrap();

        // Second call should skip duplicate
        let rels2 = engine.detect_relationships(&entities, &graph);
        assert!(rels2.is_empty());
    }
}
