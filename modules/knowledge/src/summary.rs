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
