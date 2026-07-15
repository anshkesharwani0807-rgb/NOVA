use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use parking_lot::RwLock;

use crate::error::KnowledgeError;
use crate::graph::{GraphEntity, KnowledgeGraph, KnowledgeRelationship};

#[derive(Debug, Clone)]
pub struct PathResult {
    pub path: Vec<PathNode>,
    pub total_strength: f64,
    pub total_distance: usize,
    pub path_type: String,
}

#[derive(Debug, Clone)]
pub struct PathNode {
    pub entity_id: String,
    pub entity_name: String,
    pub relationship_type: String,
    pub strength: f64,
}

#[derive(Debug, Clone)]
pub struct ReasoningResult {
    pub query: String,
    pub paths: Vec<PathResult>,
    pub expanded_entities: Vec<GraphEntity>,
    pub context_text: String,
    pub citations: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct KnowledgeContext {
    pub context_text: String,
    pub entities: Vec<GraphEntity>,
    pub relationships: Vec<KnowledgeRelationship>,
    pub citations: Vec<String>,
    pub confidence: f64,
}

pub struct KnowledgeReasoner {
    graph: Arc<RwLock<KnowledgeGraph>>,
    max_depth: usize,
}

impl KnowledgeReasoner {
    pub fn new(graph: Arc<RwLock<KnowledgeGraph>>, max_depth: usize) -> Self {
        Self { graph, max_depth }
    }

