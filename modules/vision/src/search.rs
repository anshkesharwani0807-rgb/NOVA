use chrono::{DateTime, Local};
use nova_kernel::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::embedding::ImageEmbedding;
use crate::engine::VisionEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchType {
    Text,
    Image,
    Similar,
    Metadata,
    Ocr,
    Tag,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: Option<String>,
    pub search_type: SearchType,
    pub min_confidence: f64,
    pub date_from: Option<DateTime<Local>>,
    pub date_to: Option<DateTime<Local>>,
    pub max_results: usize,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            text: None,
            search_type: SearchType::Text,
            min_confidence: 0.0,
            date_from: None,
            date_to: None,
            max_results: 50,
        }
    }
}

impl SearchQuery {
    pub fn text(t: &str) -> Self {
        Self {
            text: Some(t.to_string()),
            ..Default::default()
        }
    }

    pub fn similar() -> Self {
        Self {
            search_type: SearchType::Similar,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f64,
    pub matched_fields: Vec<String>,
    pub thumbnail_path: Option<String>,
    pub caption: Option<String>,
    pub ocr_text: Option<String>,
    pub tags: Vec<String>,
    pub timestamp: DateTime<Local>,
}

pub struct IndexedImage {
    pub id: String,
    pub path: String,
    pub embedding: Option<ImageEmbedding>,
    pub ocr_text: Option<String>,
    pub caption: Option<String>,
    pub tags: Vec<String>,
    pub metadata_text: String,
    pub timestamp: DateTime<Local>,
}

pub struct VisualSearch {
    engine: Arc<VisionEngine>,
    index: RwLock<Vec<IndexedImage>>,
}

impl VisualSearch {
    pub fn new(engine: Arc<VisionEngine>) -> Self {
        Self {
            engine,
            index: RwLock::new(Vec::new()),
        }
    }

    pub fn index_image(&self, image: IndexedImage) {
        self.index.write().push(image);
    }

    pub fn index_count(&self) -> usize {
        self.index.read().len()
    }

    pub fn remove(&self, id: &str) {
        self.index.write().retain(|i| i.id != id);
    }

    pub fn clear(&self) {
        self.index.write().clear();
    }

    pub fn search(&self, query: &SearchQuery) -> Vec<SearchResult> {
        let index = self.index.read();
        let mut results: Vec<SearchResult> = Vec::new();

        for item in index.iter() {
            let mut score = 0.0f64;
            let mut matched = Vec::new();

            match &query.search_type {
                SearchType::Text | SearchType::Tag => {
                    if let Some(ref text) = query.text {
                        let lower = text.to_lowercase();
                        if item.metadata_text.to_lowercase().contains(&lower) {
                            score += 0.5;
                            matched.push("metadata".to_string());
                        }
                        if let Some(ref ocr) = item.ocr_text {
                            if ocr.to_lowercase().contains(&lower) {
                                score += 0.8;
                                matched.push("ocr".to_string());
                            }
                        }
                        if let Some(ref cap) = item.caption {
                            if cap.to_lowercase().contains(&lower) {
                                score += 0.7;
                                matched.push("caption".to_string());
                            }
                        }
                        for tag in &item.tags {
                            if tag.to_lowercase().contains(&lower) {
                                score += 0.6;
                                matched.push("tag".to_string());
                            }
                        }
                    }
                }
                SearchType::Ocr => {
                    if let Some(ref text) = query.text {
                        if let Some(ref ocr) = item.ocr_text {
                            if ocr.to_lowercase().contains(&text.to_lowercase()) {
                                score = 1.0;
                                matched.push("ocr".to_string());
                            }
                        }
                    }
                }
                SearchType::Similar | SearchType::Image => {
                    if item.embedding.is_some() {
                        score = 0.5;
                        matched.push("embedding".to_string());
                    }
                }
                SearchType::Metadata => {
                    if let Some(ref text) = query.text {
                        if item
                            .metadata_text
                            .to_lowercase()
                            .contains(&text.to_lowercase())
                        {
                            score = 1.0;
                            matched.push("metadata".to_string());
                        }
                    }
                }
            }

            if score > 0.0 && score >= query.min_confidence {
                results.push(SearchResult {
                    id: item.id.clone(),
                    score,
                    matched_fields: matched,
                    thumbnail_path: Some(item.path.clone()),
                    caption: item.caption.clone(),
                    ocr_text: item.ocr_text.clone(),
                    tags: item.tags.clone(),
                    timestamp: item.timestamp,
                });
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(query.max_results);
        results
    }

    pub async fn search_by_image(
        &self,
        bytes: &[u8],
        max_results: usize,
    ) -> Result<Vec<SearchResult>> {
        let query_emb = self.engine.embed_image(bytes).await?;
        let index = self.index.read();
        let mut results: Vec<(f64, &IndexedImage)> = Vec::new();

        for item in index.iter() {
            if let Some(ref emb) = item.embedding {
                let sim = query_emb.cosine_similarity(emb);
                if sim > 0.5 {
                    results.push((sim, item));
                }
            }
        }

        results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let results: Vec<SearchResult> = results
            .into_iter()
            .take(max_results)
            .map(|(score, item)| SearchResult {
                id: item.id.clone(),
                score,
                matched_fields: vec!["embedding".to_string()],
                thumbnail_path: Some(item.path.clone()),
                caption: item.caption.clone(),
                ocr_text: item.ocr_text.clone(),
                tags: item.tags.clone(),
                timestamp: item.timestamp,
            })
            .collect();

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> VisualSearch {
        let provider = Arc::new(crate::providers::mock::MockVisionProvider::new())
            as Arc<dyn crate::providers::VisionProvider>;
        let engine = Arc::new(VisionEngine::new(provider));
        VisualSearch::new(engine)
    }

    #[test]
    fn test_index_and_search_text() {
        let vs = setup();
        vs.index_image(IndexedImage {
            id: "1".to_string(),
            path: "/img1.jpg".to_string(),
            embedding: None,
            ocr_text: Some("Hello World".to_string()),
            caption: Some("A test image".to_string()),
            tags: vec!["test".to_string(), "sample".to_string()],
            metadata_text: "test image".to_string(),
            timestamp: Local::now(),
        });
        assert_eq!(vs.index_count(), 1);

        let results = vs.search(&SearchQuery::text("Hello"));
        assert!(!results.is_empty());
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn test_search_no_match() {
        let vs = setup();
        vs.index_image(IndexedImage {
            id: "1".to_string(),
            path: "/img1.jpg".to_string(),
            embedding: None,
            ocr_text: Some("Hello World".to_string()),
            caption: None,
            tags: vec![],
            metadata_text: String::new(),
            timestamp: Local::now(),
        });
        let results = vs.search(&SearchQuery::text("nonexistent"));
        assert!(results.is_empty());
    }

    #[test]
    fn test_remove_from_index() {
        let vs = setup();
        vs.index_image(IndexedImage {
            id: "1".to_string(),
            path: "/img1.jpg".to_string(),
            embedding: None,
            ocr_text: None,
            caption: None,
            tags: vec![],
            metadata_text: String::new(),
            timestamp: Local::now(),
        });
        assert_eq!(vs.index_count(), 1);
        vs.remove("1");
        assert_eq!(vs.index_count(), 0);
    }

    #[test]
    fn test_clear_index() {
        let vs = setup();
        vs.index_image(IndexedImage {
            id: "1".to_string(),
            path: "/img1.jpg".to_string(),
            embedding: None,
            ocr_text: None,
            caption: None,
            tags: vec![],
            metadata_text: String::new(),
            timestamp: Local::now(),
        });
        vs.clear();
        assert_eq!(vs.index_count(), 0);
    }
}
