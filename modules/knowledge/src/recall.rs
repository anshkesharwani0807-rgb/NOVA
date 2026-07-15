use chrono::Utc;
use nova_search::{SearchQuery, SearchResult, UniversalSearch};
use serde::{Deserialize, Serialize};

use crate::error::KnowledgeError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallQuery {
    pub text: String,
    pub time_range: Option<TimeRange>,
    pub category_filter: Option<String>,
    pub tag_filter: Option<String>,
    pub max_results: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: Option<i64>,
    pub end: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallResult {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub total_count: usize,
    pub time_range: Option<String>,
}

pub struct SmartRecall {
    search: std::sync::Arc<UniversalSearch>,
}

impl SmartRecall {
    pub fn new(search: std::sync::Arc<UniversalSearch>) -> Self {
        Self { search }
    }

    pub fn recall(&self, query: &RecallQuery) -> Result<RecallResult, KnowledgeError> {
        let search_query = self.build_search_query(query);
        let results = self
            .search
            .search(&search_query)
            .map_err(|e| KnowledgeError::RecallFailed(e.to_string()))?;
        let time_desc = query.time_range.as_ref().map(|tr| {
            let start = tr
                .start
                .map(|t| {
                    chrono::DateTime::from_timestamp_millis(t)
                        .map(|d| d.format("%Y-%m-%d").to_string())
                        .unwrap_or_default()
                })
                .unwrap_or_else(|| "any".to_string());
            let end = tr
                .end
                .map(|t| {
                    chrono::DateTime::from_timestamp_millis(t)
                        .map(|d| d.format("%Y-%m-%d").to_string())
                        .unwrap_or_default()
                })
                .unwrap_or_else(|| "now".to_string());
            format!("{} to {}", start, end)
        });
        Ok(RecallResult {
            query: query.text.clone(),
            total_count: results.len(),
            results,
            time_range: time_desc,
        })
    }

    pub fn recall_text(&self, text: &str, limit: usize) -> Result<RecallResult, KnowledgeError> {
        let query = RecallQuery {
            text: text.to_string(),
            time_range: None,
            category_filter: None,
            tag_filter: None,
            max_results: limit,
        };
        self.recall(&query)
    }

    pub fn recall_last_week(
        &self,
        text: &str,
        limit: usize,
    ) -> Result<RecallResult, KnowledgeError> {
        let now = Utc::now().timestamp_millis();
        let week_ago = now - 7 * 24 * 60 * 60 * 1000;
        let query = RecallQuery {
            text: text.to_string(),
            time_range: Some(TimeRange {
                start: Some(week_ago),
                end: Some(now),
            }),
            category_filter: None,
            tag_filter: None,
            max_results: limit,
        };
        self.recall(&query)
    }

    pub fn recall_by_category(
        &self,
        text: &str,
        category: &str,
        limit: usize,
    ) -> Result<RecallResult, KnowledgeError> {
        let query = RecallQuery {
            text: format!("category:{} {}", category, text),
            time_range: None,
            category_filter: Some(category.to_string()),
            tag_filter: None,
            max_results: limit,
        };
        self.recall(&query)
    }

    fn build_search_query(&self, query: &RecallQuery) -> SearchQuery {
        let mut sq = SearchQuery::partial(&query.text);
        sq = sq.limit(query.max_results);
        if let Some(cat) = &query.category_filter {
            sq = sq.category(cat);
        }
        if let Some(tag) = &query.tag_filter {
            sq = sq.tag(tag);
        }
        sq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recall_query_builder() {
        let query = RecallQuery {
            text: "rust project".into(),
            time_range: None,
            category_filter: None,
            tag_filter: None,
            max_results: 10,
        };
        assert_eq!(query.text, "rust project");
        assert_eq!(query.max_results, 10);
    }

    #[test]
    fn test_recall_query_with_filters() {
        let query = RecallQuery {
            text: "meeting".into(),
            time_range: Some(TimeRange {
                start: Some(1000),
                end: Some(2000),
            }),
            category_filter: Some("Conversation".into()),
            tag_filter: Some("work".into()),
            max_results: 5,
        };
        assert!(query.time_range.is_some());
        assert!(query.category_filter.is_some());
        assert!(query.tag_filter.is_some());
    }

    #[test]
    fn test_time_range_default() {
        let tr = TimeRange {
            start: None,
            end: None,
        };
        assert!(tr.start.is_none());
        assert!(tr.end.is_none());
    }

    #[test]
    fn test_recall_result_empty() {
        let result = RecallResult {
            query: "test".into(),
            results: vec![],
            total_count: 0,
            time_range: None,
        };
        assert_eq!(result.total_count, 0);
        assert!(result.results.is_empty());
    }
}
