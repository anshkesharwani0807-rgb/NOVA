pub mod mock;

use async_trait::async_trait;
use nova_kernel::Result;

use crate::caption::{CaptionOptions, CaptionResult};
use crate::color::ColorResult;
use crate::decoder::DecodedImage;
use crate::detection::DetectionResult;
use crate::embedding::ImageEmbedding;
use crate::face::{FaceClusteringResult, FaceDetectionResult};
use crate::hashing::ImageHash;
use crate::image_loader::LoadedImage;
use crate::metadata::ImageMetadata;
use crate::ocr::{OcrOptions, OcrResult};
use crate::quality::QualityResult;
use crate::scene::SceneResult;
use crate::tags::TagsResult;
use crate::thumbnail::{ThumbnailMode, ThumbnailResult};

#[async_trait]
pub trait VisionProvider: Send + Sync {
    async fn load_image(&self, path: &str) -> Result<LoadedImage>;
    async fn load_image_from_bytes(&self, bytes: &[u8]) -> Result<LoadedImage>;
    async fn decode_rgba(&self, bytes: &[u8]) -> Result<DecodedImage>;
    async fn decode_grayscale(&self, bytes: &[u8]) -> Result<DecodedImage>;
    async fn read_metadata(&self, bytes: &[u8]) -> Result<ImageMetadata>;
    async fn thumbnail(
        &self,
        bytes: &[u8],
        max_w: u32,
        max_h: u32,
        mode: ThumbnailMode,
    ) -> Result<ThumbnailResult>;
    async fn hash_image(&self, bytes: &[u8]) -> Result<ImageHash>;
    async fn ocr(&self, bytes: &[u8], options: OcrOptions) -> Result<OcrResult>;
    async fn detect_objects(&self, bytes: &[u8]) -> Result<DetectionResult>;
    async fn classify_scene(&self, bytes: &[u8]) -> Result<SceneResult>;
    async fn caption(&self, bytes: &[u8], options: CaptionOptions) -> Result<CaptionResult>;
    async fn embed(&self, bytes: &[u8]) -> Result<ImageEmbedding>;
    async fn detect_faces(&self, bytes: &[u8]) -> Result<FaceDetectionResult>;
    async fn cluster_faces(&self, encodings: Vec<Vec<f64>>) -> Result<FaceClusteringResult>;
    async fn analyze_quality(&self, bytes: &[u8]) -> Result<QualityResult>;
    async fn analyze_colors(&self, bytes: &[u8]) -> Result<ColorResult>;
    async fn generate_tags(&self, bytes: &[u8]) -> Result<TagsResult>;
}
