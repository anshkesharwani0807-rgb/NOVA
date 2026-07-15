use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::entity::{EntityExtractor, EntitySource, EntityType, KnowledgeEntity};
use crate::events::KnowledgeEventPayload;
use crate::graph::{GraphEntity, KnowledgeRelationship};
use crate::index::{EmbeddingProvider, KnowledgeIndex, MockEmbeddingProvider};
use crate::ranking::{CombinedRanker, RankedResult, Ranker};
use crate::reasoning::{KnowledgeContext, KnowledgeReasoner, ReasoningResult};
use crate::storage::KnowledgeStorage;

use nova_kernel::{log_activity, NovaError, Result};

// ── Permission constants for Plugin SDK integration ─────────────────────────
pub const PERM_KNOWLEDGE_READ: &str = "knowledge.read";
pub const PERM_KNOWLEDGE_WRITE: &str = "knowledge.write";
pub const PERM_KNOWLEDGE_REASON: &str = "knowledge.reason";
pub const PERM_KNOWLEDGE_INDEX: &str = "knowledge.index";

impl super::KnowledgeEngine {
    pub fn set_storage(&self, storage: Arc<dyn KnowledgeStorage>) {
        *self.inner.storage.write() = Some(storage);
    }

    pub fn set_embedder(&self, embedder: Arc<dyn EmbeddingProvider>) {
        *self.inner.embedder.write() = Some(embedder);
    }

    fn get_or_create_index(&self) -> Arc<KnowledgeIndex> {
        let mut idx = self.inner.index.write();
        if idx.is_none() {
            let embedder = self.inner.embedder.read();
            let emb: Arc<dyn EmbeddingProvider> = if let Some(ref e) = *embedder {
                e.clone()
            } else {
                Arc::new(MockEmbeddingProvider::new(384))
            };
            *idx = Some(Arc::new(KnowledgeIndex::new(emb)));
        }
        idx.clone().unwrap()
    }

    fn get_reasoner(&self) -> KnowledgeReasoner {
        let graph = self.inner.graph.read().clone();
        let max_depth = self.inner.config.read().max_path_depth;
        KnowledgeReasoner::new(Arc::new(parking_lot::RwLock::new(graph)), max_depth)
    }

    // ── Entity Extraction ──────────────────────────────────────────────────

    pub fn extract_entities_from_text(
        &self,
        text: &str,
        title: &str,
        source: EntitySource,
    ) -> Vec<KnowledgeEntity> {
        let extractor = EntityExtractor::new(self.inner.config.read().graph_max_entities);
        extractor.extract_from_text(text, title, source)
    }

    pub fn extract_entities_from_memory(&self, content: &str, title: &str) -> Vec<KnowledgeEntity> {
        let extractor = EntityExtractor::new(self.inner.config.read().graph_max_entities);
        extractor.extract_from_memory(content, title)
    }

    pub fn extract_entities_from_ocr(&self, text: &str) -> Vec<KnowledgeEntity> {
        let extractor = EntityExtractor::new(self.inner.config.read().graph_max_entities);
        extractor.extract_from_ocr(text)
    }

    pub fn extract_entities_from_screenshot(&self, text_parts: &[&str]) -> Vec<KnowledgeEntity> {
        let extractor = EntityExtractor::new(self.inner.config.read().graph_max_entities);
        extractor.extract_from_screenshot_text(text_parts)
    }

    pub fn extract_entities_from_conversation(
        &self,
        text: &str,
        speaker: Option<&str>,
    ) -> Vec<KnowledgeEntity> {
        let extractor = EntityExtractor::new(self.inner.config.read().graph_max_entities);
        extractor.extract_from_conversation(text, speaker)
    }

    pub fn extract_entities_from_automation(
        &self,
        workflow_name: &str,
        action_type: &str,
    ) -> Vec<KnowledgeEntity> {
        let extractor = EntityExtractor::new(self.inner.config.read().graph_max_entities);
        extractor.extract_from_automation(workflow_name, action_type)
    }

    pub fn extract_entities_from_plugin(
        &self,
        plugin_id: &str,
        plugin_name: &str,
    ) -> Vec<KnowledgeEntity> {
        let extractor = EntityExtractor::new(self.inner.config.read().graph_max_entities);
        extractor.extract_from_plugin(plugin_id, plugin_name)
    }