    pub fn find_paths(
        &self,
        source_id: &str,
        target_id: &str,
        max_paths: usize,
    ) -> Result<Vec<PathResult>, KnowledgeError> {
        if source_id == target_id {
            return Err(KnowledgeError::NoPathFound(
                source_id.to_string(),
                target_id.to_string(),
            ));
        }
        let graph = self.graph.read();
        if !graph.has_entity(source_id) {
            return Err(KnowledgeError::EntityNotFound(source_id.to_string()));
        }
        if !graph.has_entity(target_id) {
            return Err(KnowledgeError::EntityNotFound(target_id.to_string()));
        }

        let mut paths = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(vec![source_id.to_string()]);
        visited.insert(source_id.to_string());

        while let Some(current_path) = queue.pop_front() {
            if paths.len() >= max_paths {
                break;
            }
            if current_path.len() > self.max_depth + 1 {
                continue;
            }

            let last = current_path
                .last()
                .ok_or_else(|| KnowledgeError::ReasoningError("empty path".to_string()))?;

            if last == target_id {
                let mut path_nodes = Vec::new();
                for window in current_path.windows(2) {
                    if let [a, b] = window {
                        let rels = graph.get_relationships_between(a, b);
                        let rel = rels.first().ok_or_else(|| {
                            KnowledgeError::ReasoningError(
                                "missing relationship in path".to_string(),
                            )
                        })?;
                        let entity = graph
                            .get_entity(b)
                            .ok_or_else(|| KnowledgeError::EntityNotFound(b.clone()))?;
                        path_nodes.push(PathNode {
                            entity_id: b.clone(),
                            entity_name: entity.name.clone(),
                            relationship_type: rel.relationship_type.clone(),
                            strength: rel.strength,
                        });
                    }
                }

                let total_strength: f64 = path_nodes.iter().map(|n| n.strength).sum();
                let avg_strength = if path_nodes.is_empty() {
                    0.0
                } else {
                    total_strength / path_nodes.len() as f64
                };

                paths.push(PathResult {
                    path: path_nodes,
                    total_strength: avg_strength,
                    total_distance: current_path.len() - 1,
                    path_type: "shortest".to_string(),
                });
                continue;
            }

            if current_path.len() <= self.max_depth {
                let neighbors = graph.neighbors(last);
                for neighbor in neighbors {
                    if !visited.contains(&neighbor) || neighbor == target_id {
                        let mut new_path = current_path.clone();
                        new_path.push(neighbor.clone());
                        visited.insert(neighbor.clone());
                        queue.push_back(new_path);
                    }
                }
            }
        }

        if paths.is_empty() {
            return Err(KnowledgeError::NoPathFound(
                source_id.to_string(),
                target_id.to_string(),
            ));
        }

        paths.sort_by(|a, b| {
            b.total_strength
                .partial_cmp(&a.total_strength)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(paths)
    }

    pub fn expand_context(
        &self,
        entity_ids: &[String],
        depth: usize,
        max_entities: usize,
    ) -> KnowledgeContext {
        let graph = self.graph.read();
        let mut expanded = HashSet::new();
        let mut entities = Vec::new();
        let mut relationships = Vec::new();
        let mut queue = VecDeque::new();

        for id in entity_ids {
            queue.push_back((id.clone(), 0));
            expanded.insert(id.clone());
        }

        while let Some((current_id, current_depth)) = queue.pop_front() {
            if entities.len() >= max_entities {
                break;
            }
            if let Some(entity) = graph.get_entity(&current_id) {
                entities.push(entity.clone());
            }
            let rels = graph.get_relationships(&current_id);
            for rel in &rels {
                if !relationships
                    .iter()
                    .any(|r: &KnowledgeRelationship| r.id == rel.id)
                {
                    relationships.push((*rel).clone());
                }
                let neighbor = if rel.source_id == current_id {
                    &rel.target_id
                } else {
                    &rel.source_id
                };
                if current_depth < depth && !expanded.contains(neighbor) {
                    expanded.insert(neighbor.clone());
                    queue.push_back((neighbor.clone(), current_depth + 1));
                }
            }
        }

        let context_text = self.build_context_text(&entities, &relationships);
        let citations: Vec<String> = entities
            .iter()
            .map(|e| format!("{} ({})", e.name, e.entity_type))
            .collect();

        let confidence = if entities.is_empty() {
            0.0
        } else {
            entities.iter().map(|e| e.confidence).sum::<f64>() / entities.len() as f64
        };

        KnowledgeContext {
            context_text,
            entities,
            relationships,
            citations,
            confidence,
        }
    }

    pub fn dependency_search(
        &self,
        entity_id: &str,
        dependency_type: &str,
    ) -> Result<Vec<GraphEntity>, KnowledgeError> {
        let graph = self.graph.read();
        if !graph.has_entity(entity_id) {
            return Err(KnowledgeError::EntityNotFound(entity_id.to_string()));
        }
        Ok(graph
            .get_connected_entities_by_type(entity_id, dependency_type)
            .into_iter()
            .cloned()
            .collect())
    }

    pub fn generate_citations(&self, entity_ids: &[String]) -> Vec<String> {
        let graph = self.graph.read();
        let mut citations = Vec::new();
        for id in entity_ids {
            if let Some(entity) = graph.get_entity(id) {
                let rels = graph.get_relationships(id);
                let rel_count = rels.len();
                citations.push(format!(
                    "{} [{}] - {} relationship(s), last seen: {}",
                    entity.name,
                    entity.entity_type,
                    rel_count,
                    chrono::DateTime::from_timestamp_millis(entity.last_seen)
                        .map(|d| d.format("%Y-%m-%d").to_string())
                        .unwrap_or_default()
                ));
            }
        }
        citations
    }

    pub fn graph_expansion_search(
        &self,
        seed_entity_id: &str,
        relationship_types: &[String],
        max_results: usize,
    ) -> Vec<GraphEntity> {
        let graph = self.graph.read();
        let mut result_ids = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(seed_entity_id.to_string());

        while let Some(current) = queue.pop_front() {
            if result_ids.len() >= max_results {
                break;
            }
            let rels = graph.get_relationships(&current);
            for rel in &rels {
                let neighbor = if rel.source_id == current {
                    &rel.target_id
                } else {
                    &rel.source_id
                };
                if (relationship_types.is_empty()
                    || relationship_types.contains(&rel.relationship_type))
                    && !result_ids.contains(neighbor)
                    && neighbor != seed_entity_id
                    && graph.get_entity(neighbor).is_some()
                {
                    result_ids.insert(neighbor.clone());
                    queue.push_back(neighbor.clone());
                }
            }
        }

        result_ids
            .iter()
            .filter_map(|id| graph.get_entity(id).cloned())
            .take(max_results)
            .collect()
    }

    pub fn reason(
        &self,
        query: &str,
        seed_entity_ids: &[String],
        max_depth: usize,
    ) -> ReasoningResult {
        let start = std::time::Instant::now();

        let context = self.expand_context(seed_entity_ids, max_depth, 50);
        let citations = self.generate_citations(seed_entity_ids);

        let mut paths = Vec::new();
        if seed_entity_ids.len() >= 2 {
            for i in 0..seed_entity_ids.len().saturating_sub(1) {
                for j in (i + 1)..seed_entity_ids.len() {
                    if let Ok(mut found) =
                        self.find_paths(&seed_entity_ids[i], &seed_entity_ids[j], 3)
                    {
                        paths.append(&mut found);
                    }
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        ReasoningResult {
            query: query.to_string(),
            paths,
            expanded_entities: context.entities.clone(),
            context_text: context.context_text,
            citations,
            duration_ms,
        }
    }

    fn build_context_text(
        &self,
        entities: &[GraphEntity],
        relationships: &[KnowledgeRelationship],
    ) -> String {
        let mut parts = Vec::new();
        parts.push("Knowledge Graph Context:".to_string());
        parts.push(format!("Entities ({})", entities.len()));
        for e in entities {
            parts.push(format!(
                "  - {} ({}) - confidence: {:.2}",
                e.name, e.entity_type, e.confidence
            ));
        }
        if !relationships.is_empty() {
            parts.push(format!("Relationships ({})", relationships.len()));
            for r in relationships {
                let src_name = entities
                    .iter()
                    .find(|e| e.id == r.source_id)
                    .map(|e| e.name.as_str())
                    .unwrap_or("?");
                let tgt_name = entities
                    .iter()
                    .find(|e| e.id == r.target_id)
                    .map(|e| e.name.as_str())
                    .unwrap_or("?");
                parts.push(format!(
                    "  - {} --[{}]--> {} (strength: {:.2})",
                    src_name, r.relationship_type, tgt_name, r.strength
                ));
            }
        }
        parts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityType;
    use std::collections::HashMap;

    fn make_entity(id: &str, name: &str, etype: EntityType) -> GraphEntity {
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

    fn make_rel(
        id: &str,
        src: &str,
        tgt: &str,
        rtype: &str,
        strength: f64,
    ) -> KnowledgeRelationship {
        let now = chrono::Utc::now().timestamp_millis();
        KnowledgeRelationship {
            id: id.to_string(),
            source_id: src.to_string(),
            target_id: tgt.to_string(),
            relationship_type: rtype.to_string(),
            strength,
            confidence: 0.9,
            first_seen: now,
            last_seen: now,
            provenance: "test".to_string(),
            metadata: HashMap::new(),
        }
    }

    fn setup_graph() -> Arc<RwLock<KnowledgeGraph>> {
        let mut g = KnowledgeGraph::new();
        g.add_entity(make_entity("alice", "Alice", EntityType::Person))
            .unwrap();
        g.add_entity(make_entity("bob", "Bob", EntityType::Person))
            .unwrap();
        g.add_entity(make_entity("carol", "Carol", EntityType::Person))
            .unwrap();
        g.add_entity(make_entity("project", "ProjectX", EntityType::Topic))
            .unwrap();
        g.add_entity(make_entity("doc", "Doc1", EntityType::Document))
            .unwrap();

        g.add_relationship(make_rel("r1", "alice", "bob", "knows", 0.9))
            .unwrap();
        g.add_relationship(make_rel("r2", "bob", "carol", "knows", 0.8))
            .unwrap();
        g.add_relationship(make_rel("r3", "alice", "project", "works_on", 0.95))
            .unwrap();
        g.add_relationship(make_rel("r4", "project", "doc", "produces", 0.7))
            .unwrap();

        Arc::new(RwLock::new(g))
    }

    #[test]
    fn test_find_paths() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let paths = reasoner.find_paths("alice", "carol", 5).unwrap();
        assert!(!paths.is_empty());
        assert!(paths[0].total_distance <= 3);
    }

    #[test]
    fn test_find_paths_no_path() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let result = reasoner.find_paths("alice", "nonexistent", 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_context() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 2);
        let context = reasoner.expand_context(&["alice".to_string()], 2, 10);
        assert!(!context.entities.is_empty());
        assert!(context.entities.iter().any(|e| e.name == "Alice"));
        assert!(!context.context_text.is_empty());
    }

    #[test]
    fn test_dependency_search() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let deps = reasoner.dependency_search("alice", "works_on").unwrap();
        assert!(deps.iter().any(|e| e.name == "ProjectX"));
    }

    #[test]
    fn test_generate_citations() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let citations = reasoner.generate_citations(&["alice".to_string(), "bob".to_string()]);
        assert_eq!(citations.len(), 2);
        assert!(citations[0].contains("Alice"));
    }

    #[test]
    fn test_graph_expansion_search() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let results = reasoner.graph_expansion_search("alice", &["knows".to_string()], 10);
        assert!(!results.is_empty());
        assert!(results.iter().any(|e| e.name == "Bob"));
    }

    #[test]
    fn test_reason() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 3);
        let result = reasoner.reason(
            "Who knows Alice?",
            &["alice".to_string(), "carol".to_string()],
            3,
        );
        assert!(!result.citations.is_empty());
        assert!(!result.context_text.is_empty());
    }

