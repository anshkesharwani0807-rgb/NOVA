use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::entity::{EntityType, KnowledgeEntity};
use crate::error::KnowledgeError;
use crate::ranking::{compute_recency_score, CombinedRanker, RankedResult, Ranker};

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, KnowledgeError>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, KnowledgeError>;
    fn dimension(&self) -> usize;
}

pub struct MockEmbeddingProvider {
    dimension: usize,
}

impl MockEmbeddingProvider {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, KnowledgeError> {
        let mut vec = vec![0.0f32; self.dimension];
        vec[0] = 1.0;
        Ok(vec)
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, KnowledgeError> {
        let mut results = Vec::with_capacity(texts.len());
        for t in texts {
            results.push(self.embed(t).await?);
        }
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub entity_id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub entity_type: EntityType,
    pub timestamp: i64,
    pub confidence: f64,
}

pub struct KnowledgeIndex {
    entries: RwLock<Vec<IndexEntry>>,
    embedder: Arc<dyn EmbeddingProvider>,
}

impl KnowledgeIndex {
    pub fn new(embedder: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            embedder,
        }
    }

    pub async fn index_entity(&self, entity: &KnowledgeEntity) -> Result<(), KnowledgeError> {
        let text = format!(
            "{} {} {}",
            entity.name,
            entity.description,
            entity.entity_type.as_str()
        );
        let embedding = self.embedder.embed(&text).await?;
        let entry = IndexEntry {
            entity_id: entity.id.clone(),
            text,
            embedding,
            entity_type: entity.entity_type.clone(),
            timestamp: entity.last_seen,
            confidence: entity.confidence,
        };
        self.entries.write().push(entry);
        Ok(())
    }

    pub async fn index_entities(&self, entities: &[KnowledgeEntity]) -> Result<(), KnowledgeError> {
        let texts: Vec<String> = entities
            .iter()
            .map(|e| format!("{} {} {}", e.name, e.description, e.entity_type.as_str()))
            .collect();
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let embeddings = self.embedder.embed_batch(&text_refs).await?;

        let mut entries = self.entries.write();
        for (i, entity) in entities.iter().enumerate() {
            if i < embeddings.len() {
                entries.push(IndexEntry {
                    entity_id: entity.id.clone(),
                    text: texts[i].clone(),
                    embedding: embeddings[i].clone(),
                    entity_type: entity.entity_type.clone(),
                    timestamp: entity.last_seen,
                    confidence: entity.confidence,
                });
            }
        }
        Ok(())
    }

    pub fn remove_entity(&self, entity_id: &str) {
        self.entries.write().retain(|e| e.entity_id != entity_id);
    }

    pub fn clear(&self) {
        self.entries.write().clear();
    }

    pub fn entry_count(&self) -> usize {
        self.entries.read().len()
    }

    pub async fn semantic_search(
        &self,
        query: &str,
        limit: usize,
        type_filter: Option<EntityType>,
        _metadata_filter: Option<&HashMap<String, String>>,
    ) -> Result<Vec<RankedResult>, KnowledgeError> {
        let query_embedding = self.embedder.embed(query).await?;
        let entries = self.entries.read();
        let now = chrono::Utc::now().timestamp_millis();

        let mut results: Vec<RankedResult> = Vec::new();

        for entry in entries.iter() {
            if let Some(ref tf) = type_filter {
                if entry.entity_type != *tf {
                    continue;
                }
            }
            let embedding_score = cosine_similarity(&query_embedding, &entry.embedding);
            if embedding_score < 0.1 {
                continue;
            }

            let keyword_score = compute_keyword_score(query, &entry.text);
            let recency_score = compute_recency_score(entry.timestamp, now);

            results.push(RankedResult {
                entity_id: entry.entity_id.clone(),
                name: entry
                    .text
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string(),
                score: 0.0,
                entity_relevance: embedding_score,
                graph_distance: 0.5,
                embedding_score,
                keyword_score,
                recency_score,
                confidence_score: entry.confidence,
                details: {
                    let mut m = HashMap::new();
                    m.insert("embedding_score".to_string(), embedding_score);
                    m.insert("keyword_score".to_string(), keyword_score);
                    m.insert("recency_score".to_string(), recency_score);
                    m
                },
            });
        }

        let ranker = CombinedRanker::new();
        let mut ranked = ranker.rank(results);
        ranked.truncate(limit);
        Ok(ranked)
    }
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)) as f64
}