    // ── Graph Entity Management ────────────────────────────────────────────

    pub fn add_entity_to_graph(&self, entity: KnowledgeEntity) -> Result<GraphEntity> {
        let mut graph = self.inner.graph.write();
        let ge: GraphEntity = entity.into();
        if let Some(existing_entity) = graph.find_entity_by_name(&ge.name) {
            let existing_id = existing_entity.id.clone();
            if let Some(existing) = graph.get_entity_mut(&existing_id) {
                let ke = KnowledgeEntity {
                    id: existing_id.clone(),
                    name: ge.name.clone(),
                    entity_type: ge.entity_type.clone(),
                    description: ge.description.clone(),
                    aliases: ge.aliases.clone(),
                    first_seen: ge.first_seen,
                    last_seen: ge.last_seen,
                    mention_count: ge.mention_count,
                    confidence: ge.confidence,
                    source: EntitySource::Memory,
                    metadata: ge.metadata.clone(),
                };
                EntityExtractor::merge_entity_from_graph(existing, &ke);
            }
            let entity = graph.get_entity(&existing_id).unwrap().clone();
            return Ok(entity);
        }
        let id = ge.id.clone();
        graph.add_entity(ge).map_err(|e| {
            NovaError::new(
                nova_kernel::ErrorCategory::Storage,
                "ERR_KNOWLEDGE",
                &e.to_string(),
            )
        })?;
        let entity = graph.get_entity(&id).unwrap().clone();
        self.publish(KnowledgeEventPayload::EntityCreated {
            entity_id: entity.id.clone(),
            entity_type: entity.entity_type.to_string(),
            name: entity.name.clone(),
            source: "memory".to_string(),
        });
        log_activity("knowledge", "entity_added", &entity.name, None);
        Ok(entity)
    }

    pub fn get_entity_by_name(&self, name: &str) -> Option<GraphEntity> {
        let graph = self.inner.graph.read();
        graph.find_entity_by_name(name).cloned()
    }

    pub fn search_entities_in_graph(&self, query: &str) -> Vec<GraphEntity> {
        let graph = self.inner.graph.read();
        graph.search_entities(query).into_iter().cloned().collect()
    }

    pub fn get_connected_entities(&self, entity_id: &str) -> Vec<GraphEntity> {
        let graph = self.inner.graph.read();
        graph
            .get_connected_entities(entity_id)
            .into_iter()
            .cloned()
            .collect()
    }

    // ── Relationship Management ────────────────────────────────────────────