    #[test]
    fn test_expand_context_empty_seeds() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 2);
        let context = reasoner.expand_context(&[], 2, 10);
        assert!(context.entities.is_empty());
        assert_eq!(context.confidence, 0.0);
    }

    #[test]
    fn test_find_paths_self_returns_empty() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let result = reasoner.find_paths("alice", "alice", 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_context_depth_0() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 0);
        let context = reasoner.expand_context(&["alice".to_string()], 0, 10);
        assert_eq!(context.entities.len(), 1);
        assert!(context.entities.iter().any(|e| e.name == "Alice"));
    }

    #[test]
    fn test_dependency_search_nonexistent() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let result = reasoner.dependency_search("nonexistent", "works_on");
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_citations_no_entities() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let citations = reasoner.generate_citations(&[]);
        assert!(citations.is_empty());
    }

    #[test]
    fn test_graph_expansion_search_no_matches() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let results =
            reasoner.graph_expansion_search("alice", &["nonexistent_rel".to_string()], 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_find_paths_max_depth_exceeded() {
        let mut g = KnowledgeGraph::new();
        let now = chrono::Utc::now().timestamp_millis();
        for i in 0..6 {
            g.add_entity(GraphEntity {
                id: format!("e{}", i),
                name: format!("Node{}", i),
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
        }
        for i in 0..5 {
            g.add_relationship(KnowledgeRelationship {
                id: format!("r{}", i),
                source_id: format!("e{}", i),
                target_id: format!("e{}", i + 1),
                relationship_type: "connected".into(),
                strength: 1.0,
                confidence: 1.0,
                first_seen: now,
                last_seen: now,
                provenance: "test".into(),
                metadata: HashMap::new(),
            })
            .unwrap();
        }
        let graph = Arc::new(RwLock::new(g));
        let reasoner = KnowledgeReasoner::new(graph, 2);
        let result = reasoner.find_paths("e0", "e5", 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_context_with_relationships() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 2);
        let context = reasoner.expand_context(&["alice".to_string()], 2, 10);
        assert!(!context.relationships.is_empty());
        assert!(context
            .relationships
            .iter()
            .any(|r| r.relationship_type == "knows" || r.relationship_type == "works_on"));
    }

    #[test]
    fn test_reason_single_entity() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 3);
        let result = reasoner.reason("Alice", &["alice".to_string()], 3);
        assert!(!result.citations.is_empty());
        assert!(!result.context_text.is_empty());
        assert!(result.expanded_entities.iter().any(|e| e.name == "Alice"));
    }

    #[test]
    fn test_generate_citations_with_relationships() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let citations = reasoner.generate_citations(&["alice".to_string()]);
        assert!(!citations.is_empty());
        assert!(citations[0].contains("relationship"));
    }

    #[test]
    fn test_context_confidence_average() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 2);
        let context = reasoner.expand_context(&["alice".to_string()], 1, 5);
        assert!(context.confidence > 0.0);
    }

    #[test]
    fn test_graph_expansion_with_rel_types() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let results = reasoner.graph_expansion_search("alice", &["works_on".to_string()], 10);
        assert!(results.iter().any(|e| e.name == "ProjectX"));
    }

    #[test]
    fn test_expand_context_multiple_seeds() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 2);
        let context = reasoner.expand_context(&["alice".to_string(), "bob".to_string()], 1, 10);
        assert!(context.entities.iter().any(|e| e.name == "Alice"));
        assert!(context.entities.iter().any(|e| e.name == "Bob"));
    }

    #[test]
    fn test_build_context_text_formatting() {
        let graph = setup_graph();
        let reasoner = KnowledgeReasoner::new(graph, 5);
        let context = reasoner.expand_context(&["alice".to_string()], 1, 5);
        assert!(context.context_text.contains("Knowledge Graph Context"));
        assert!(context.context_text.contains("Alice"));
    }
}
