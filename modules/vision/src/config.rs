use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionConfig {
    pub thumbnail_size: (u32, u32),
    pub thumbnail_quality: u8,
    pub max_cache_entries: usize,
    pub cache_ttl_secs: u64,
    pub memory_budget_bytes: u64,
    pub ocr_language: String,
    pub embedding_dim: usize,
    pub max_batch_size: usize,
    pub face_recognition_enabled: bool,
    pub visual_search_enabled: bool,
    pub auto_caption_enabled: bool,
    pub enable_auto_ocr: bool,
    pub similarity_threshold: f64,
    pub duplicate_threshold: f64,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            thumbnail_size: (256, 256),
            thumbnail_quality: 85,
            max_cache_entries: 500,
            cache_ttl_secs: 3600,
            memory_budget_bytes: 256 * 1024 * 1024,
            ocr_language: "eng".to_string(),
            embedding_dim: 384,
            max_batch_size: 16,
            face_recognition_enabled: false,
            visual_search_enabled: false,
            auto_caption_enabled: false,
            enable_auto_ocr: false,
            similarity_threshold: 0.75,
            duplicate_threshold: 0.95,
        }
    }
}
