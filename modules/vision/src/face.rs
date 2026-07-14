use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

use crate::detection::ObjectBoundingBox;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceLandmarks {
    pub left_eye: (f64, f64),
    pub right_eye: (f64, f64),
    pub nose: (f64, f64),
    pub mouth_left: (f64, f64),
    pub mouth_right: (f64, f64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceEncoding {
    pub vector: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedFace {
    pub bounding_box: ObjectBoundingBox,
    pub landmarks: Option<FaceLandmarks>,
    pub confidence: f64,
    pub encoding: Option<FaceEncoding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceDetectionResult {
    pub faces: Vec<DetectedFace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceCluster {
    pub id: String,
    pub face_ids: Vec<String>,
    pub centroid: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceClusteringResult {
    pub clusters: Vec<FaceCluster>,
    pub num_clusters: usize,
}

#[async_trait]
pub trait FaceEngine: Send + Sync {
    async fn detect(&self, bytes: &[u8]) -> Result<FaceDetectionResult>;
    async fn encode(&self, bytes: &[u8], faces: &[DetectedFace]) -> Result<Vec<FaceEncoding>>;
    async fn cluster(&self, encodings: &[FaceEncoding]) -> Result<FaceClusteringResult>;
}

pub struct MockFaceEngine;

impl MockFaceEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockFaceEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FaceEngine for MockFaceEngine {
    async fn detect(&self, _bytes: &[u8]) -> Result<FaceDetectionResult> {
        Ok(FaceDetectionResult {
            faces: vec![DetectedFace {
                bounding_box: ObjectBoundingBox {
                    x: 20.0,
                    y: 20.0,
                    w: 30.0,
                    h: 30.0,
                },
                landmarks: Some(FaceLandmarks {
                    left_eye: (25.0, 25.0),
                    right_eye: (45.0, 25.0),
                    nose: (35.0, 35.0),
                    mouth_left: (28.0, 45.0),
                    mouth_right: (42.0, 45.0),
                }),
                confidence: 0.98,
                encoding: Some(FaceEncoding {
                    vector: vec![0.2f64; 128],
                }),
            }],
        })
    }

    async fn encode(&self, _bytes: &[u8], faces: &[DetectedFace]) -> Result<Vec<FaceEncoding>> {
        Ok(faces
            .iter()
            .map(|f| {
                f.encoding.clone().unwrap_or(FaceEncoding {
                    vector: vec![0.0; 128],
                })
            })
            .collect())
    }

    async fn cluster(&self, encodings: &[FaceEncoding]) -> Result<FaceClusteringResult> {
        let clusters = if encodings.is_empty() {
            vec![]
        } else {
            vec![FaceCluster {
                id: "cluster-0".to_string(),
                face_ids: (0..encodings.len()).map(|i| format!("face-{i}")).collect(),
                centroid: encodings[0].vector.clone(),
            }]
        };
        Ok(FaceClusteringResult {
            num_clusters: clusters.len(),
            clusters,
        })
    }
}
