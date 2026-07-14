use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CaptionStyle {
    Concise,
    Descriptive,
    Detailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionOptions {
    pub style: CaptionStyle,
    pub max_length: usize,
}

impl Default for CaptionOptions {
    fn default() -> Self {
        Self {
            style: CaptionStyle::Descriptive,
            max_length: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionResult {
    pub caption: String,
    pub confidence: f64,
    pub duration_ms: u64,
}

#[async_trait]
pub trait CaptionEngine: Send + Sync {
    async fn generate(&self, bytes: &[u8], options: CaptionOptions) -> Result<CaptionResult>;
}

pub struct MockCaptionEngine;

impl MockCaptionEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockCaptionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CaptionEngine for MockCaptionEngine {
    async fn generate(&self, _bytes: &[u8], _options: CaptionOptions) -> Result<CaptionResult> {
        Ok(CaptionResult {
            caption: "A mock generated caption describing the image content".to_string(),
            confidence: 0.85,
            duration_ms: 25,
        })
    }
}
