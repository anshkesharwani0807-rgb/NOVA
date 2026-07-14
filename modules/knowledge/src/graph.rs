use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::KnowledgeError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EntityType {
    Person,
    Place,
    Project,
    Document,
    Conversation,
    Task,
    Idea,
    Technology,
    Unknown,
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityType::Person => write!(f, "person"),
            EntityType::Place => write!(f, "place"),
            EntityType::Project => write!(f, "project"),
            EntityType::Document => write!(f, "document"),
            EntityType::Conversation => write!(f, "conversation"),
            EntityType::Task => write!(f, "task"),
            EntityType::Idea => write!(f, "idea"),
            EntityType::Technology => write!(f, "technology"),
            EntityType::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEntity {
    pub id: String,
    pub name: String,
    pub entity_type: EntityType,
    pub description: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub mention_count: u32,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relationship_type: String,
    pub strength: f64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    entities: HashMap<String, GraphEntity>,
    relationships: HashMap<String, Relationship>,
    adjacency: HashMap<String, HashSet<String>>,
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
        }
    }

    pub fn add_entity(&mut self, entity: GraphEntity) -> Result<(), KnowledgeError> {
        if self.entities.contains_key(&entity.id) {
            return Err(KnowledgeError::DuplicateEntity(entity.name));
        }
        let id = entity.id.clone();
        self.adjacency.entry(id.clone()).or_default();
        self.entities.insert(id, entity);
        Ok(())
    }

    pub fn upsert_entity(&mut self, entity: GraphEntity) {
        let id = entity.id.clone();
        self.adjacency.entry(id.clone()).or_default();
        self.entities.insert(id, entity);
    }

    pub fn get_entity(&self, id: &str) -> Option<&GraphEntity> {
        self.entities.get(id)
    }

    pub fn find_entity_by_name(&self, name: &str) -> Option<&GraphEntity> {
        self.entities
            .values()
            .find(|e| e.name.to_lowercase() == name.to_lowercase())
    }

    pub fn search_entities(&self, query: &str) -> Vec<&GraphEntity> {
        let lower = query.to_lowercase();
        self.entities
            .values()
            .filter(|e| {
                e.name.to_lowercase().contains(&lower)
                    || e.entity_type.to_string().contains(&lower)
                    || e.description.to_lowercase().contains(&lower)
            })
            .collect()
    }

    pub fn add_relationship(&mut self, rel: Relationship) -> Result<(), KnowledgeError> {
        if !self.entities.contains_key(&rel.source_id) {
            return Err(KnowledgeError::EntityNotFound(rel.source_id.clone()));
        }
        if !self.entities.contains_key(&rel.target_id) {
            return Err(KnowledgeError::EntityNotFound(rel.target_id.clone()));
        }
        let id = rel.id.clone();
        self.adjacency
            .entry(rel.source_id.clone())
            .or_default()
            .insert(rel.target_id.clone());
        self.adjacency
            .entry(rel.target_id.clone())
            .or_default()
            .insert(rel.source_id.clone());
        self.relationships.insert(id, rel);
        Ok(())
    }

    pub fn get_relationships(&self, entity_id: &str) -> Vec<&Relationship> {
        self.relationships
            .values()
            .filter(|r| r.source_id == entity_id || r.target_id == entity_id)
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

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn relationship_count(&self) -> usize {
        self.relationships.len()
    }

    pub fn all_entities(&self) -> Vec<&GraphEntity> {
        self.entities.values().collect()
    }

    pub fn all_relationships(&self) -> Vec<&Relationship> {
        self.relationships.values().collect()
    }

    pub fn get_entity_by_type(&self, entity_type: &EntityType) -> Vec<&GraphEntity> {
        self.entities
            .values()
            .filter(|e| e.entity_type == *entity_type)
            .collect()
    }

    pub fn remove_entity(&mut self, id: &str) {
        self.entities.remove(id);
        if let Some(connected) = self.adjacency.remove(id) {
            for conn in connected {
                if let Some(adj) = self.adjacency.get_mut(&conn) {
                    adj.remove(id);
                }
            }
        }
        self.relationships
            .retain(|_, r| r.source_id != id && r.target_id != id);
    }
}
