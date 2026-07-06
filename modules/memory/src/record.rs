//! Memory record model, categories, and query types (Milestone 4).

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// The category a memory belongs to. Stored as a stable string in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryCategory {
    Conversation,
    Reminder,
    Contact,
    Preference,
    Knowledge,
    Gallery,
    Music,
    SearchHistory,
    Automation,
    Plugin,
    Device,
    Calendar,
    Custom,
}

impl MemoryCategory {
    /// Stable string form used for storage and filtering.
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryCategory::Conversation => "conversation",
            MemoryCategory::Reminder => "reminder",
            MemoryCategory::Contact => "contact",
            MemoryCategory::Preference => "preference",
            MemoryCategory::Knowledge => "knowledge",
            MemoryCategory::Gallery => "gallery",
            MemoryCategory::Music => "music",
            MemoryCategory::SearchHistory => "search_history",
            MemoryCategory::Automation => "automation",
            MemoryCategory::Plugin => "plugin",
            MemoryCategory::Device => "device",
            MemoryCategory::Calendar => "calendar",
            MemoryCategory::Custom => "custom",
        }
    }

    /// Parse the stable string form; unknown values map to `Custom`.
    pub fn from_stored(s: &str) -> Self {
        match s {
            "conversation" => MemoryCategory::Conversation,
            "reminder" => MemoryCategory::Reminder,
            "contact" => MemoryCategory::Contact,
            "preference" => MemoryCategory::Preference,
            "knowledge" => MemoryCategory::Knowledge,
            "gallery" => MemoryCategory::Gallery,
            "music" => MemoryCategory::Music,
            "search_history" => MemoryCategory::SearchHistory,
            "automation" => MemoryCategory::Automation,
            "plugin" => MemoryCategory::Plugin,
            "device" => MemoryCategory::Device,
            "calendar" => MemoryCategory::Calendar,
            _ => MemoryCategory::Custom,
        }
    }
}

/// Current unix time in milliseconds.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A single stored memory. Sensitive fields (`title`, `content`, `tags`, `source`) are
/// encrypted at rest; the remaining fields are operational metadata used for indexing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub category: MemoryCategory,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub importance: i32,
    pub source: String,
    pub device_id: String,
    pub correlation_id: Option<String>,
    pub version: i64,
    pub deleted: bool,
}

impl MemoryRecord {
    /// Create a new record with a fresh UUID and current timestamps.
    pub fn new(
        category: MemoryCategory,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let now = now_millis();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            category,
            title: title.into(),
            content: content.into(),
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
            importance: 0,
            source: String::new(),
            device_id: String::new(),
            correlation_id: None,
            version: 1,
            deleted: false,
        }
    }

    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_importance(mut self, importance: i32) -> Self {
        self.importance = importance;
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    pub fn with_device_id(mut self, device_id: impl Into<String>) -> Self {
        self.device_id = device_id.into();
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }
}

/// Text match mode for search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Exact,
    Contains,
    Prefix,
}

/// Result ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    CreatedAtAsc,
    CreatedAtDesc,
    UpdatedAtDesc,
    ImportanceDesc,
}

/// A query over stored memories combining metadata filters and text/tag search.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pub text: Option<String>,
    pub mode: Option<SearchMode>,
    pub category: Option<MemoryCategory>,
    pub tags: Vec<String>,
    pub include_deleted: bool,
    pub case_insensitive: bool,
    pub sort: Option<SortBy>,
    pub limit: Option<usize>,
    pub offset: usize,
}

impl Query {
    /// A new query with case-insensitive matching enabled by default.
    pub fn new() -> Self {
        Self {
            case_insensitive: true,
            ..Self::default()
        }
    }

    pub fn category(mut self, category: MemoryCategory) -> Self {
        self.category = Some(category);
        self
    }

    pub fn contains(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self.mode = Some(SearchMode::Contains);
        self
    }

    pub fn exact(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self.mode = Some(SearchMode::Exact);
        self
    }

    pub fn prefix(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self.mode = Some(SearchMode::Prefix);
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn include_deleted(mut self, include: bool) -> Self {
        self.include_deleted = include;
        self
    }

    pub fn case_sensitive(mut self) -> Self {
        self.case_insensitive = false;
        self
    }

    pub fn sort(mut self, sort: SortBy) -> Self {
        self.sort = Some(sort);
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }
}

/// An operation applied atomically inside a [`crate::store::Store`] transaction.
#[derive(Debug, Clone)]
pub enum MemoryOp {
    Insert(MemoryRecord),
    Update(MemoryRecord),
    SoftDelete(String),
    Restore(String),
}
