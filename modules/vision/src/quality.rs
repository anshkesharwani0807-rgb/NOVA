use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlurLevel {
    None,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverallQuality {
    Excellent,
    Good,
    Fair,
    Poor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityResult {
    pub blur_score: f64,
    pub brightness: f64,
    pub contrast: f64,
    pub noise: f64,
    pub aesthetics: f64,
    pub is_blurry: bool,
    pub overall: OverallQuality,
    pub blur_level: BlurLevel,
}

#[async_trait]
pub trait QualityAnalyzer: Send + Sync {
    async fn analyze(&self, bytes: &[u8]) -> Result<QualityResult>;
}

#[async_trait]
pub trait BlurDetector: Send + Sync {
    async fn detect_blur(&self, bytes: &[u8]) -> Result<BlurLevel>;
}

pub struct MockQualityAnalyzer;

impl MockQualityAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockQualityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl QualityAnalyzer for MockQualityAnalyzer {
    async fn analyze(&self, _bytes: &[u8]) -> Result<QualityResult> {
        Ok(QualityResult {
            blur_score: 0.05,
            brightness: 0.6,
            contrast: 0.7,
            noise: 0.02,
            aesthetics: 0.8,
            is_blurry: false,
            overall: OverallQuality::Good,
            blur_level: BlurLevel::None,
        })
    }
}

#[async_trait]
impl BlurDetector for MockQualityAnalyzer {
    async fn detect_blur(&self, _bytes: &[u8]) -> Result<BlurLevel> {
        Ok(BlurLevel::None)
    }
}
