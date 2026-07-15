use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::entity::{EntityType, KnowledgeEntity};
use crate::error::KnowledgeError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEntity {
    pub id: String,
    pub name: String,
    pub entity_type: EntityType,
    pub description: String,
    pub aliases: Vec<String>,
    pub first_seen: i64,
    pub last_seen: i64,
    pub mention_count: u32,
    pub confidence: f64,
    pub metadata: HashMap<String, String>,
}

impl From<KnowledgeEntity> for GraphEntity {
    fn from(e: KnowledgeEntity) -> Self {
        Self {
            id: e.id,
            name: e.name,
            entity_type: e.entity_type,
            description: e.description,
            aliases: e.aliases,
            first_seen: e.first_seen,
            last_seen: e.last_seen,
            mention_count: e.mention_count,
            confidence: e.confidence,
            metadata: e.metadata,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRelationship {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relationship_type: String,
    pub strength: f64,
    pub confidence: f64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub provenance: String,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    entities: HashMap<String, GraphEntity>,
    relationships: HashMap<String, KnowledgeRelationship>,
    adjacency: HashMap<String, HashSet<String>>,
    adjacency_types: HashMap<String, HashMap<String, Vec<String>>>,
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            relationships: HashMap::new(),
            adjacency: HashMap::new(),
            adjacency_types: HashMap::new(),
        }
    }

    pub fn add_entity(&mut self, entity: GraphEntity) -> Result<(), KnowledgeError> {
        if self.entities.contains_key(&entity.id) {
            return Err(KnowledgeError::DuplicateEntity(entity.name));
        }
        let id = entity.id.clone();
        self.adjacency.entry(id.clone()).or_default();
        self.adjacency_types.entry(id.clone()).or_default();
        self.entities.insert(id, entity);
        Ok(())
    }

    pub fn upsert_entity(&mut self, entity: GraphEntity) {
        let id = entity.id.clone();
        self.adjacency.entry(id.clone()).or_default();
        self.adjacency_types.entry(id.clone()).or_default();
        self.entities.insert(id, entity);
    }

    pub fn get_entity(&self, id: &str) -> Option<&GraphEntity> {
        self.entities.get(id)
    }

    pub fn get_entity_mut(&mut self, id: &str) -> Option<&mut GraphEntity> {
        self.entities.get_mut(id)
    }

    pub fn find_entity_by_name(&self, name: &str) -> Option<&GraphEntity> {
        let lower = name.to_lowercase();
        self.entities.values().find(|e| {
            e.name.to_lowercase() == lower || e.aliases.iter().any(|a| a.to_lowercase() == lower)
        })
    }

    pub fn find_entities_by_type(&self, entity_type: &EntityType) -> Vec<&GraphEntity> {
        self.entities
            .values()
            .filter(|e| e.entity_type == *entity_type)
            .collect()
    }

    pub fn search_entities(&self, query: &str) -> Vec<&GraphEntity> {
        let lower = query.to_lowercase();
        self.entities
            .values()
            .filter(|e| {
                e.name.to_lowercase().contains(&lower)
                    || e.entity_type.to_string().contains(&lower)
                    || e.description.to_lowercase().contains(&lower)
                    || e.aliases.iter().any(|a| a.to_lowercase().contains(&lower))
            })
            .collect()
    }

    pub fn add_relationship(&mut self, rel: KnowledgeRelationship) -> Result<(), KnowledgeError> {
        if !self.entities.contains_key(&rel.source_id) {
            return Err(KnowledgeError::EntityNotFound(rel.source_id.clone()));
        }
        if !self.entities.contains_key(&rel.target_id) {
            return Err(KnowledgeError::EntityNotFound(rel.target_id.clone()));
        }
        let id = rel.id.clone();
        let src = rel.source_id.clone();
        let tgt = rel.target_id.clone();
        let rtype = rel.relationship_type.clone();

        self.adjacency
            .entry(src.clone())
            .or_default()
            .insert(tgt.clone());
        self.adjacency
            .entry(tgt.clone())
            .or_default()
            .insert(src.clone());

        self.adjacency_types
            .entry(src.clone())
            .or_default()
            .entry(tgt.clone())
            .or_default()
            .push(rtype.clone());
        self.adjacency_types
            .entry(tgt.clone())
            .or_default()
            .entry(src.clone())
            .or_default()
            .push(rtype);

        self.relationships.insert(id, rel);
        Ok(())
    }

    pub fn get_relationship(&self, id: &str) -> Option<&KnowledgeRelationship> {
        self.relationships.get(id)
    }

    pub fn get_relationships(&self, entity_id: &str) -> Vec<&KnowledgeRelationship> {
        self.relationships
            .values()
            .filter(|r| r.source_id == entity_id || r.target_id == entity_id)
            .collect()
    }

    pub fn get_relationships_between(
        &self,
        src_id: &str,
        tgt_id: &str,
    ) -> Vec<&KnowledgeRelationship> {
        self.relationships
            .values()
            .filter(|r| {
                (r.source_id == src_id && r.target_id == tgt_id)
                    || (r.source_id == tgt_id && r.target_id == src_id)
            })
            .collect()
    }

    pub fn get_connected_entities(&self, entity_id: &str) -> Vec<&GraphEntity> {
        self.adjacency
            .get(entity_id)
            .map(|connected| {
                connected
                    .iter()
                    .filter_map(|id| self.entities.get(id.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_connected_entities_by_type(
        &self,
        entity_id: &str,
        rel_type: &str,
    ) -> Vec<&GraphEntity> {
        self.adjacency_types
            .get(entity_id)
            .map(|edges| {
                let mut result = Vec::new();
                for (neighbor_id, types) in edges {
                    if types.contains(&rel_type.to_string()) {
                        if let Some(entity) = self.entities.get(neighbor_id.as_str()) {
                            result.push(entity);
                        }
                    }
                }
                result
            })
            .unwrap_or_default()
    }

    pub fn update_relationship_strength(
        &mut self,
        id: &str,
        new_strength: f64,
    ) -> Result<(), KnowledgeError> {
        if let Some(rel) = self.relationships.get_mut(id) {
            rel.strength = new_strength;
            rel.last_seen = chrono::Utc::now().timestamp_millis();
            Ok(())
        } else {
            Err(KnowledgeError::RelationshipNotFound(id.to_string()))
        }
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn relationship_count(&self) -> usize {
        self.relationships.len()
    }

    pub fn all_entities(&self) -> Vec<&GraphEntity> {
        self.entities.values().collect()
    }

    pub fn all_relationships(&self) -> Vec<&KnowledgeRelationship> {
        self.relationships.values().collect()
    }

    pub fn remove_entity(&mut self, id: &str) {
        self.entities.remove(id);
        if let Some(connected) = self.adjacency.remove(id) {
            for conn in connected {
                if let Some(adj) = self.adjacency.get_mut(&conn) {
                    adj.remove(id);
                }
                if let Some(adj_types) = self.adjacency_types.get_mut(&conn) {
                    adj_types.remove(id);
                }
            }
        }
        self.adjacency_types.remove(id);
        self.relationships
            .retain(|_, r| r.source_id != id && r.target_id != id);
    }

    pub fn remove_relationship(&mut self, id: &str) -> Result<(), KnowledgeError> {
        let rel = self
            .relationships
            .remove(id)
            .ok_or_else(|| KnowledgeError::RelationshipNotFound(id.to_string()))?;
        if let Some(adj) = self.adjacency.get_mut(&rel.source_id) {
            adj.remove(&rel.target_id);
        }
        if let Some(adj) = self.adjacency.get_mut(&rel.target_id) {
            adj.remove(&rel.source_id);
        }
        if let Some(adj_types) = self.adjacency_types.get_mut(&rel.source_id) {
            if let Some(types) = adj_types.get_mut(&rel.target_id) {
                types.retain(|t| t != &rel.relationship_type);
            }
        }
        Ok(())
    }

    pub fn neighbors(&self, entity_id: &str) -> Vec<String> {
        self.adjacency
            .get(entity_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn has_entity(&self, id: &str) -> bool {
        self.entities.contains_key(id)
    }

    pub fn update_entity_mention(&mut self, id: &str, timestamp: i64) {
        if let Some(entity) = self.entities.get_mut(id) {
            entity.mention_count = entity.mention_count.saturating_add(1);
            entity.last_seen = timestamp;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityType;

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

    #[test]
    fn test_add_and_get_entity() {
        let mut g = KnowledgeGraph::new();
        let e = make_entity("e1", "Test", EntityType::Topic);
        g.add_entity(e).unwrap();
        assert_eq!(g.entity_count(), 1);
        assert!(g.get_entity("e1").is_some());
    }

    #[test]
    fn test_duplicate_entity_errors() {
        let mut g = KnowledgeGraph::new();
        let e1 = make_entity("e1", "Test", EntityType::Topic);
        let e2 = make_entity("e1", "Test2", EntityType::Person);
        g.add_entity(e1).unwrap();
        assert!(g.add_entity(e2).is_err());
    }

    #[test]
    fn test_find_by_name_case_insensitive() {
        let mut g = KnowledgeGraph::new();
        let e = make_entity("e1", "Rust", EntityType::Topic);
        g.add_entity(e).unwrap();
        assert!(g.find_entity_by_name("rust").is_some());
        assert!(g.find_entity_by_name("RUST").is_some());
    }

    #[test]
    fn test_find_by_alias() {
        let mut g = KnowledgeGraph::new();
        let mut e = make_entity("e1", "Rust", EntityType::Topic);
        e.aliases = vec!["rust-lang".into(), "rs".into()];
        g.add_entity(e).unwrap();
        assert!(g.find_entity_by_name("rust-lang").is_some());
        assert!(g.find_entity_by_name("rs").is_some());
    }

    #[test]
    fn test_add_relationship() {
        let mut g = KnowledgeGraph::new();
        let e1 = make_entity("e1", "Alice", EntityType::Person);
        let e2 = make_entity("e2", "ProjectX", EntityType::Topic);
        g.add_entity(e1).unwrap();
        g.add_entity(e2).unwrap();
        let rel = make_rel("r1", "e1", "e2", "works_on", 0.9);
        g.add_relationship(rel).unwrap();
        assert_eq!(g.relationship_count(), 1);
    }

    #[test]
    fn test_add_relationship_missing_entity_errors() {
        let mut g = KnowledgeGraph::new();
        let e1 = make_entity("e1", "Alice", EntityType::Person);
        g.add_entity(e1).unwrap();
        let rel = make_rel("r1", "e1", "e_nonexistent", "knows", 0.5);
        assert!(g.add_relationship(rel).is_err());
    }

    #[test]
    fn test_get_connected_entities() {
        let mut g = KnowledgeGraph::new();
        let e1 = make_entity("e1", "Alice", EntityType::Person);
        let e2 = make_entity("e2", "Bob", EntityType::Person);
        let e3 = make_entity("e3", "ProjectX", EntityType::Topic);
        g.add_entity(e1).unwrap();
        g.add_entity(e2).unwrap();
        g.add_entity(e3).unwrap();
        g.add_relationship(make_rel("r1", "e1", "e2", "knows", 0.8))
            .unwrap();
        g.add_relationship(make_rel("r2", "e1", "e3", "works_on", 0.9))
            .unwrap();
        let connected = g.get_connected_entities("e1");
        assert_eq!(connected.len(), 2);
    }

    #[test]
    fn test_get_connected_by_type() {
        let mut g = KnowledgeGraph::new();
        let e1 = make_entity("e1", "Alice", EntityType::Person);
        let e2 = make_entity("e2", "Bob", EntityType::Person);
        let e3 = make_entity("e3", "ProjectX", EntityType::Topic);
        g.add_entity(e1).unwrap();
        g.add_entity(e2).unwrap();
        g.add_entity(e3).unwrap();
        g.add_relationship(make_rel("r1", "e1", "e2", "knows", 0.8))
            .unwrap();
        g.add_relationship(make_rel("r2", "e1", "e3", "works_on", 0.9))
            .unwrap();
        let by_type = g.get_connected_entities_by_type("e1", "works_on");
        assert_eq!(by_type.len(), 1);
        assert_eq!(by_type[0].name, "ProjectX");
    }

    #[test]
    fn test_remove_entity() {
        let mut g = KnowledgeGraph::new();
        let e1 = make_entity("e1", "Alice", EntityType::Person);
        let e2 = make_entity("e2", "Bob", EntityType::Person);
        g.add_entity(e1).unwrap();
        g.add_entity(e2).unwrap();
        g.add_relationship(make_rel("r1", "e1", "e2", "knows", 0.8))
            .unwrap();
        g.remove_entity("e1");
        assert_eq!(g.entity_count(), 1);
        assert_eq!(g.relationship_count(), 0);
    }

    #[test]
    fn test_remove_relationship() {
        let mut g = KnowledgeGraph::new();
        let e1 = make_entity("e1", "Alice", EntityType::Person);
        let e2 = make_entity("e2", "Bob", EntityType::Person);
        g.add_entity(e1).unwrap();
        g.add_entity(e2).unwrap();
        g.add_relationship(make_rel("r1", "e1", "e2", "knows", 0.8))
            .unwrap();
        assert!(g.remove_relationship("r1").is_ok());
        assert_eq!(g.relationship_count(), 0);
    }

    #[test]
    fn test_search_entities() {
        let mut g = KnowledgeGraph::new();
        let e1 = make_entity("e1", "Rust Programming", EntityType::Topic);
        let e2 = make_entity("e2", "Python", EntityType::Topic);
        g.add_entity(e1).unwrap();
        g.add_entity(e2).unwrap();
        let results = g.search_entities("rust");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_update_relationship_strength() {
        let mut g = KnowledgeGraph::new();
        let e1 = make_entity("e1", "Alice", EntityType::Person);
        let e2 = make_entity("e2", "Bob", EntityType::Person);
        g.add_entity(e1).unwrap();
        g.add_entity(e2).unwrap();
        g.add_relationship(make_rel("r1", "e1", "e2", "knows", 0.5))
            .unwrap();
        g.update_relationship_strength("r1", 0.95).unwrap();
        assert!((g.get_relationship("r1").unwrap().strength - 0.95).abs() < 0.01);
    }

    #[test]
    fn test_relationships_between() {
        let mut g = KnowledgeGraph::new();
        let e1 = make_entity("e1", "Alice", EntityType::Person);
        let e2 = make_entity("e2", "ProjectX", EntityType::Topic);
        g.add_entity(e1).unwrap();
        g.add_entity(e2).unwrap();
        g.add_relationship(make_rel("r1", "e1", "e2", "works_on", 0.9))
            .unwrap();
        let rels = g.get_relationships_between("e1", "e2");
        assert_eq!(rels.len(), 1);
    }

    #[test]
    fn test_find_entities_by_type() {
        let mut g = KnowledgeGraph::new();
        g.add_entity(make_entity("e1", "Alice", EntityType::Person))
            .unwrap();
        g.add_entity(make_entity("e2", "Bob", EntityType::Person))
            .unwrap();
        g.add_entity(make_entity("e3", "ProjectX", EntityType::Topic))
            .unwrap();
        assert_eq!(g.find_entities_by_type(&EntityType::Person).len(), 2);
    }

    #[test]
    fn test_update_entity_mention() {
        let mut g = KnowledgeGraph::new();
        g.add_entity(make_entity("e1", "Test", EntityType::Topic))
            .unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        g.update_entity_mention("e1", now);
        assert_eq!(g.get_entity("e1").unwrap().mention_count, 2);
    }

    #[test]
    fn test_upsert_entity() {
        let mut g = KnowledgeGraph::new();
        let e = make_entity("e1", "Original", EntityType::Topic);
        g.upsert_entity(e);
        assert_eq!(g.entity_count(), 1);
        let e2 = make_entity("e1", "Updated", EntityType::Topic);
        g.upsert_entity(e2);
        assert_eq!(g.entity_count(), 1);
        assert_eq!(g.get_entity("e1").unwrap().name, "Updated");
    }

    #[test]
    fn test_entity_count_zero_on_new() {
        let g = KnowledgeGraph::new();
        assert_eq!(g.entity_count(), 0);
        assert_eq!(g.relationship_count(), 0);
    }

    #[test]
    fn test_find_entities_by_type_none() {
        let g = KnowledgeGraph::new();
        let results = g.find_entities_by_type(&EntityType::Person);
        assert!(results.is_empty());
    }

    #[test]
    fn test_get_relationships_no_entity() {
        let g = KnowledgeGraph::new();
        let rels = g.get_relationships("nonexistent");
        assert!(rels.is_empty());
    }

    #[test]
    fn test_get_connected_entities_no_entity() {
        let g = KnowledgeGraph::new();
        let connected = g.get_connected_entities("nonexistent");
        assert!(connected.is_empty());
    }

    #[test]
    fn test_neighbors_empty() {
        let g = KnowledgeGraph::new();
        let neighbors = g.neighbors("nonexistent");
        assert!(neighbors.is_empty());
    }

    #[test]
    fn test_add_relationship_updates_adjacency() {
        let mut g = KnowledgeGraph::new();
        g.add_entity(make_entity("e1", "Alice", EntityType::Person))
            .unwrap();
        g.add_entity(make_entity("e2", "Bob", EntityType::Person))
            .unwrap();
        g.add_relationship(make_rel("r1", "e1", "e2", "knows", 0.9))
            .unwrap();
        let neighbors_e1 = g.neighbors("e1");
        let neighbors_e2 = g.neighbors("e2");
        assert!(neighbors_e1.contains(&"e2".to_string()));
        assert!(neighbors_e2.contains(&"e1".to_string()));
    }

    #[test]
    fn test_remove_nonexistent_relationship() {
        let mut g = KnowledgeGraph::new();
        assert!(g.remove_relationship("nonexistent").is_err());
    }

    #[test]
    fn test_update_nonexistent_relationship() {
        let mut g = KnowledgeGraph::new();
        assert!(g.update_relationship_strength("nonexistent", 0.5).is_err());
    }

    #[test]
    fn test_get_relationships_between_none() {
        let mut g = KnowledgeGraph::new();
        g.add_entity(make_entity("e1", "Alice", EntityType::Person))
            .unwrap();
        g.add_entity(make_entity("e2", "Bob", EntityType::Person))
            .unwrap();
        let rels = g.get_relationships_between("e1", "e2");
        assert!(rels.is_empty());
    }

    #[test]
    fn test_get_connected_by_type_no_matches() {
        let mut g = KnowledgeGraph::new();
        g.add_entity(make_entity("e1", "Alice", EntityType::Person))
            .unwrap();
        g.add_entity(make_entity("e2", "Bob", EntityType::Person))
            .unwrap();
        g.add_relationship(make_rel("r1", "e1", "e2", "knows", 0.8))
            .unwrap();
        let by_type = g.get_connected_entities_by_type("e1", "works_on");
        assert!(by_type.is_empty());
    }

    #[test]
    fn test_multiple_relationships_between_same_entities() {
        let mut g = KnowledgeGraph::new();
        g.add_entity(make_entity("e1", "Alice", EntityType::Person))
            .unwrap();
        g.add_entity(make_entity("e2", "ProjectX", EntityType::Topic))
            .unwrap();
        g.add_relationship(make_rel("r1", "e1", "e2", "works_on", 0.9))
            .unwrap();
        g.add_relationship(make_rel("r2", "e1", "e2", "manages", 0.7))
            .unwrap();
        let rels = g.get_relationships_between("e1", "e2");
        assert_eq!(rels.len(), 2);
    }

    #[test]
    fn test_has_entity() {
        let mut g = KnowledgeGraph::new();
        g.add_entity(make_entity("e1", "Alice", EntityType::Person))
            .unwrap();
        assert!(g.has_entity("e1"));
        assert!(!g.has_entity("nonexistent"));
    }

    #[test]
    fn test_remove_entity_cleans_relationships() {
        let mut g = KnowledgeGraph::new();
        g.add_entity(make_entity("e1", "Alice", EntityType::Person))
            .unwrap();
        g.add_entity(make_entity("e2", "Bob", EntityType::Person))
            .unwrap();
        g.add_relationship(make_rel("r1", "e1", "e2", "knows", 0.8))
            .unwrap();
        g.remove_entity("e2");
        assert_eq!(g.relationship_count(), 0);
    }

    #[test]
    fn test_to_and_from_knowledge_entity() {
        let now = chrono::Utc::now().timestamp_millis();
        let ke = crate::entity::KnowledgeEntity {
            id: "e1".into(),
            name: "Test".into(),
            entity_type: EntityType::Topic,
            description: "desc".into(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            source: crate::entity::EntitySource::Memory,
            metadata: HashMap::new(),
        };
        let ge: GraphEntity = ke.into();
        assert_eq!(ge.name, "Test");
    }
}
