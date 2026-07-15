use std::collections::HashSet;

use nova_memory::MemoryRecord;

use crate::config::KnowledgeConfig;
use crate::error::KnowledgeError;

pub struct AnalyzedMemory {
    pub memory_id: String,
    pub category: String,
    pub tags: Vec<String>,
    pub importance: i32,
    pub confidence: f64,
    pub is_duplicate: bool,
    pub duplicate_of: Option<String>,
    pub linked_ids: Vec<String>,
    pub entities: Vec<ExtractedEntity>,
    pub normalized_timestamp: i64,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: EntityType,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq)]
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

pub struct MemoryAnalyzer {
    config: KnowledgeConfig,
}

impl MemoryAnalyzer {
    pub fn new(config: KnowledgeConfig) -> Self {
        Self { config }
    }

    pub fn analyze(&self, record: &MemoryRecord) -> Result<AnalyzedMemory, KnowledgeError> {
        let content = &record.content;
        let title = &record.title;

        let category = self.categorize(content, title, &record.category);
        let tags = self.extract_tags(content, title, &category);
        let importance = self.score_importance(record, &tags.len());
        let entities = self.extract_entities(content, title);
        let linked_ids = Vec::new();

        Ok(AnalyzedMemory {
            memory_id: record.id.clone(),
            category,
            tags,
            importance,
            confidence: 0.9,
            is_duplicate: false,
            duplicate_of: None,
            linked_ids,
            entities,
            normalized_timestamp: record.created_at,
            source: record.source.clone(),
        })
    }

    fn categorize(
        &self,
        content: &str,
        title: &str,
        existing: &nova_memory::MemoryCategory,
    ) -> String {
        if existing != &nova_memory::MemoryCategory::Custom {
            return format!("{:?}", existing);
        }
        let lower = format!("{} {}", title, content).to_lowercase();

        if lower.contains("remind")
            || lower.contains("todo")
            || lower.contains("task")
            || lower.contains("deadline")
        {
            return "Reminder".to_string();
        }
        if lower.contains("call")
            || lower.contains("meet")
            || lower.contains("said")
            || lower.contains("asked")
        {
            return "Conversation".to_string();
        }
        if lower.contains("project")
            || lower.contains("feature")
            || lower.contains("code")
            || lower.contains("bug")
        {
            return "Project".to_string();
        }
        if lower.contains("idea")
            || lower.contains("maybe")
            || lower.contains("could")
            || lower.contains("what if")
        {
            return "Idea".to_string();
        }
        if lower.contains("photo")
            || lower.contains("image")
            || lower.contains("picture")
            || lower.contains("gallery")
        {
            return "Gallery".to_string();
        }
        if lower.contains("contact")
            || lower.contains("email")
            || lower.contains("phone")
            || lower.starts_with("person:")
        {
            return "Contact".to_string();
        }
        if lower.contains("document") || lower.contains("file") || lower.contains("note") {
            return "Knowledge".to_string();
        }
        if lower.contains("search") || lower.contains("find") || lower.contains("look up") {
            return "SearchHistory".to_string();
        }
        "Knowledge".to_string()
    }

    fn extract_tags(&self, content: &str, title: &str, _category: &str) -> Vec<String> {
        let mut tags = Vec::new();
        let text = format!("{} {}", title, content);

        if let Some(start) = text.find('#') {
            for word in text[start..].split_whitespace() {
                if word.starts_with('#') && word.len() > 1 {
                    let tag = word[1..].trim_end_matches(|c: char| c.is_ascii_punctuation());
                    if !tag.is_empty() {
                        tags.push(tag.to_lowercase());
                    }
                }
            }
        }

        let lower = text.to_lowercase();
        if lower.contains("rust") {
            tags.push("rust".to_string());
        }
        if lower.contains("python") {
            tags.push("python".to_string());
        }
        if lower.contains("project") {
            tags.push("project".to_string());
        }
        if lower.contains("idea") {
            tags.push("idea".to_string());
        }
        if lower.contains("meeting") {
            tags.push("meeting".to_string());
        }
        if lower.contains("task") || lower.contains("todo") {
            tags.push("task".to_string());
        }

        tags.sort();
        tags.dedup();
        tags
    }

