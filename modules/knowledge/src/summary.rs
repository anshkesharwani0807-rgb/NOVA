use std::collections::HashMap;

use nova_memory::MemoryRecord;
use serde::{Deserialize, Serialize};

use crate::error::KnowledgeError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub summary_type: String,
    pub target_id: String,
    pub title: String,
    pub content: String,
    pub key_points: Vec<String>,
    pub related_count: usize,
    pub generated_at: i64,
}

pub struct SummaryEngine {
    max_length: usize,
}

impl SummaryEngine {
    pub fn new(max_length: usize) -> Self {
        Self { max_length }
    }

    pub fn summarize_conversation(
        &self,
        records: &[MemoryRecord],
    ) -> Result<Summary, KnowledgeError> {
        if records.is_empty() {
            return Err(KnowledgeError::SummaryFailed(
                "no conversation records".to_string(),
            ));
        }
        let title = records
            .first()
            .map(|r| r.title.clone())
            .unwrap_or_else(|| "Conversation Summary".to_string());
        let mut content = String::new();
        let mut key_points = Vec::new();
        let mut participants = Vec::new();
        for rec in records {
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&rec.content);
            if rec.content.len() > 20 {
                key_points.push(self.truncate(&rec.content, 80));
            }
            if rec.tags.iter().any(|t| t == "person") {
                participants.push(rec.title.clone());
            }
        }
        content = self.truncate(&content, self.max_length);
        if !participants.is_empty() {
            key_points.push(format!("Participants: {}", participants.join(", ")));
        }
        let now = chrono::Utc::now().timestamp_millis();
        Ok(Summary {
            summary_type: "conversation".to_string(),
            target_id: records.first().map(|r| r.id.clone()).unwrap_or_default(),
            title,
            content,
            key_points,
            related_count: records.len(),
            generated_at: now,
        })
    }

    pub fn summarize_project(
        &self,
        records: &[MemoryRecord],
        project_name: &str,
    ) -> Result<Summary, KnowledgeError> {
        if records.is_empty() {
            return Err(KnowledgeError::SummaryFailed(format!(
                "no records for project '{}'",
                project_name
            )));
        }
        let mut content = String::new();
        let mut key_points = Vec::new();
        for rec in records {
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&rec.content);
            if rec.importance >= 5 {
                key_points.push(self.truncate(&rec.content, 100));
            }
        }
        content = self.truncate(&content, self.max_length);
        let now = chrono::Utc::now().timestamp_millis();
        Ok(Summary {
            summary_type: "project".to_string(),
            target_id: project_name.to_string(),
            title: format!("Project Summary: {}", project_name),
            content,
            key_points,
            related_count: records.len(),
            generated_at: now,
        })
    }

    pub fn summarize_daily(
        &self,
        records: &[MemoryRecord],
        date_label: &str,
    ) -> Result<Summary, KnowledgeError> {
        if records.is_empty() {
            return Err(KnowledgeError::SummaryFailed(format!(
                "no records for date '{}'",
                date_label
            )));
        }
        let mut content = String::new();
        let mut key_points = Vec::new();
        let mut category_counts: HashMap<String, usize> = HashMap::new();
        for rec in records {
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&rec.content);
            *category_counts
                .entry(format!("{:?}", rec.category))
                .or_insert(0) += 1;
            if rec.importance >= 6 {
                key_points.push(self.truncate(&rec.content, 100));
            }
        }
        content = self.truncate(&content, self.max_length);
        let mut cats: Vec<String> = category_counts
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect();
        cats.sort();
        key_points.push(format!("Activities: {}", cats.join(", ")));
        let now = chrono::Utc::now().timestamp_millis();
        Ok(Summary {
            summary_type: "daily".to_string(),
            target_id: date_label.to_string(),
            title: format!("Daily Summary: {}", date_label),
            content,
            key_points,
            related_count: records.len(),
            generated_at: now,
        })
    }

    pub fn summarize_cluster(
        &self,
        records: &[MemoryRecord],
        cluster_label: &str,
    ) -> Result<Summary, KnowledgeError> {
        if records.is_empty() {
            return Err(KnowledgeError::SummaryFailed(format!(
                "no records in cluster '{}'",
                cluster_label
            )));
        }
        let mut content = String::new();
        let mut key_points = Vec::new();
        for rec in records.iter().take(10) {
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&rec.content);
            if rec.importance >= 4 {
                key_points.push(self.truncate(&rec.content, 80));
            }
        }
        content = self.truncate(&content, self.max_length);
        let now = chrono::Utc::now().timestamp_millis();
        Ok(Summary {
            summary_type: "cluster".to_string(),
            target_id: cluster_label.to_string(),
            title: format!("Memory Cluster: {}", cluster_label),
            content,
            key_points,
            related_count: records.len(),
            generated_at: now,
        })
    }

    fn truncate(&self, s: &str, max: usize) -> String {
        if s.len() <= max {
            s.to_string()
        } else {
            format!("{}...", &s[..max])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_memory::{MemoryCategory, MemoryRecord};

    fn make_record(content: &str, title: &str, importance: i32) -> MemoryRecord {
        MemoryRecord::new(MemoryCategory::Knowledge, title, content).with_importance(importance)
    }

    #[test]
    fn test_summarize_conversation() {
        let engine = SummaryEngine::new(500);
        let records = vec![make_record("Hello how are you?", "Alice", 5)];
        let summary = engine.summarize_conversation(&records).unwrap();
        assert_eq!(summary.summary_type, "conversation");
        assert!(!summary.content.is_empty());
    }

    #[test]
    fn test_summarize_conversation_empty() {
        let engine = SummaryEngine::new(500);
        let result = engine.summarize_conversation(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_summarize_project() {
        let engine = SummaryEngine::new(500);
        let records = vec![
            make_record("Implemented feature X", "Feature", 6),
            make_record("Fixed bug Y", "Bug fix", 4),
        ];
        let summary = engine.summarize_project(&records, "Project Alpha").unwrap();
        assert_eq!(summary.summary_type, "project");
        assert!(summary.title.contains("Project Alpha"));
    }

    #[test]
    fn test_summarize_project_empty() {
        let engine = SummaryEngine::new(500);
        let result = engine.summarize_project(&[], "Empty");
        assert!(result.is_err());
    }

    #[test]
    fn test_summarize_daily() {
        let engine = SummaryEngine::new(500);
        let records = vec![
            make_record("Meeting with team", "Meeting", 7),
            make_record("Code review done", "Review", 5),
        ];
        let summary = engine.summarize_daily(&records, "2026-07-15").unwrap();
        assert_eq!(summary.summary_type, "daily");
    }

    #[test]
    fn test_summarize_daily_empty() {
        let engine = SummaryEngine::new(500);
        let result = engine.summarize_daily(&[], "today");
        assert!(result.is_err());
    }

    #[test]
    fn test_summarize_cluster() {
        let engine = SummaryEngine::new(500);
        let records = vec![
            make_record("First memory in cluster", "Mem1", 5),
            make_record("Second memory in cluster", "Mem2", 3),
        ];
        let summary = engine.summarize_cluster(&records, "Test Cluster").unwrap();
        assert_eq!(summary.summary_type, "cluster");
    }

    #[test]
    fn test_truncate() {
        let engine = SummaryEngine::new(10);
        let short = engine.truncate("hello", 10);
        assert_eq!(short, "hello");
        let long = engine.truncate("hello world this is long", 10);
        assert_eq!(long, "hello worl...");
    }

    #[test]
    fn test_summary_serialization() {
        let engine = SummaryEngine::new(500);
        let records = vec![make_record("Test content", "Title", 5)];
        let summary = engine.summarize_conversation(&records).unwrap();
        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: Summary = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.summary_type, "conversation");
    }
}
