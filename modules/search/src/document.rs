//! Universal search document model, query types, and future-capability interfaces
//! (Milestone 5, ADR-0006).
//!
//! An [`IndexDocument`] is a source-agnostic searchable record. Memory records are the  
//! first source; the same shape indexes gallery, files, notes, contacts, calendar,      
//! browser history, voice history, OCR, AI memories, and plugins later without redesign.

use nova_memory::MemoryRecord;
use serde::{Deserialize, Serialize};

/// A single indexed document, identified by its `(source, source_id)` pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexDocument {
    /// Origin subsystem, e.g. `"memory"`, `"gallery"`, `"files"`.
    pub source: String,
    /// Stable id within the source (e.g. the memory record's UUID).
    pub source_id: String,
    pub category: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    /// The origin/source label of the underlying record (its provenance).
    pub source_field: String,
    /// Free-form JSON metadata carried for display/filtering.
    pub metadata: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub importance: i32,
    /// Semantic embedding vector (M5 Hybrid Search).
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
}

impl IndexDocument {
    /// The globally-unique document id (`"{source}:{source_id}"`), used to prevent
    /// duplicate index rows.
    pub fn doc_id(&self) -> String {
        format!("{}:{}", self.source, self.source_id)
    }

    /// Lowercased concatenation of the searchable fields, used for matching.
    pub fn norm_text(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.title);
        s.push(' ');
        s.push_str(&self.content);
        s.push(' ');
        s.push_str(&self.tags.join(" "));
        s.push(' ');
        s.push_str(&self.source_field);
        s.to_lowercase()
    }

    /// Build an index document from a memory record.
    pub fn from_memory(rec: &MemoryRecord) -> Self {
        let metadata = serde_json::json!({
            "device_id": rec.device_id,
            "importance": rec.importance,
            "version": rec.version,
            "correlation_id": rec.correlation_id,
        })
        .to_string();
        Self {
            source: "memory".to_string(),
            source_id: rec.id.clone(),
            category: rec.category.as_str().to_string(),
            title: rec.title.clone(),
            content: rec.content.clone(),
            tags: rec.tags.clone(),
            source_field: rec.source.clone(),
            metadata,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
            importance: rec.importance,
            embedding: None,
        }
    }
}

/// A ranked search hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub document: IndexDocument,
    pub score: f64,
}

/// How the query text is matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum MatchMode {
    /// The whole query equals a field (title or content).
    Exact,
    /// Substring match anywhere (default).
    Partial,
    /// Word/prefix match.
    Prefix,
    /// The whole quoted phrase must appear as a substring.
    Phrase,
}

/// How multiple query words are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum Combine {
    And,
    Or,
}

/// A universal search query.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SearchQuery {
    pub text: Option<String>,
    pub mode: MatchMode,
    pub combine: Combine,
    pub case_insensitive: bool,
    pub tags: Vec<String>,
    pub source: Option<String>,
    pub category: Option<String>,
    pub date_from: Option<i64>,
    pub date_to: Option<i64>,
    pub limit: Option<usize>,
    pub offset: usize,
    pub embedding: Option<Vec<f32>>,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            text: None,
            mode: MatchMode::Partial,
            combine: Combine::And,
            case_insensitive: true,
            tags: Vec::new(),
            source: None,
            category: None,
            date_from: None,
            date_to: None,
            limit: None,
            offset: 0,
            embedding: None,
        }
    }
}

