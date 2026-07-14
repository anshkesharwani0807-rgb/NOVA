use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectBoundingBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionObject {
    pub label: String,
    pub confidence: f64,
    pub bounding_box: ObjectBoundingBox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    pub objects: Vec<DetectionObject>,
}

#[async_trait]
pub trait ObjectDetector: Send + Sync {
    async fn detect(&self, bytes: &[u8]) -> Result<DetectionResult>;
}

pub struct MockObjectDetector;

impl MockObjectDetector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockObjectDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ObjectDetector for MockObjectDetector {
    async fn detect(&self, _bytes: &[u8]) -> Result<DetectionResult> {
        Ok(DetectionResult {
            objects: vec![
                DetectionObject {
                    label: "person".to_string(),
                    confidence: 0.95,
                    bounding_box: ObjectBoundingBox {
                        x: 10.0,
                        y: 10.0,
                        w: 50.0,
                        h: 100.0,
                    },
                },
                DetectionObject {
                    label: "chair".to_string(),
                    confidence: 0.80,
                    bounding_box: ObjectBoundingBox {
                        x: 60.0,
                        y: 50.0,
                        w: 30.0,
                        h: 40.0,
                    },
                },
            ],
        })
    }
}
