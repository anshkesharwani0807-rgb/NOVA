mod analysis;
mod config;
mod error;
mod events;
mod graph;
mod recall;
mod relationship;
mod summary;
mod timeline;

pub use analysis::{AnalyzedMemory, ExtractedEntity, MemoryAnalyzer};
pub use config::KnowledgeConfig;
pub use error::KnowledgeError;
pub use events::KnowledgeEventPayload;
pub use graph::{GraphEntity, KnowledgeGraph, Relationship};
pub use recall::{RecallQuery, RecallResult, SmartRecall};
pub use relationship::RelationshipEngine;
pub use summary::{Summary, SummaryEngine};
pub use timeline::{Timeline, TimelineEntry, TimelineGenerator};

use chrono::Datelike;
use nova_kernel::{log_activity, EventBus, EventMetadata, NovaError, NovaEvent, Result};
use std::sync::Arc;

pub struct KnowledgeEngine {
    inner: Arc<KnowledgeInner>,
}

struct KnowledgeInner {
    config: parking_lot::RwLock<KnowledgeConfig>,
    analyzer: parking_lot::RwLock<MemoryAnalyzer>,
    graph: parking_lot::RwLock<KnowledgeGraph>,
    relationship_engine: RelationshipEngine,
    timeline_gen: parking_lot::RwLock<TimelineGenerator>,
    summary_engine: parking_lot::RwLock<SummaryEngine>,
    memory: std::sync::Mutex<Option<Arc<nova_memory::MemoryEngine>>>,
    search: std::sync::Mutex<Option<Arc<nova_search::UniversalSearch>>>,
    event_bus: std::sync::Mutex<Option<Arc<EventBus>>>,
}

impl Default for KnowledgeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl KnowledgeEngine {
    pub fn new() -> Self {
        let cfg = KnowledgeConfig::default();
        let max_len = cfg.summary_max_length;
        let max_entries = cfg.timeline_max_entries;
        Self {
            inner: Arc::new(KnowledgeInner {
                config: parking_lot::RwLock::new(cfg),
                analyzer: parking_lot::RwLock::new(MemoryAnalyzer::new(KnowledgeConfig::default())),
                graph: parking_lot::RwLock::new(KnowledgeGraph::new()),
                relationship_engine: RelationshipEngine::new(),
                timeline_gen: parking_lot::RwLock::new(TimelineGenerator::new(max_entries)),
                summary_engine: parking_lot::RwLock::new(SummaryEngine::new(max_len)),
                memory: std::sync::Mutex::new(None),
                search: std::sync::Mutex::new(None),
                event_bus: std::sync::Mutex::new(None),
            }),
        }
    }

    pub fn set_memory(&self, memory: Arc<nova_memory::MemoryEngine>) {
        if let Ok(mut m) = self.inner.memory.lock() {
            *m = Some(memory);
        }
    }

    pub fn set_search(&self, search: Arc<nova_search::UniversalSearch>) {
        if let Ok(mut s) = self.inner.search.lock() {
            *s = Some(search);
        }
    }