pub fn compute_keyword_score(query: &str, text: &str) -> f64 {
    let query_lower = query.to_lowercase();
    let text_lower = text.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();
    if query_words.is_empty() {
        return 0.0;
    }
    let match_count = query_words
        .iter()
        .filter(|w| text_lower.contains(*w))
        .count();
    match_count as f64 / query_words.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_embedder() {
        let embedder = Arc::new(MockEmbeddingProvider::new(4));
        let vec = embedder.embed("test").await.unwrap();
        assert_eq!(vec.len(), 4);
    }

    #[tokio::test]
    async fn test_index_and_search() {
        let embedder = Arc::new(MockEmbeddingProvider::new(8));
        let index = KnowledgeIndex::new(embedder);

        let now = chrono::Utc::now().timestamp_millis();
        let entity = KnowledgeEntity {
            id: "e1".into(),
            name: "Rust Programming".into(),
            entity_type: EntityType::Topic,
            description: "A systems programming language".into(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            source: crate::entity::EntitySource::Memory,
            metadata: HashMap::new(),
        };
        index.index_entity(&entity).await.unwrap();
        assert_eq!(index.entry_count(), 1);

        let results = index.semantic_search("rust", 10, None, None).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_index_batch() {
        let embedder = Arc::new(MockEmbeddingProvider::new(4));
        let index = KnowledgeIndex::new(embedder);
        let now = chrono::Utc::now().timestamp_millis();
        let entities = vec![
            KnowledgeEntity {
                id: "e1".into(),
                name: "Rust".into(),
                entity_type: EntityType::Topic,
                description: "lang".into(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.9,
                source: crate::entity::EntitySource::Memory,
                metadata: HashMap::new(),
            },
            KnowledgeEntity {
                id: "e2".into(),
                name: "Python".into(),
                entity_type: EntityType::Topic,
                description: "lang".into(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.9,
                source: crate::entity::EntitySource::Memory,
                metadata: HashMap::new(),
            },
        ];
        index.index_entities(&entities).await.unwrap();
        assert_eq!(index.entry_count(), 2);
    }

    #[tokio::test]
    async fn test_remove_entity() {
        let embedder = Arc::new(MockEmbeddingProvider::new(4));
        let index = KnowledgeIndex::new(embedder);
        let now = chrono::Utc::now().timestamp_millis();
        let entity = KnowledgeEntity {
            id: "e1".into(),
            name: "Rust".into(),
            entity_type: EntityType::Topic,
            description: "".into(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            source: crate::entity::EntitySource::Memory,
            metadata: HashMap::new(),
        };
        index.index_entity(&entity).await.unwrap();
        assert_eq!(index.entry_count(), 1);
        index.remove_entity("e1");
        assert_eq!(index.entry_count(), 0);
    }

    #[tokio::test]
    async fn test_search_with_type_filter() {
        let embedder = Arc::new(MockEmbeddingProvider::new(4));
        let index = KnowledgeIndex::new(embedder);
        let now = chrono::Utc::now().timestamp_millis();
        let person = KnowledgeEntity {
            id: "e1".into(),
            name: "Alice".into(),
            entity_type: EntityType::Person,
            description: "".into(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            source: crate::entity::EntitySource::Memory,
            metadata: HashMap::new(),
        };
        index.index_entity(&person).await.unwrap();

        let results = index
            .semantic_search("Alice", 10, Some(EntityType::Person), None)
            .await
            .unwrap();
        assert!(!results.is_empty());

        let results = index
            .semantic_search("Alice", 10, Some(EntityType::Place), None)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.01);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 0.01);
    }

    #[test]
    fn test_keyword_score() {
        assert!((compute_keyword_score("rust programming", "I love Rust") - 0.5).abs() < 0.01);
        assert!((compute_keyword_score("python", "I love Rust") - 0.0).abs() < 0.01);
        assert!((compute_keyword_score("", "anything") - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_mock_embedder_dimension() {
        let embedder = MockEmbeddingProvider::new(384);
        assert_eq!(embedder.dimension(), 384);
    }

    #[tokio::test]
    async fn test_semantic_search_empty_index() {
        let embedder = Arc::new(MockEmbeddingProvider::new(4));
        let index = KnowledgeIndex::new(embedder);
        let results = index
            .semantic_search("anything", 10, None, None)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_semantic_search_with_type_filter_no_match() {
        let embedder = Arc::new(MockEmbeddingProvider::new(8));
        let index = KnowledgeIndex::new(embedder);
        let now = chrono::Utc::now().timestamp_millis();
        let entity = KnowledgeEntity {
            id: "e1".into(),
            name: "Test".into(),
            entity_type: EntityType::Topic,
            description: "something".into(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.5,
            source: crate::entity::EntitySource::Memory,
            metadata: HashMap::new(),
        };
        index.index_entity(&entity).await.unwrap();
        let results = index
            .semantic_search("test", 10, Some(EntityType::Person), None)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_index_remove_nonexistent() {
        let embedder = Arc::new(MockEmbeddingProvider::new(4));
        let index = KnowledgeIndex::new(embedder);
        index.remove_entity("nonexistent");
        assert_eq!(index.entry_count(), 0);
    }

    #[test]
    fn test_mock_embedder_consistency() {
        let embedder = MockEmbeddingProvider::new(8);
        assert_eq!(embedder.dimension(), 8);
    }

    #[tokio::test]
    async fn test_index_multiple_entities_same_type() {
        let embedder = Arc::new(MockEmbeddingProvider::new(4));
        let index = KnowledgeIndex::new(embedder);
        let now = chrono::Utc::now().timestamp_millis();
        let entities = vec![
            KnowledgeEntity {
                id: "e1".into(),
                name: "Alice".into(),
                entity_type: EntityType::Person,
                description: "".into(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.9,
                source: crate::entity::EntitySource::Memory,
                metadata: HashMap::new(),
            },
            KnowledgeEntity {
                id: "e2".into(),
                name: "Bob".into(),
                entity_type: EntityType::Person,
                description: "".into(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.9,
                source: crate::entity::EntitySource::Memory,
                metadata: HashMap::new(),
            },
            KnowledgeEntity {
                id: "e3".into(),
                name: "Rust".into(),
                entity_type: EntityType::Topic,
                description: "".into(),
                aliases: vec![],
                first_seen: now,
                last_seen: now,
                mention_count: 1,
                confidence: 0.9,
                source: crate::entity::EntitySource::Memory,
                metadata: HashMap::new(),
            },
        ];
        index.index_entities(&entities).await.unwrap();
        assert_eq!(index.entry_count(), 3);

        let person_results = index
            .semantic_search("Alice", 10, Some(EntityType::Person), None)
            .await
            .unwrap();
        assert!(!person_results.is_empty());
    }

    #[tokio::test]
    async fn test_reindex_updates_entry() {
        let embedder = Arc::new(MockEmbeddingProvider::new(4));
        let index = KnowledgeIndex::new(embedder);
        let now = chrono::Utc::now().timestamp_millis();
        let entity = KnowledgeEntity {
            id: "e1".into(),
            name: "Original".into(),
            entity_type: EntityType::Topic,
            description: "first".into(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            source: crate::entity::EntitySource::Memory,
            metadata: HashMap::new(),
        };
        index.index_entity(&entity).await.unwrap();
        assert_eq!(index.entry_count(), 1);

        let updated = KnowledgeEntity {
            id: "e2".into(),
            name: "Updated".into(),
            entity_type: EntityType::Topic,
            description: "second".into(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            source: crate::entity::EntitySource::Memory,
            metadata: HashMap::new(),
        };
        index.index_entity(&updated).await.unwrap();
        assert_eq!(index.entry_count(), 2);
    }

    #[tokio::test]
    async fn test_batch_index_empty() {
        let embedder = Arc::new(MockEmbeddingProvider::new(4));
        let index = KnowledgeIndex::new(embedder);
        index.index_entities(&[]).await.unwrap();
        assert_eq!(index.entry_count(), 0);
    }

    #[tokio::test]
    async fn test_embedder_preserves_dimension_across_calls() {
        let embedder = Arc::new(MockEmbeddingProvider::new(384));
        let v1 = embedder.embed("hello").await.unwrap();
        let v2 = embedder.embed("world").await.unwrap();
        assert_eq!(v1.len(), 384);
        assert_eq!(v2.len(), 384);
    }

    #[tokio::test]
    async fn test_clear_index() {
        let embedder = Arc::new(MockEmbeddingProvider::new(4));
        let index = KnowledgeIndex::new(embedder);
        let now = chrono::Utc::now().timestamp_millis();
        let entity = KnowledgeEntity {
            id: "e1".into(),
            name: "Test".into(),
            entity_type: EntityType::Topic,
            description: "".into(),
            aliases: vec![],
            first_seen: now,
            last_seen: now,
            mention_count: 1,
            confidence: 0.9,
            source: crate::entity::EntitySource::Memory,
            metadata: HashMap::new(),
        };
        index.index_entity(&entity).await.unwrap();
        assert_eq!(index.entry_count(), 1);
        index.clear();
        assert_eq!(index.entry_count(), 0);
    }
}
