use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TagCategory {
    Object,
    Scene,
    Activity,
    Concept,
    Color,
    Text,
    Other(String),
}

impl TagCategory {
    pub fn as_str(&self) -> &str {
        match self {
            TagCategory::Object => "object",
            TagCategory::Scene => "scene",
            TagCategory::Activity => "activity",
            TagCategory::Concept => "concept",
            TagCategory::Color => "color",
            TagCategory::Text => "text",
            TagCategory::Other(s) => s,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualTag {
    pub tag: String,
    pub confidence: f64,
    pub category: TagCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagsResult {
    pub tags: Vec<VisualTag>,
}

#[async_trait]
pub trait VisualTagger: Send + Sync {
    async fn generate_tags(&self, bytes: &[u8]) -> Result<TagsResult>;
}

pub struct MockVisualTagger;

impl MockVisualTagger {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockVisualTagger {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VisualTagger for MockVisualTagger {
    async fn generate_tags(&self, _bytes: &[u8]) -> Result<TagsResult> {
        Ok(TagsResult {
            tags: vec![
                VisualTag {
                    tag: "person".to_string(),
                    confidence: 0.95,
                    category: TagCategory::Object,
                },
                VisualTag {
                    tag: "outdoor".to_string(),
                    confidence: 0.88,
                    category: TagCategory::Scene,
                },
                VisualTag {
                    tag: "daytime".to_string(),
                    confidence: 0.75,
                    category: TagCategory::Concept,
                },
                VisualTag {
                    tag: "blue".to_string(),
                    confidence: 0.60,
                    category: TagCategory::Color,
                },
            ],
        })
    }
}
