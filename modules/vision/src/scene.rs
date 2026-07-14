use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SceneLabel {
    Indoors,
    Outdoors,
    Nature,
    Urban,
    Food,
    Document,
    Portrait,
    Selfie,
    Sunset,
    Night,
    Sports,
    Screenshot,
    Other(String),
}

impl SceneLabel {
    pub fn as_str(&self) -> &str {
        match self {
            SceneLabel::Indoors => "indoors",
            SceneLabel::Outdoors => "outdoors",
            SceneLabel::Nature => "nature",
            SceneLabel::Urban => "urban",
            SceneLabel::Food => "food",
            SceneLabel::Document => "document",
            SceneLabel::Portrait => "portrait",
            SceneLabel::Selfie => "selfie",
            SceneLabel::Sunset => "sunset",
            SceneLabel::Night => "night",
            SceneLabel::Sports => "sports",
            SceneLabel::Screenshot => "screenshot",
            SceneLabel::Other(s) => s,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneClassification {
    pub label: SceneLabel,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneResult {
    pub scenes: Vec<SceneClassification>,
}

#[async_trait]
pub trait SceneClassifier: Send + Sync {
    async fn classify(&self, bytes: &[u8]) -> Result<SceneResult>;
}

pub struct MockSceneClassifier;

impl MockSceneClassifier {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockSceneClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SceneClassifier for MockSceneClassifier {
    async fn classify(&self, _bytes: &[u8]) -> Result<SceneResult> {
        Ok(SceneResult {
            scenes: vec![
                SceneClassification {
                    label: SceneLabel::Outdoors,
                    confidence: 0.88,
                },
                SceneClassification {
                    label: SceneLabel::Nature,
                    confidence: 0.72,
                },
            ],
        })
    }
}
