use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenConfig {
    pub target_fps: u32,
    pub include_cursor: bool,
    pub downscale_factor: Option<f32>,
    pub ocr_language: String,
    pub max_cache_entries: usize,
    pub enable_auto_ocr: bool,
    pub enable_ui_tree_extraction: bool,
    pub grounding_confidence_threshold: f32,
}

impl Default for ScreenConfig {
    fn default() -> Self {
        Self {
            target_fps: 30,
            include_cursor: true,
            downscale_factor: None,
            ocr_language: "en".to_string(),
            max_cache_entries: 100,
            enable_auto_ocr: false,
            enable_ui_tree_extraction: true,
            grounding_confidence_threshold: 0.6,
        }
    }
}