impl SearchQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn partial(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            mode: MatchMode::Partial,
            ..Self::default()
        }
    }

    pub fn exact(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            mode: MatchMode::Exact,
            ..Self::default()
        }
    }

    pub fn prefix(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            mode: MatchMode::Prefix,
            ..Self::default()
        }
    }

    pub fn phrase(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            mode: MatchMode::Phrase,
            ..Self::default()
        }
    }

    /// Parse a natural-language query string into a structured [`SearchQuery`] (M5 basic
    /// query interface). Supports inline filters and quoted phrases:
    ///
    /// - `tag:foo` / `#foo`        → add a tag filter
    /// - `source:memory`           → restrict to a source
    /// - `category:note`           → restrict to a category
    /// - `"exact phrase"`          → phrase match on the quoted text
    /// - everything else           → free-text terms (partial match)
    ///
    /// Unknown `key:value` tokens are treated as free text so the parser never rejects
    /// input. A quoted phrase selects [`MatchMode::Phrase`]; otherwise partial match.
    pub fn parse(input: &str) -> Self {
        let mut query = SearchQuery::new();
        let mut terms: Vec<String> = Vec::new();
        let mut phrase: Option<String> = None;

        let mut rest = input.trim();
        while !rest.is_empty() {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }
            // Quoted phrase.
            if let Some(after) = rest.strip_prefix('"') {
                if let Some(end) = after.find('"') {
                    let p = after[..end].trim();
                    if !p.is_empty() {
                        phrase = Some(p.to_string());
                    }
                    rest = &after[end + 1..];
                    continue;
                }
            }
            // Next whitespace-delimited token.
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let token = &rest[..end];
            rest = &rest[end..];

            if let Some(tag) = token.strip_prefix('#') {
                if !tag.is_empty() {
                    query.tags.push(tag.to_lowercase());
                }
            } else if let Some((key, value)) = token.split_once(':') {
                if value.is_empty() {
                    terms.push(token.to_string());
                    continue;
                }
                match key.to_ascii_lowercase().as_str() {
                    "tag" => query.tags.push(value.to_lowercase()),
                    "source" => query.source = Some(value.to_string()),
                    "category" => query.category = Some(value.to_string()),
                    _ => terms.push(token.to_string()),
                }
            } else {
                terms.push(token.to_string());
            }
        }

        if let Some(p) = phrase {
            let text = if terms.is_empty() {
                p
            } else {
                format!("{} {}", p, terms.join(" "))
            };
            query.text = Some(text);
            query.mode = MatchMode::Phrase;
        } else if !terms.is_empty() {
            query.text = Some(terms.join(" "));
            query.mode = MatchMode::Partial;
        }
        query
    }

    pub fn any_word(mut self) -> Self {
        self.combine = Combine::Or;
        self
    }

    pub fn all_words(mut self) -> Self {
        self.combine = Combine::And;
        self
    }

    pub fn case_sensitive(mut self) -> Self {
        self.case_insensitive = false;
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    pub fn date_range(mut self, from: Option<i64>, to: Option<i64>) -> Self {
        self.date_from = from;
        self.date_to = to;
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

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

// â”€â”€ Future-capability interfaces (design only; not implemented in M5) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
//
// These traits reserve the seams for AI-powered search so later milestones can add
// implementations without redesigning the engine. None are wired up yet.

/// Turns text (or other modalities) into an embedding vector. Implemented by a future
/// on-device embedding model (Chapter 11 / M6).
pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> nova_kernel::Result<Vec<f32>>;
}

/// A nearest-neighbour vector index (future semantic backend).
pub trait VectorIndex: Send + Sync {
    fn upsert_embedding(&self, doc_id: &str, embedding: &[f32]) -> nova_kernel::Result<()>;
    fn remove_embedding(&self, doc_id: &str) -> nova_kernel::Result<()>;
    fn search_vector(&self, embedding: &[f32], k: usize)
        -> nova_kernel::Result<Vec<(String, f32)>>;
}

/// Semantic (meaning-based) search over indexed documents (future).
pub trait SemanticSearch: Send + Sync {
    fn semantic_search(&self, query: &str, k: usize) -> nova_kernel::Result<Vec<SearchResult>>;
}

/// Visual search seams (future): image similarity, OCR text, face and object search.
pub trait VisualSearch: Send + Sync {
    fn image_search(&self, image: &[u8], k: usize) -> nova_kernel::Result<Vec<SearchResult>>;
    fn ocr_search(&self, query: &str, k: usize) -> nova_kernel::Result<Vec<SearchResult>>;
    fn face_search(&self, face: &[u8], k: usize) -> nova_kernel::Result<Vec<SearchResult>>;
    fn object_search(&self, label: &str, k: usize) -> nova_kernel::Result<Vec<SearchResult>>;
}
