use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEmbedding {
    pub vector: Vec<f32>,
    pub dim: usize,
    pub version: String,
}

impl ImageEmbedding {
    pub fn cosine_similarity(&self, other: &ImageEmbedding) -> f64 {
        if self.dim != other.dim || self.vector.is_empty() {
            return 0.0;
        }
        let dot: f64 = self
            .vector
            .iter()
            .zip(&other.vector)
            .map(|(a, b)| *a as f64 * *b as f64)
            .sum();
        let norm_a: f64 = self
            .vector
            .iter()
            .map(|v| *v as f64 * *v as f64)
            .sum::<f64>()
            .sqrt();
        let norm_b: f64 = other
            .vector
            .iter()
            .map(|v| *v as f64 * *v as f64)
            .sum::<f64>()
            .sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
    }
}

#[async_trait]
pub trait VisionEmbedder: Send + Sync {
    async fn embed(&self, bytes: &[u8]) -> Result<ImageEmbedding>;
    async fn embed_batch(&self, batch: &[&[u8]]) -> Result<Vec<ImageEmbedding>>;
    fn embedding_dim(&self) -> usize;
}

pub struct MockVisionEmbedder;

impl MockVisionEmbedder {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockVisionEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VisionEmbedder for MockVisionEmbedder {
    async fn embed(&self, _bytes: &[u8]) -> Result<ImageEmbedding> {
        Ok(ImageEmbedding {
            vector: vec![0.1f32; 384],
            dim: 384,
            version: "mock-v1".to_string(),
        })
    }

    async fn embed_batch(&self, batch: &[&[u8]]) -> Result<Vec<ImageEmbedding>> {
        Ok(batch
            .iter()
            .map(|_| ImageEmbedding {
                vector: vec![0.1f32; 384],
                dim: 384,
                version: "mock-v1".to_string(),
            })
            .collect())
    }

    fn embedding_dim(&self) -> usize {
        384
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = ImageEmbedding {
            vector: vec![1.0, 0.0, 0.0],
            dim: 3,
            version: "v1".to_string(),
        };
        let sim = a.cosine_similarity(&a);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = ImageEmbedding {
            vector: vec![1.0, 0.0, 0.0],
            dim: 3,
            version: "v1".to_string(),
        };
        let b = ImageEmbedding {
            vector: vec![0.0, 1.0, 0.0],
            dim: 3,
            version: "v1".to_string(),
        };
        let sim = a.cosine_similarity(&b);
        assert!((sim - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = ImageEmbedding {
            vector: vec![1.0, 0.0],
            dim: 2,
            version: "v1".to_string(),
        };
        let b = ImageEmbedding {
            vector: vec![-1.0, 0.0],
            dim: 2,
            version: "v1".to_string(),
        };
        let sim = a.cosine_similarity(&b);
        assert!((sim - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a = ImageEmbedding {
            vector: vec![],
            dim: 0,
            version: "v1".to_string(),
        };
        let b = ImageEmbedding {
            vector: vec![],
            dim: 0,
            version: "v1".to_string(),
        };
        assert_eq!(a.cosine_similarity(&b), 0.0);
    }
}
