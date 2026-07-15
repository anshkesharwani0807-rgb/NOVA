use async_trait::async_trait;
use parking_lot::RwLock;

use crate::entity::KnowledgeEntity;
use crate::error::KnowledgeError;
use crate::graph::{GraphEntity, KnowledgeGraph, KnowledgeRelationship};

#[async_trait]
pub trait KnowledgeStorage: Send + Sync {
    async fn save_graph(&self, graph: &KnowledgeGraph) -> Result<(), KnowledgeError>;
    async fn load_graph(&self) -> Result<KnowledgeGraph, KnowledgeError>;
    async fn save_entities(&self, entities: &[KnowledgeEntity]) -> Result<(), KnowledgeError>;
    async fn load_entities(&self) -> Result<Vec<KnowledgeEntity>, KnowledgeError>;
    async fn save_index_data(&self, data: &[IndexData]) -> Result<(), KnowledgeError>;
    async fn load_index_data(&self) -> Result<Vec<IndexData>, KnowledgeError>;
    async fn clear(&self) -> Result<(), KnowledgeError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexData {
    pub entity_id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub entity_type: String,
    pub timestamp: i64,
    pub confidence: f64,
}

use serde::{Deserialize, Serialize};

pub struct JsonFileStorage {
    base_path: std::path::PathBuf,
}

impl JsonFileStorage {
    pub fn new(base_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    fn graph_path(&self) -> std::path::PathBuf {
        self.base_path.join("knowledge_graph.json")
    }

    fn entities_path(&self) -> std::path::PathBuf {
        self.base_path.join("knowledge_entities.json")
    }

    fn index_path(&self) -> std::path::PathBuf {
        self.base_path.join("knowledge_index.json")
    }
}

#[async_trait]
impl KnowledgeStorage for JsonFileStorage {
    async fn save_graph(&self, graph: &KnowledgeGraph) -> Result<(), KnowledgeError> {
        let data = GraphData {
            entities: graph.all_entities().into_iter().cloned().collect(),
            relationships: graph.all_relationships().into_iter().cloned().collect(),
        };
        let json = serde_json::to_string_pretty(&data)
            .map_err(|e| KnowledgeError::SerializationError(e.to_string()))?;
        if let Some(parent) = self.graph_path().parent() {
            std::fs::create_dir_all(parent).map_err(|e| KnowledgeError::Storage(e.to_string()))?;
        }
        std::fs::write(self.graph_path(), json)
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn load_graph(&self) -> Result<KnowledgeGraph, KnowledgeError> {
        let path = self.graph_path();
        if !path.exists() {
            return Ok(KnowledgeGraph::new());
        }
        let json =
            std::fs::read_to_string(&path).map_err(|e| KnowledgeError::Storage(e.to_string()))?;
        let data: GraphData = serde_json::from_str(&json)
            .map_err(|e| KnowledgeError::SerializationError(e.to_string()))?;
        let mut graph = KnowledgeGraph::new();
        for entity in data.entities {
            graph.upsert_entity(entity);
        }
        for rel in data.relationships {
            let _ = graph.add_relationship(rel);
        }
        Ok(graph)
    }

    async fn save_entities(&self, entities: &[KnowledgeEntity]) -> Result<(), KnowledgeError> {
        let json = serde_json::to_string_pretty(entities)
            .map_err(|e| KnowledgeError::SerializationError(e.to_string()))?;
        if let Some(parent) = self.entities_path().parent() {
            std::fs::create_dir_all(parent).map_err(|e| KnowledgeError::Storage(e.to_string()))?;
        }
        std::fs::write(self.entities_path(), json)
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn load_entities(&self) -> Result<Vec<KnowledgeEntity>, KnowledgeError> {
        let path = self.entities_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let json =
            std::fs::read_to_string(&path).map_err(|e| KnowledgeError::Storage(e.to_string()))?;
        let entities: Vec<KnowledgeEntity> = serde_json::from_str(&json)
            .map_err(|e| KnowledgeError::SerializationError(e.to_string()))?;
        Ok(entities)
    }

    async fn save_index_data(&self, data: &[IndexData]) -> Result<(), KnowledgeError> {
        let json = serde_json::to_string_pretty(data)
            .map_err(|e| KnowledgeError::SerializationError(e.to_string()))?;
        if let Some(parent) = self.index_path().parent() {
            std::fs::create_dir_all(parent).map_err(|e| KnowledgeError::Storage(e.to_string()))?;
        }
        std::fs::write(self.index_path(), json)
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn load_index_data(&self) -> Result<Vec<IndexData>, KnowledgeError> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let json =
            std::fs::read_to_string(&path).map_err(|e| KnowledgeError::Storage(e.to_string()))?;
        let data: Vec<IndexData> = serde_json::from_str(&json)
            .map_err(|e| KnowledgeError::SerializationError(e.to_string()))?;
        Ok(data)
    }

    async fn clear(&self) -> Result<(), KnowledgeError> {
        for path in [self.graph_path(), self.entities_path(), self.index_path()] {
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| KnowledgeError::Storage(e.to_string()))?;
            }
        }
        Ok(())
    }
}

pub struct InMemoryStorage {
    graph: RwLock<Option<KnowledgeGraph>>,
    entities: RwLock<Vec<KnowledgeEntity>>,
    index_data: RwLock<Vec<IndexData>>,
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            graph: RwLock::new(None),
            entities: RwLock::new(Vec::new()),
            index_data: RwLock::new(Vec::new()),
        }
    }
}

