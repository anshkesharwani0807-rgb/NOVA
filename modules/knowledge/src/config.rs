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
        }
    }
}