    #[expect(dead_code)]
    fn memory(&self) -> Result<Arc<nova_memory::MemoryEngine>> {
        self.inner
            .memory
            .lock()
            .map_err(|_| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Kernel,
                    "ERR_LOCK",
                    "memory lock failed",
                )
            })?
            .clone()
            .ok_or_else(|| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Kernel,
                    "ERR_NOT_INIT",
                    "memory not set",
                )
            })
    }

    fn search(&self) -> Result<Arc<nova_search::UniversalSearch>> {
        self.inner
            .search
            .lock()
            .map_err(|_| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Kernel,
                    "ERR_LOCK",
                    "search lock failed",
                )
            })?
            .clone()
            .ok_or_else(|| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Kernel,
                    "ERR_NOT_INIT",
                    "search not set",
                )
            })
    }

    fn event_bus(&self) -> Result<Arc<EventBus>> {
        self.inner
            .event_bus
            .lock()
            .map_err(|_| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Kernel,
                    "ERR_LOCK",
                    "event_bus lock failed",
                )
            })?
            .clone()
            .ok_or_else(|| {
                NovaError::new(
                    nova_kernel::ErrorCategory::Kernel,
                    "ERR_NOT_INIT",
                    "event_bus not set",
                )
            })
    }

    fn publish(&self, payload: KnowledgeEventPayload) {
        if let Ok(bus) = self.event_bus() {
            let meta = EventMetadata::new("knowledge", None);
            let event = NovaEvent {
                metadata: meta,
                payload: Arc::new(payload),
            };
            let _ = bus.publish(event);
        }
    }

    pub fn analyze_memory(&self, record: &nova_memory::MemoryRecord) -> Result<AnalyzedMemory> {
        let analyzer = self.inner.analyzer.read();
        let analyzed = analyzer.analyze(record).map_err(|e| {
            NovaError::new(
                nova_kernel::ErrorCategory::Storage,
                "ERR_ANALYSIS",
                &e.to_string(),
            )
        })?;
        drop(analyzer);

        let mut graph = self.inner.graph.write();
        for entity in &analyzed.entities {
            let entity_type = match entity.entity_type {
                crate::analysis::EntityType::Person => graph::EntityType::Person,
                crate::analysis::EntityType::Place => graph::EntityType::Place,
                crate::analysis::EntityType::Project => graph::EntityType::Project,
                crate::analysis::EntityType::Document => graph::EntityType::Document,
                crate::analysis::EntityType::Conversation => graph::EntityType::Conversation,
                crate::analysis::EntityType::Task => graph::EntityType::Task,
                crate::analysis::EntityType::Idea => graph::EntityType::Idea,
                crate::analysis::EntityType::Technology => graph::EntityType::Technology,
                crate::analysis::EntityType::Unknown => graph::EntityType::Unknown,
            };
            if graph.find_entity_by_name(&entity.name).is_none() {
                let ge = GraphEntity {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: entity.name.clone(),
                    entity_type,
                    description: String::new(),
                    first_seen: record.created_at,
                    last_seen: record.created_at,
                    mention_count: 1,
                    metadata: std::collections::HashMap::new(),
                };
                let _ = graph.add_entity(ge);
            }
        }

        let rel_engine = &self.inner.relationship_engine;
        let entities: Vec<crate::analysis::ExtractedEntity> = analyzed.entities.clone();
        let rels = rel_engine.detect_relationships(&entities, &graph);
        for rel in rels {
            let _ = graph.add_relationship(rel);
        }
        drop(graph);

        self.publish(KnowledgeEventPayload::MemoryAnalyzed {
            memory_id: analyzed.memory_id.clone(),
            category: analyzed.category.clone(),
            tags: analyzed.tags.clone(),
            importance: analyzed.importance,
        });

        log_activity(
            "knowledge",
            "memory_analyzed",
            &format!("id={}", record.id),
            None,
        );
        Ok(analyzed)
    }

    pub fn get_graph(&self) -> parking_lot::RwLockReadGuard<'_, KnowledgeGraph> {
        self.inner.graph.read()
    }

    pub fn config(&self) -> parking_lot::RwLockReadGuard<'_, KnowledgeConfig> {
        self.inner.config.read()
    }

    pub fn config_mut(&self) -> parking_lot::RwLockWriteGuard<'_, KnowledgeConfig> {
        self.inner.config.write()
    }

    pub fn recall(&self, query: &RecallQuery) -> Result<RecallResult> {
        let search = self.search()?;
        let recall = SmartRecall::new(search);
        let result = recall.recall(query).map_err(|e| {
            NovaError::new(
                nova_kernel::ErrorCategory::Inference,
                "ERR_RECALL",
                &e.to_string(),
            )
        })?;
        self.publish(KnowledgeEventPayload::RecallCompleted {
            query: query.text.clone(),
            result_count: result.results.len(),
        });
        log_activity(
            "knowledge",
            "recall_completed",
            &format!("query={}", query.text),
            None,
        );
        Ok(result)
    }

    pub fn generate_timeline(
        &self,
        records: &[nova_memory::MemoryRecord],
        granularity: &str,
    ) -> Result<Timeline> {
        let gen = self.inner.timeline_gen.read();
        let now = chrono::Utc::now().timestamp_millis();
        let timeline = match granularity {
            "daily" => gen.generate_daily(records, now),
            "weekly" => gen.generate_weekly(records, now),
            "monthly" => {
                let now_dt = chrono::Utc::now();
                gen.generate_monthly(records, now_dt.year(), now_dt.month())
            }
            "project" => gen.generate_project_timeline(records, "project"),
            "conversation" => gen.generate_conversation_timeline(records),
            _ => {
                return Err(NovaError::new(
                    nova_kernel::ErrorCategory::ConfigInvalid,
                    "ERR_TIMELINE",
                    &format!("unknown granularity: {}", granularity),
                ))
            }
        };
        match timeline {
            Ok(t) => {
                self.publish(KnowledgeEventPayload::TimelineGenerated {
                    granularity: t.granularity.clone(),
                    entry_count: t.entries.len(),
                    time_range: format!("{} - {}", t.time_range.0, t.time_range.1),
                });
                log_activity("knowledge", "timeline_generated", &t.granularity, None);
                Ok(t)
            }
            Err(e) => Err(NovaError::new(
                nova_kernel::ErrorCategory::Storage,
                "ERR_TIMELINE",
                &e.to_string(),
            )),
        }
    }

    pub fn summarize(
        &self,
        records: &[nova_memory::MemoryRecord],
        summary_type: &str,
        label: &str,
    ) -> Result<Summary> {
        let engine = self.inner.summary_engine.read();
        let summary = match summary_type {
            "conversation" => engine.summarize_conversation(records),
            "project" => engine.summarize_project(records, label),
            "daily" => engine.summarize_daily(records, label),
            "cluster" => engine.summarize_cluster(records, label),
            _ => {
                return Err(NovaError::new(
                    nova_kernel::ErrorCategory::ConfigInvalid,
                    "ERR_SUMMARY",
                    &format!("unknown summary type: {}", summary_type),
                ))
            }
        };
        match summary {
            Ok(s) => {
                self.publish(KnowledgeEventPayload::SummaryCreated {
                    summary_type: s.summary_type.clone(),
                    target_id: s.target_id.clone(),
                    length: s.content.len(),
                });
                log_activity("knowledge", "summary_created", &s.summary_type, None);
                Ok(s)
            }
            Err(e) => Err(NovaError::new(
                nova_kernel::ErrorCategory::Inference,
                "ERR_SUMMARY",
                &e.to_string(),
            )),
        }
    }

    pub fn detect_duplicates(
        &self,
        records: &[nova_memory::MemoryRecord],
    ) -> Result<Vec<(String, String, f64)>> {
        let analyzer = self.inner.analyzer.read();
        let dups = analyzer.detect_duplicates(records);
        let cfg = self.inner.config.read();
        let threshold = cfg.dedup_similarity_threshold;
        let result: Vec<(String, String, f64)> = dups
            .iter()
            .map(|(a, b)| (a.clone(), b.clone(), threshold))
            .collect();
        Ok(result)
    }
}