    fn score_importance(&self, record: &MemoryRecord, tag_count: &usize) -> i32 {
        let mut score = 0i32;
        if record.importance != 0 {
            return record.importance;
        }
        if tag_count >= &3 {
            score += 3;
        } else if tag_count >= &1 {
            score += 1;
        }
        if record.title.len() > 20 {
            score += 1;
        }
        if record.content.len() > 200 {
            score += 2;
        } else if record.content.len() > 50 {
            score += 1;
        }
        if !record.source.is_empty() && record.source != "manual" {
            score += 1;
        }
        score.clamp(0, 10)
    }

    fn extract_entities(&self, content: &str, title: &str) -> Vec<ExtractedEntity> {
        let mut entities = Vec::new();
        let text = format!("{} {}", title, content);
        let lower = text.to_lowercase();

        if text.contains("Rust") || text.contains("NOVA") {
            entities.push(ExtractedEntity {
                name: if text.contains("NOVA") {
                    "NOVA".to_string()
                } else {
                    "Rust".to_string()
                },
                entity_type: EntityType::Project,
                confidence: 0.9,
            });
        }
        if lower.contains("python") {
            entities.push(ExtractedEntity {
                name: "Python".to_string(),
                entity_type: EntityType::Technology,
                confidence: 0.8,
            });
        }
        if lower.contains("gallery") {
            entities.push(ExtractedEntity {
                name: "Gallery".to_string(),
                entity_type: EntityType::Project,
                confidence: 0.7,
            });
        }

        entities
    }

    pub fn detect_duplicates(&self, records: &[MemoryRecord]) -> Vec<(String, String)> {
        let mut duplicates = Vec::new();
        for i in 0..records.len() {
            for j in (i + 1)..records.len() {
                let sim = self.text_similarity(&records[i].content, &records[j].content);
                if sim >= self.config.dedup_similarity_threshold {
                    duplicates.push((records[i].id.clone(), records[j].id.clone()));
                }
            }
        }
        duplicates
    }

    fn text_similarity(&self, a: &str, b: &str) -> f64 {
        let a_words: HashSet<&str> = a.split_whitespace().collect();
        let b_words: HashSet<&str> = b.split_whitespace().collect();
        if a_words.is_empty() && b_words.is_empty() {
            return 1.0;
        }
        let intersection = a_words.intersection(&b_words).count();
        let union = a_words.union(&b_words).count();
        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }

    pub fn suggest_links(&self, records: &[MemoryRecord]) -> Vec<(String, String, String)> {
        let mut links = Vec::new();
        for i in 0..records.len() {
            for j in (i + 1)..records.len() {
                let a = &records[i];
                let b = &records[j];
                if a.tags.iter().any(|t| b.tags.contains(t)) {
                    links.push((a.id.clone(), b.id.clone(), "shared_tags".to_string()));
                }
                if a.title.to_lowercase().contains(&b.title.to_lowercase())
                    || b.title.to_lowercase().contains(&a.title.to_lowercase())
                {
                    links.push((a.id.clone(), b.id.clone(), "related_title".to_string()));
                }
            }
        }
        links
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_memory::{MemoryCategory, MemoryRecord};

    fn make_record(content: &str, title: &str, category: MemoryCategory) -> MemoryRecord {
        MemoryRecord::new(category, title, content)
    }

    #[test]
    fn test_analyze_basic() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let record = make_record(
            "Working on Rust project",
            "Project Update",
            MemoryCategory::Custom,
        );
        let result = analyzer.analyze(&record).unwrap();
        assert_eq!(result.category, "Project");
        assert!(!result.tags.is_empty());
        assert!(result.tags.contains(&"rust".to_string()));
    }