    pub fn add_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
        strength: f64,
        provenance: &str,
    ) -> Result<String> {
        let mut graph = self.inner.graph.write();
        if !graph.has_entity(source_id) {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Storage,
                "ERR_KNOWLEDGE",
                &format!("source entity not found: {}", source_id),
            ));
        }
        if !graph.has_entity(target_id) {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Storage,
                "ERR_KNOWLEDGE",
                &format!("target entity not found: {}", target_id),
            ));
        }
        let now = Utc::now().timestamp_millis();
        let rel = KnowledgeRelationship {
            id: Uuid::new_v4().to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            relationship_type: rel_type.to_string(),
            strength,
            confidence: 0.8,
            first_seen: now,
            last_seen: now,
            provenance: provenance.to_string(),
            metadata: HashMap::new(),
        };
        let rel_id = rel.id.clone();
        graph.add_relationship(rel).map_err(|e| {
            NovaError::new(
                nova_kernel::ErrorCategory::Storage,
                "ERR_KNOWLEDGE",
                &e.to_string(),
            )
        })?;
        self.publish(KnowledgeEventPayload::RelationshipCreated {
            source_entity: source_id.to_string(),
            target_entity: target_id.to_string(),
            relationship_type: rel_type.to_string(),
            strength,
        });
        log_activity(
            "knowledge",
            "relationship_added",
            &format!("{} --[{}]--> {}", source_id, rel_type, target_id),
            None,
        );
        Ok(rel_id)
    }

    // ── Semantic Index ─────────────────────────────────────────────────────

    pub async fn index_entity_for_search(&self, entity: &KnowledgeEntity) -> Result<()> {
        let index = self.get_or_create_index();
        index.index_entity(entity).await.map_err(|e| {
            NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_KNOWLEDGE_INDEX",
                &e.to_string(),
            )
        })
    }

    pub async fn semantic_search(
        &self,
        query: &str,
        limit: usize,
        entity_type: Option<EntityType>,
    ) -> Result<Vec<RankedResult>> {
        let start = std::time::Instant::now();
        let index = self.get_or_create_index();
        let results = index
            .semantic_search(query, limit, entity_type, None)
            .await
            .map_err(|e| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Internal,
                    "ERR_KNOWLEDGE_SEARCH",
                    &e.to_string(),
                )
            })?;
        let duration = start.elapsed().as_millis() as u64;
        self.publish(KnowledgeEventPayload::KnowledgeSearchCompleted {
            query: query.to_string(),
            result_count: results.len(),
            duration_ms: duration,
        });
        log_activity(
            "knowledge",
            "semantic_search",
            &format!("query={} results={}", query, results.len()),
            None,
        );
        Ok(results)
    }

    pub async fn index_all_entities(&self) -> Result<usize> {
        let entities: Vec<KnowledgeEntity> = {
            let graph = self.inner.graph.read();
            graph
                .all_entities()
                .iter()
                .map(|ge| KnowledgeEntity {
                    id: ge.id.clone(),
                    name: ge.name.clone(),
                    entity_type: ge.entity_type.clone(),
                    description: ge.description.clone(),
                    aliases: ge.aliases.clone(),
                    first_seen: ge.first_seen,
                    last_seen: ge.last_seen,
                    mention_count: ge.mention_count,
                    confidence: ge.confidence,
                    source: EntitySource::Memory,
                    metadata: ge.metadata.clone(),
                })
                .collect()
        };

        if entities.is_empty() {
            return Ok(0);
        }

        let index = self.get_or_create_index();
        index.index_entities(&entities).await.map_err(|e| {
            NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_KNOWLEDGE_INDEX",
                &e.to_string(),
            )
        })?;

        self.publish(KnowledgeEventPayload::KnowledgeIndexed {
            entity_count: entities.len(),
            relationship_count: self.inner.graph.read().relationship_count(),
            duration_ms: 0,
        });
        Ok(entities.len())
    }

    // ── Reasoning ──────────────────────────────────────────────────────────

    pub fn reason(
        &self,
        query: &str,
        seed_entity_ids: &[String],
        max_depth: usize,
    ) -> Result<ReasoningResult> {
        let start = std::time::Instant::now();
        let reasoner = self.get_reasoner();
        let result = reasoner.reason(query, seed_entity_ids, max_depth);
        let duration = start.elapsed().as_millis() as u64;
        self.publish(KnowledgeEventPayload::KnowledgeReasoningCompleted {
            query: query.to_string(),
            path_count: result.paths.len(),
            duration_ms: duration,
        });
        log_activity(
            "knowledge",
            "reasoning",
            &format!("query={} paths={}", query, result.paths.len()),
            None,
        );
        Ok(result)
    }

    pub fn find_paths(
        &self,
        source_id: &str,
        target_id: &str,
        max_paths: usize,
    ) -> Result<Vec<crate::reasoning::PathResult>> {
        let reasoner = self.get_reasoner();
        reasoner
            .find_paths(source_id, target_id, max_paths)
            .map_err(|e| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Internal,
                    "ERR_KNOWLEDGE_REASONING",
                    &e.to_string(),
                )
            })
    }

    pub fn expand_context(
        &self,
        entity_ids: &[String],
        depth: usize,
        max_entities: usize,
    ) -> KnowledgeContext {
        let reasoner = self.get_reasoner();
        reasoner.expand_context(entity_ids, depth, max_entities)
    }

    pub fn generate_citations(&self, entity_ids: &[String]) -> Vec<String> {
        let reasoner = self.get_reasoner();
        reasoner.generate_citations(entity_ids)
    }

    // ── Knowledge Context for AI Runtime ───────────────────────────────────

    pub fn build_knowledge_context(&self, query: &str, limit: usize) -> KnowledgeContext {
        let entities: Vec<GraphEntity> = {
            let graph = self.inner.graph.read();
            graph
                .search_entities(query)
                .into_iter()
                .take(limit)
                .cloned()
                .collect()
        };

        let entity_ids: Vec<String> = entities.iter().map(|e| e.id.clone()).collect();
        if entity_ids.is_empty() {
            return KnowledgeContext {
                context_text: String::new(),
                entities: vec![],
                relationships: vec![],
                citations: vec![],
                confidence: 0.0,
            };
        }

        self.expand_context(&entity_ids, 1, limit)
    }

    // ── Persistence ────────────────────────────────────────────────────────

    pub async fn save(&self) -> Result<()> {
        let storage_opt = self.inner.storage.read().as_ref().cloned();
        if let Some(s) = storage_opt {
            let graph = self.inner.graph.read().clone();
            s.save_graph(&graph).await.map_err(|e| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Storage,
                    "ERR_KNOWLEDGE_SAVE",
                    &e.to_string(),
                )
            })?;

            let entities: Vec<KnowledgeEntity> = {
                let g = self.inner.graph.read();
                g.all_entities()
                    .iter()
                    .map(|ge| KnowledgeEntity {
                        id: ge.id.clone(),
                        name: ge.name.clone(),
                        entity_type: ge.entity_type.clone(),
                        description: ge.description.clone(),
                        aliases: ge.aliases.clone(),
                        first_seen: ge.first_seen,
                        last_seen: ge.last_seen,
                        mention_count: ge.mention_count,
                        confidence: ge.confidence,
                        source: EntitySource::Memory,
                        metadata: ge.metadata.clone(),
                    })
                    .collect()
            };
            s.save_entities(&entities).await.map_err(|e| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Storage,
                    "ERR_KNOWLEDGE_SAVE",
                    &e.to_string(),
                )
            })?;
        }
        Ok(())
    }

    pub async fn load(&self) -> Result<()> {
        let storage_opt = self.inner.storage.read().as_ref().cloned();
        if let Some(s) = storage_opt {
            let graph = s.load_graph().await.map_err(|e| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Storage,
                    "ERR_KNOWLEDGE_LOAD",
                    &e.to_string(),
                )
            })?;
            *self.inner.graph.write() = graph;
        }
        Ok(())
    }

    // ── Permissions Check ──────────────────────────────────────────────────

    pub fn check_permission(&self, permission: &str) -> bool {
        matches!(
            permission,
            PERM_KNOWLEDGE_READ
                | PERM_KNOWLEDGE_WRITE
                | PERM_KNOWLEDGE_REASON
                | PERM_KNOWLEDGE_INDEX
        )
    }

    // ── Hybrid Search (Graph + Semantic + Keyword) ─────────────────────────

    pub async fn hybrid_search(&self, query: &str, limit: usize) -> Result<Vec<RankedResult>> {
        let start = std::time::Instant::now();

        let graph_results = self.search_entities_in_graph(query);
        let index = self.get_or_create_index();
        let semantic_results = index
            .semantic_search(query, limit * 2, None, None)
            .await
            .map_err(|e| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Internal,
                    "ERR_KNOWLEDGE_SEARCH",
                    &e.to_string(),
                )
            })?;

        let now = Utc::now().timestamp_millis();
        let mut combined: Vec<RankedResult> = semantic_results;

        for ge in &graph_results {
            let keyword_score = crate::index::compute_keyword_score(
                query,
                &format!("{} {}", ge.name, ge.description),
            );
            let recency_score = crate::ranking::compute_recency_score(ge.last_seen, now);
            if !combined.iter().any(|r| r.entity_id == ge.id) {
                combined.push(RankedResult {
                    entity_id: ge.id.clone(),
                    name: ge.name.clone(),
                    score: 0.0,
                    entity_relevance: keyword_score,
                    graph_distance: 0.0,
                    embedding_score: 0.0,
                    keyword_score,
                    recency_score,
                    confidence_score: ge.confidence,
                    details: HashMap::new(),
                });
            }
        }

        let ranker = CombinedRanker::new();
        let mut results = ranker.rank(combined);
        results.truncate(limit);

        let duration = start.elapsed().as_millis() as u64;
        self.publish(KnowledgeEventPayload::KnowledgeSearchCompleted {
            query: query.to_string(),
            result_count: results.len(),
            duration_ms: duration,
        });
        Ok(results)
    }
}
