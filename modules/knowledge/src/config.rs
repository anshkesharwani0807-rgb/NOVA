use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeConfig {
    pub auto_categorize: bool,
    pub auto_tag: bool,
    pub auto_importance: bool,
    pub auto_dedup: bool,
    pub auto_link: bool,
    pub auto_relationship: bool,
    pub timeline_max_entries: usize,
    pub summary_max_length: usize,
    pub importance_threshold_low: i32,
    pub importance_threshold_high: i32,
    pub dedup_similarity_threshold: f64,
    pub graph_max_entities: usize,
    pub index_batch_size: usize,
    pub enable_reasoning: bool,
    pub enable_semantic_index: bool,
    pub storage_auto_save: bool,
    pub storage_save_interval_ms: u64,
    pub max_path_depth: usize,
    pub max_context_fragments: usize,
}

impl KnowledgeConfig {
    pub fn conservative() -> Self {
        Self {
            auto_categorize: true,
            auto_tag: true,
            auto_importance: false,
            auto_dedup: true,
            auto_link: false,
            auto_relationship: false,
            timeline_max_entries: 100,
            summary_max_length: 200,
            importance_threshold_low: 0,
            importance_threshold_high: 8,
            dedup_similarity_threshold: 0.95,
            graph_max_entities: 10000,
            index_batch_size: 16,
            enable_reasoning: true,
            enable_semantic_index: true,
            storage_auto_save: false,
            storage_save_interval_ms: 60000,
            max_path_depth: 3,
            max_context_fragments: 10,
        }
    }
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            auto_categorize: true,
            auto_tag: true,
            auto_importance: true,
            auto_dedup: true,
            auto_link: true,
            auto_relationship: true,
            timeline_max_entries: 1000,
            summary_max_length: 500,
            importance_threshold_low: 0,
            importance_threshold_high: 8,
            dedup_similarity_threshold: 0.85,
            graph_max_entities: 100000,
            index_batch_size: 64,
            enable_reasoning: true,
            enable_semantic_index: true,
            storage_auto_save: true,
            storage_save_interval_ms: 30000,
            max_path_depth: 5,
            max_context_fragments: 20,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = KnowledgeConfig::default();
        assert!(cfg.auto_categorize);
        assert!(cfg.auto_tag);
        assert!(cfg.auto_dedup);
        assert!(cfg.enable_reasoning);
        assert!(cfg.enable_semantic_index);
        assert_eq!(cfg.timeline_max_entries, 1000);
        assert_eq!(cfg.max_path_depth, 5);
    }

    #[test]
    fn test_conservative_config() {
        let cfg = KnowledgeConfig::conservative();
        assert!(!cfg.auto_importance);
        assert!(!cfg.auto_link);
        assert!(!cfg.auto_relationship);
        assert!(!cfg.storage_auto_save);
        assert_eq!(cfg.graph_max_entities, 10000);
        assert_eq!(cfg.max_path_depth, 3);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let cfg = KnowledgeConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: KnowledgeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.auto_categorize, deserialized.auto_categorize);
        assert_eq!(cfg.timeline_max_entries, deserialized.timeline_max_entries);
        assert_eq!(cfg.max_path_depth, deserialized.max_path_depth);
        assert_eq!(
            cfg.dedup_similarity_threshold,
            deserialized.dedup_similarity_threshold
        );
    }

    #[test]
    fn test_config_fields_range() {
        let cfg = KnowledgeConfig::default();
        assert!(cfg.importance_threshold_low < cfg.importance_threshold_high);
        assert!(cfg.dedup_similarity_threshold > 0.0 && cfg.dedup_similarity_threshold <= 1.0);
        assert!(cfg.max_path_depth > 0);
        assert!(cfg.index_batch_size > 0);
    }
}