    #[test]
    fn test_analyze_reminder() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let record = make_record("Remember to buy milk", "Reminder", MemoryCategory::Custom);
        let result = analyzer.analyze(&record).unwrap();
        assert_eq!(result.category, "Reminder");
    }

    #[test]
    fn test_analyze_conversation() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let record = make_record("Alice said hello", "Chat", MemoryCategory::Custom);
        let result = analyzer.analyze(&record).unwrap();
        assert_eq!(result.category, "Conversation");
    }

    #[test]
    fn test_analyze_idea() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let record = make_record("What if we could do this?", "Idea", MemoryCategory::Custom);
        let result = analyzer.analyze(&record).unwrap();
        assert_eq!(result.category, "Idea");
    }

    #[test]
    fn test_analyze_gallery() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let record = make_record(
            "Beautiful photo from the trip",
            "Photo",
            MemoryCategory::Custom,
        );
        let result = analyzer.analyze(&record).unwrap();
        assert_eq!(result.category, "Gallery");
    }

    #[test]
    fn test_analyze_existing_category_preserved() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let record = make_record("Some content", "Title", MemoryCategory::Reminder);
        let result = analyzer.analyze(&record).unwrap();
        assert_eq!(result.category, "Reminder");
    }

    #[test]
    fn test_extract_entities_from_memory() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let record = make_record("NOVA is built with Rust", "Project", MemoryCategory::Custom);
        let result = analyzer.analyze(&record).unwrap();
        assert!(result.entities.iter().any(|e| e.name == "NOVA"));
    }

    #[test]
    fn test_detect_duplicates_no_records() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let dups = analyzer.detect_duplicates(&[]);
        assert!(dups.is_empty());
    }

    #[test]
    fn test_detect_duplicates_exact_match() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let records = vec![
            make_record("hello world", "Title", MemoryCategory::Knowledge),
            make_record("hello world", "Title", MemoryCategory::Knowledge),
        ];
        let dups = analyzer.detect_duplicates(&records);
        assert!(!dups.is_empty());
    }

    #[test]
    fn test_suggest_links_shared_tags() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let mut r1 = make_record("content", "Title1", MemoryCategory::Knowledge);
        r1.tags = vec!["rust".into()];
        let mut r2 = make_record("content", "Title2", MemoryCategory::Knowledge);
        r2.tags = vec!["rust".into()];
        let links = analyzer.suggest_links(&[r1, r2]);
        assert!(!links.is_empty());
    }

    #[test]
    fn test_suggest_links_empty() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let r1 = make_record("alpha", "Title1", MemoryCategory::Knowledge);
        let r2 = make_record("beta", "Title2", MemoryCategory::Knowledge);
        let links = analyzer.suggest_links(&[r1, r2]);
        assert!(links.is_empty());
    }

    #[test]
    fn test_importance_scoring() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let record = make_record(
            "A very long content that should get a high importance score because it has many details",
            "Important Project Update with Details",
            MemoryCategory::Custom,
        );
        let result = analyzer.analyze(&record).unwrap();
        assert!(result.importance > 0);
    }

    #[test]
    fn test_importance_preserved() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let mut record = make_record("content", "title", MemoryCategory::Knowledge);
        record.importance = 10;
        let result = analyzer.analyze(&record).unwrap();
        assert_eq!(result.importance, 10);
    }

    #[test]
    fn test_tag_extraction_from_hashtags() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let record = make_record(
            "Working on #rust and #python",
            "Dev",
            MemoryCategory::Custom,
        );
        let result = analyzer.analyze(&record).unwrap();
        assert!(result.tags.contains(&"rust".to_string()));
        assert!(result.tags.contains(&"python".to_string()));
    }

    #[test]
    fn test_text_similarity_identical() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let sim = analyzer.text_similarity("hello world", "hello world");
        assert!((sim - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_text_similarity_empty() {
        let config = KnowledgeConfig::default();
        let analyzer = MemoryAnalyzer::new(config);
        let sim = analyzer.text_similarity("", "");
        assert!((sim - 1.0).abs() < 0.01);
    }
}