#[async_trait]
impl KnowledgeStorage for InMemoryStorage {
    async fn save_graph(&self, graph: &KnowledgeGraph) -> Result<(), KnowledgeError> {
        *self.graph.write() = Some(graph.clone());
        Ok(())
    }

    async fn load_graph(&self) -> Result<KnowledgeGraph, KnowledgeError> {
        Ok(self.graph.read().clone().unwrap_or_default())
    }

    async fn save_entities(&self, entities: &[KnowledgeEntity]) -> Result<(), KnowledgeError> {
        *self.entities.write() = entities.to_vec();
        Ok(())
    }

    async fn load_entities(&self) -> Result<Vec<KnowledgeEntity>, KnowledgeError> {
        Ok(self.entities.read().clone())
    }

    async fn save_index_data(&self, data: &[IndexData]) -> Result<(), KnowledgeError> {
        *self.index_data.write() = data.to_vec();
        Ok(())
    }

    async fn load_index_data(&self) -> Result<Vec<IndexData>, KnowledgeError> {
        Ok(self.index_data.read().clone())
    }

    async fn clear(&self) -> Result<(), KnowledgeError> {
        *self.graph.write() = None;
        self.entities.write().clear();
        self.index_data.write().clear();
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GraphData {
    entities: Vec<GraphEntity>,
    relationships: Vec<KnowledgeRelationship>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityType;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn make_entity(id: &str, name: &str) -> KnowledgeEntity {
        let now = chrono::Utc::now().timestamp_millis();
        KnowledgeEntity {
            id: id.to_string(),
            name: name.to_string(),
            entity_type: EntityType::Topic,
            description: String::new(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            source: crate::entity::EntitySource::Memory,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_in_memory_storage() {
        let storage = InMemoryStorage::new();
        let entities = vec![make_entity("e1", "Test1"), make_entity("e2", "Test2")];
        storage.save_entities(&entities).await.unwrap();
        let loaded = storage.load_entities().await.unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[tokio::test]
    async fn test_in_memory_graph() {
        let storage = InMemoryStorage::new();
        let mut graph = KnowledgeGraph::new();
        let e = GraphEntity {
            id: "e1".into(),
            name: "Test".into(),
            entity_type: EntityType::Topic,
            description: "".into(),
            aliases: vec![],
            first_seen: 0,
            last_seen: 0,
            mention_count: 1,
            confidence: 0.9,
            metadata: HashMap::new(),
        };
        graph.add_entity(e).unwrap();
        storage.save_graph(&graph).await.unwrap();
        let loaded = storage.load_graph().await.unwrap();
        assert_eq!(loaded.entity_count(), 1);
    }

    #[tokio::test]
    async fn test_in_memory_clear() {
        let storage = InMemoryStorage::new();
        storage
            .save_entities(&[make_entity("e1", "Test")])
            .await
            .unwrap();
        storage.clear().await.unwrap();
        let loaded = storage.load_entities().await.unwrap();
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn test_json_storage_roundtrip() {
        let dir = std::env::temp_dir().join(format!("nova-knowledge-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = JsonFileStorage::new(&dir);

        let mut graph = KnowledgeGraph::new();
        let e = GraphEntity {
            id: "e1".into(),
            name: "Test".into(),
            entity_type: EntityType::Topic,
            description: "desc".into(),
            aliases: vec![],
            first_seen: 0,
            last_seen: 0,
            mention_count: 1,
            confidence: 0.9,
            metadata: HashMap::new(),
        };
        graph.add_entity(e).unwrap();
        storage.save_graph(&graph).await.unwrap();
        let loaded = storage.load_graph().await.unwrap();
        assert_eq!(loaded.entity_count(), 1);
        assert_eq!(loaded.get_entity("e1").unwrap().name, "Test");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_json_storage_load_nonexistent() {
        let dir =
            std::env::temp_dir().join(format!("nova-knowledge-test-nonexist-{}", Uuid::new_v4()));
        let storage = JsonFileStorage::new(&dir);
        let graph = storage.load_graph().await.unwrap();
        assert_eq!(graph.entity_count(), 0);
    }

    #[tokio::test]
    async fn test_in_memory_storage_empty() {
        let storage = InMemoryStorage::new();
        let entities = storage.load_entities().await.unwrap();
        assert!(entities.is_empty());
        let graph = storage.load_graph().await.unwrap();
        assert_eq!(graph.entity_count(), 0);
    }

    #[tokio::test]
    async fn test_in_memory_storage_overwrite() {
        let storage = InMemoryStorage::new();
        let e1 = vec![make_entity("e1", "First")];
        storage.save_entities(&e1).await.unwrap();
        let e2 = vec![make_entity("e2", "Second")];
        storage.save_entities(&e2).await.unwrap();
        let loaded = storage.load_entities().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Second");
    }

    #[tokio::test]
    async fn test_in_memory_graph_overwrite() {
        let storage = InMemoryStorage::new();
        let mut g1 = KnowledgeGraph::new();
        g1.add_entity(GraphEntity {
            id: "e1".into(),
            name: "First".into(),
            entity_type: EntityType::Topic,
            description: "".into(),
            aliases: vec![],
            first_seen: 0,
            last_seen: 0,
            mention_count: 1,
            confidence: 0.9,
            metadata: HashMap::new(),
        })
        .unwrap();
        storage.save_graph(&g1).await.unwrap();

        let mut g2 = KnowledgeGraph::new();
        g2.add_entity(GraphEntity {
            id: "e2".into(),
            name: "Second".into(),
            entity_type: EntityType::Topic,
            description: "".into(),
            aliases: vec![],
            first_seen: 0,
            last_seen: 0,
            mention_count: 1,
            confidence: 0.9,
            metadata: HashMap::new(),
        })
        .unwrap();
        storage.save_graph(&g2).await.unwrap();

        let loaded = storage.load_graph().await.unwrap();
        assert_eq!(loaded.entity_count(), 1);
        assert_eq!(loaded.get_entity("e2").unwrap().name, "Second");
    }

    #[tokio::test]
    async fn test_json_storage_with_entities() {
        let dir = std::env::temp_dir().join(format!("nova-knowledge-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = JsonFileStorage::new(&dir);

        let entities = vec![make_entity("e1", "Test1"), make_entity("e2", "Test2")];
        storage.save_entities(&entities).await.unwrap();
        let loaded = storage.load_entities().await.unwrap();
        assert_eq!(loaded.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_json_storage_clear() {
        let dir =
            std::env::temp_dir().join(format!("nova-knowledge-test-clear-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = JsonFileStorage::new(&dir);

        let entities = vec![make_entity("e1", "Test")];
        storage.save_entities(&entities).await.unwrap();
        storage.clear().await.unwrap();
        let loaded = storage.load_entities().await.unwrap();
        assert!(loaded.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_json_storage_index_roundtrip() {
        let dir = std::env::temp_dir().join(format!("nova-knowledge-test-idx-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = JsonFileStorage::new(&dir);

        let data = vec![IndexData {
            entity_id: "e1".into(),
            text: "test".into(),
            embedding: vec![0.1, 0.2],
            entity_type: "topic".into(),
            timestamp: 1000,
            confidence: 0.9,
        }];
        storage.save_index_data(&data).await.unwrap();
        let loaded = storage.load_index_data().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].embedding.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_json_storage_index_empty() {
        let dir =
            std::env::temp_dir().join(format!("nova-knowledge-test-idx-empty-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = JsonFileStorage::new(&dir);
        let loaded = storage.load_index_data().await.unwrap();
        assert!(loaded.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_index_data_overwrite() {
        let storage = InMemoryStorage::new();
        let d1 = vec![IndexData {
            entity_id: "e1".into(),
            text: "first".into(),
            embedding: vec![0.1],
            entity_type: "topic".into(),
            timestamp: 1000,
            confidence: 0.9,
        }];
        storage.save_index_data(&d1).await.unwrap();
        let d2 = vec![IndexData {
            entity_id: "e2".into(),
            text: "second".into(),
            embedding: vec![0.2],
            entity_type: "person".into(),
            timestamp: 2000,
            confidence: 0.8,
        }];
        storage.save_index_data(&d2).await.unwrap();
        let loaded = storage.load_index_data().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].entity_id, "e2");
    }

    #[tokio::test]
    async fn test_index_data_roundtrip() {
        let storage = InMemoryStorage::new();
        let data = vec![IndexData {
            entity_id: "e1".into(),
            text: "test".into(),
            embedding: vec![0.1, 0.2, 0.3],
            entity_type: "topic".into(),
            timestamp: 1000,
            confidence: 0.9,
        }];
        storage.save_index_data(&data).await.unwrap();
        let loaded = storage.load_index_data().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].entity_id, "e1");
    }
}
