use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrBlock {
    pub text: String,
    pub confidence: f64,
    pub bounding_box: BoundingBox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub confidence: f64,
    pub blocks: Vec<OcrBlock>,
    pub language: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OcrMode {
    Printed,
    Handwriting,
    MultiLanguage,
    Document,
    Receipt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrOptions {
    pub language: String,
    pub mode: OcrMode,
    pub dpi: u32,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            language: "eng".to_string(),
            mode: OcrMode::Printed,
            dpi: 300,
        }
    }
}

#[async_trait]
pub trait OcrEngine: Send + Sync {
    async fn recognize(&self, bytes: &[u8], options: OcrOptions) -> Result<OcrResult>;
    fn supported_languages(&self) -> Vec<String>;
}

pub struct MockOcrEngine;

impl MockOcrEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockOcrEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OcrEngine for MockOcrEngine {
    async fn recognize(&self, _bytes: &[u8], _options: OcrOptions) -> Result<OcrResult> {
        Ok(OcrResult {
            text: "Mock OCR: Hello World".to_string(),
            confidence: 0.95,
            blocks: vec![OcrBlock {
                text: "Mock OCR: Hello World".to_string(),
                confidence: 0.95,
                bounding_box: BoundingBox {
                    x: 10.0,
                    y: 10.0,
                    w: 200.0,
                    h: 30.0,
                },
            }],
            language: "eng".to_string(),
            duration_ms: 15,
        })
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["eng".to_string()]
    }
}
