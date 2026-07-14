use chrono::{TimeZone, Utc};
use nova_memory::MemoryRecord;
use serde::{Deserialize, Serialize};

use crate::error::KnowledgeError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[expect(dead_code)]
pub enum TimelineGranularity {
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub timestamp: i64,
    pub memory_id: String,
    pub title: String,
    pub content_preview: String,
    pub category: String,
    pub importance: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub granularity: String,
    pub time_range: (i64, i64),
    pub entries: Vec<TimelineEntry>,
    pub bucket_label: String,
}

pub struct TimelineGenerator {
    max_entries: usize,
}

impl TimelineGenerator {
    pub fn new(max_entries: usize) -> Self {
        Self { max_entries }
    }

    pub fn generate_daily(
        &self,
        records: &[MemoryRecord],
        date: i64,
    ) -> Result<Timeline, KnowledgeError> {
        let day_start = date - (date % 86400000);
        let day_end = day_start + 86400000;
        let entries: Vec<TimelineEntry> = records
            .iter()
            .filter(|r| r.created_at >= day_start && r.created_at < day_end)
            .map(|r| self.record_to_entry(r))
            .take(self.max_entries)
            .collect();
        let dt = Utc
            .timestamp_millis_opt(day_start)
            .single()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        Ok(Timeline {
            granularity: "daily".to_string(),
            time_range: (day_start, day_end),

            entries,
            bucket_label: dt,
        })
    }

    pub fn generate_weekly(
        &self,
        records: &[MemoryRecord],
        week_start: i64,
    ) -> Result<Timeline, KnowledgeError> {
        let start = week_start - (week_start % 86400000);
        let end = start + 7 * 86400000;
        let entries: Vec<TimelineEntry> = records
            .iter()
            .filter(|r| r.created_at >= start && r.created_at < end)
            .map(|r| self.record_to_entry(r))
            .take(self.max_entries)
            .collect();
        let dt = Utc
            .timestamp_millis_opt(start)
            .single()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        Ok(Timeline {
            granularity: "weekly".to_string(),
            time_range: (start, end),

            entries,
            bucket_label: format!("Week of {}", dt),
        })
    }

    pub fn generate_monthly(
        &self,
        records: &[MemoryRecord],
        year: i32,
        month: u32,
    ) -> Result<Timeline, KnowledgeError> {
        let start = Utc
            .with_ymd_and_hms(year, month, 1, 0, 0, 0)
            .single()
            .map(|d| d.timestamp_millis())
            .ok_or_else(|| KnowledgeError::TimelineNotAvailable("invalid date".to_string()))?;
        let end = if month == 12 {
            Utc.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0)
                .single()
                .map(|d| d.timestamp_millis())
                .unwrap_or(start + 31 * 86400000)
        } else {
            Utc.with_ymd_and_hms(year, month + 1, 1, 0, 0, 0)
                .single()
                .map(|d| d.timestamp_millis())
                .unwrap_or(start + 31 * 86400000)
        };
        let entries: Vec<TimelineEntry> = records
            .iter()
            .filter(|r| r.created_at >= start && r.created_at < end)
            .map(|r| self.record_to_entry(r))
            .take(self.max_entries)
            .collect();
        let label = format!("{}-{:02}", year, month);
        Ok(Timeline {
            granularity: "monthly".to_string(),
            time_range: (start, end),

            entries,
            bucket_label: label,
        })
    }

    pub fn generate_project_timeline(
        &self,
        records: &[MemoryRecord],
        project_keyword: &str,
    ) -> Result<Timeline, KnowledgeError> {
        let lower = project_keyword.to_lowercase();
        let mut filtered: Vec<&MemoryRecord> = records
            .iter()
            .filter(|r| {
                r.content.to_lowercase().contains(&lower)
                    || r.title.to_lowercase().contains(&lower)
                    || r.tags.iter().any(|t| t.to_lowercase() == lower)
            })
            .collect();
        filtered.sort_by_key(|a| a.created_at);
        if filtered.is_empty() {
            return Err(KnowledgeError::TimelineNotAvailable(format!(
                "no memories found for project '{}'",
                project_keyword
            )));
        }
        let start = filtered.first().unwrap().created_at;
        let end = filtered.last().unwrap().created_at;
        let entries: Vec<TimelineEntry> = filtered
            .iter()
            .map(|r| self.record_to_entry(r))
            .take(self.max_entries)
            .collect();
        Ok(Timeline {
            granularity: "project".to_string(),
            time_range: (start, end),

            entries,
            bucket_label: project_keyword.to_string(),
        })
    }

    pub fn generate_conversation_timeline(
        &self,
        records: &[MemoryRecord],
    ) -> Result<Timeline, KnowledgeError> {
        let mut conv: Vec<&MemoryRecord> = records
            .iter()
            .filter(|r| {
                format!("{:?}", r.category).to_lowercase() == "conversation"
                    || r.content.to_lowercase().starts_with("conv:")
            })
            .collect();
        conv.sort_by_key(|a| a.created_at);
        if conv.is_empty() {
            return Err(KnowledgeError::TimelineNotAvailable(
                "no conversation memories found".to_string(),
            ));
        }
        let start = conv.first().unwrap().created_at;
        let end = conv.last().unwrap().created_at;
        let entries: Vec<TimelineEntry> = conv
            .iter()
            .map(|r| self.record_to_entry(r))
            .take(self.max_entries)
            .collect();
        Ok(Timeline {
            granularity: "conversation".to_string(),
            time_range: (start, end),

            entries,
            bucket_label: "Conversations".to_string(),
        })
    }

    fn record_to_entry(&self, record: &MemoryRecord) -> TimelineEntry {
        let preview = if record.content.len() > 100 {
            format!("{}...", &record.content[..100])
        } else {
            record.content.clone()
        };
        TimelineEntry {
            timestamp: record.created_at,
            memory_id: record.id.clone(),
            title: record.title.clone(),
            content_preview: preview,
            category: format!("{:?}", record.category),
            importance: record.importance,
        }
    }
}
