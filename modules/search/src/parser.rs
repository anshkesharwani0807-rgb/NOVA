//! Basic natural language query parser for the Universal Search index.
//!
//! This module provides a simple way to convert a raw user string into a structured [`SearchQuery`].
//! It supports key-value pairs for filtering (e.g., "tag:work", "cat:notes") and treats
//! the remaining text as the search query.

use crate::document::SearchQuery;

/// A simple parser that extracts filters and search text from a natural language string.
pub struct QueryParser;

impl QueryParser {
    /// Parses a raw string into a [`SearchQuery`].
    ///
    /// Example: "project tag:work cat:notes"
    /// Results in: SearchQuery { text: Some("project"), tags: ["work"], category: Some("notes"), ... }
    pub fn parse(input: &str) -> SearchQuery {
        let mut query = SearchQuery::new();
        let mut text_parts = Vec::new();

        for part in input.split_whitespace() {
            if let Some((key, value)) = part.split_once(':') {
                match key.to_lowercase().as_str() {
                    "tag" | "tags" => {
                        query = query.tag(value);
                    }
                    "cat" | "category" => {
                        query = query.category(value);
                    }
                    "src" | "source" => {
                        query = query.source(value);
                    }
                    _ => {
                        // Unknown key: treat as part of the search text
                        text_parts.push(part);
                    }
                }
            } else {
                text_parts.push(part);
            }
        }

        if !text_parts.is_empty() {
            let text = text_parts.join(" ");
            query = query.text(text);
        }

        query
    }
}
