use chrono::{DateTime, Local};
use nova_kernel::Result;
use std::sync::Arc;

use crate::caption::{CaptionEngine, CaptionOptions, CaptionResult};
use crate::color::{ColorAnalyzer, ColorResult};
use crate::decoder::ImageDecoder;
use crate::detection::{DetectionResult, ObjectDetector};
use crate::embedding::{ImageEmbedding, VisionEmbedder};
use crate::face::{FaceDetectionResult, FaceEngine};
use crate::hashing::{ImageHash, ImageHasher};
use crate::image_loader::ImageLoader;
use crate::metadata::MetadataReader;
use crate::ocr::{OcrEngine, OcrOptions, OcrResult};
use crate::providers::VisionProvider;
use crate::quality::QualityAnalyzer;
use crate::scene::{SceneClassifier, SceneResult};
use crate::tags::{TagsResult, VisualTagger};
use crate::thumbnail::ThumbnailGenerator;

pub struct VisionEngine {
    pub loader: Arc<dyn ImageLoader>,
    pub decoder: Arc<dyn ImageDecoder>,
    pub metadata: Arc<dyn MetadataReader>,
    pub thumbnail: Arc<dyn ThumbnailGenerator>,
    pub hasher: Arc<dyn ImageHasher>,
    pub ocr: Arc<dyn OcrEngine>,
    pub caption: Arc<dyn CaptionEngine>,
    pub embedder: Arc<dyn VisionEmbedder>,
    pub detector: Arc<dyn ObjectDetector>,
    pub classifier: Arc<dyn SceneClassifier>,
    pub face: Arc<dyn FaceEngine>,
    pub quality: Arc<dyn QualityAnalyzer>,
    pub color: Arc<dyn ColorAnalyzer>,
    pub tagger: Arc<dyn VisualTagger>,
    pub provider: Arc<dyn VisionProvider>,
}

impl VisionEngine {
    pub fn new(provider: Arc<dyn VisionProvider>) -> Self {
        let loader =
            Arc::new(crate::image_loader::NativeImageLoader::new()) as Arc<dyn ImageLoader>;
        let decoder = Arc::new(crate::decoder::NativeImageDecoder::new()) as Arc<dyn ImageDecoder>;
        let metadata =
            Arc::new(crate::metadata::NativeMetadataReader::new()) as Arc<dyn MetadataReader>;
        let thumbnail = Arc::new(crate::thumbnail::NativeThumbnailGenerator::new(85))
            as Arc<dyn ThumbnailGenerator>;
        let hasher = Arc::new(crate::hashing::AverageHasher::new()) as Arc<dyn ImageHasher>;
        let ocr = Arc::new(crate::ocr::MockOcrEngine::new()) as Arc<dyn OcrEngine>;
        let caption = Arc::new(crate::caption::MockCaptionEngine::new()) as Arc<dyn CaptionEngine>;
        let embedder =
            Arc::new(crate::embedding::MockVisionEmbedder::new()) as Arc<dyn VisionEmbedder>;
        let detector =
            Arc::new(crate::detection::MockObjectDetector::new()) as Arc<dyn ObjectDetector>;
        let classifier =
            Arc::new(crate::scene::MockSceneClassifier::new()) as Arc<dyn SceneClassifier>;
        let face = Arc::new(crate::face::MockFaceEngine::new()) as Arc<dyn FaceEngine>;
        let quality =
            Arc::new(crate::quality::MockQualityAnalyzer::new()) as Arc<dyn QualityAnalyzer>;
        let color = Arc::new(crate::color::MockColorAnalyzer::new()) as Arc<dyn ColorAnalyzer>;
        let tagger = Arc::new(crate::tags::MockVisualTagger::new()) as Arc<dyn VisualTagger>;

        Self {
            loader,
            decoder,
            metadata,
            thumbnail,
            hasher,
            ocr,
            caption,
            embedder,
            detector,
            classifier,
            face,
            quality,
            color,
            tagger,
            provider,
        }
    }

    pub async fn load_image(&self, path: &str) -> Result<crate::image_loader::LoadedImage> {
        self.loader.load_from_path(path).await
    }

    pub async fn ocr_image(&self, bytes: &[u8], options: Option<OcrOptions>) -> Result<OcrResult> {
        let opts = options.unwrap_or_default();
        self.ocr.recognize(bytes, opts).await
    }

    pub async fn caption_image(
        &self,
        bytes: &[u8],
        options: Option<CaptionOptions>,
    ) -> Result<CaptionResult> {
        let opts = options.unwrap_or_default();
        self.caption.generate(bytes, opts).await
    }

    pub async fn embed_image(&self, bytes: &[u8]) -> Result<ImageEmbedding> {
        self.embedder.embed(bytes).await
    }

    pub async fn detect_objects(&self, bytes: &[u8]) -> Result<DetectionResult> {
        self.detector.detect(bytes).await
    }

    pub async fn detect_faces(&self, bytes: &[u8]) -> Result<FaceDetectionResult> {
        self.face.detect(bytes).await
    }

    pub async fn classify_scene(&self, bytes: &[u8]) -> Result<SceneResult> {
        self.classifier.classify(bytes).await
    }

    pub async fn analyze_quality(&self, bytes: &[u8]) -> Result<crate::quality::QualityResult> {
        self.quality.analyze(bytes).await
    }

    pub async fn analyze_colors(&self, bytes: &[u8]) -> Result<ColorResult> {
        self.color.analyze(bytes).await
    }

    pub async fn generate_tags(&self, bytes: &[u8]) -> Result<TagsResult> {
        self.tagger.generate_tags(bytes).await
    }

    pub async fn hash_image(&self, bytes: &[u8]) -> Result<ImageHash> {
        self.hasher.hash_image(bytes).await
    }

    pub async fn analyze(&self, bytes: &[u8]) -> Result<AnalysisResult> {
        let start = std::time::Instant::now();
        let (ocr, caption, embed, det, scene, qual, col, tags, face_r, hash) = tokio::join!(
            self.ocr_image(bytes, None),
            self.caption_image(bytes, None),
            self.embed_image(bytes),
            self.detect_objects(bytes),
            self.classify_scene(bytes),
            self.analyze_quality(bytes),
            self.analyze_colors(bytes),
            self.generate_tags(bytes),
            self.detect_faces(bytes),
            self.hash_image(bytes),
        );
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(AnalysisResult {
            ocr: ocr.ok(),
            caption: caption.ok(),
            embedding: embed.ok(),
            objects: det.ok(),
            scene: scene.ok(),
            quality: qual.ok(),
            colors: col.ok(),
            tags: tags.ok(),
            faces: face_r.ok(),
            hash: hash.ok(),
            duration_ms,
            analyzed_at: Local::now(),
        })
    }

    pub async fn find_similar(
        &self,
        bytes: &[u8],
        candidates: &[&[u8]],
    ) -> Result<Vec<(usize, f64)>> {
        let query = self.embed_image(bytes).await?;
        let mut results = Vec::new();
        for (i, c) in candidates.iter().enumerate() {
            if let Ok(emb) = self.embed_image(c).await {
                let sim = query.cosine_similarity(&emb);
                results.push((i, sim));
            }
        }
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }

    pub async fn is_duplicate(&self, bytes_a: &[u8], bytes_b: &[u8]) -> Result<bool> {
        let h_a = self.hash_image(bytes_a).await?;
        let h_b = self.hash_image(bytes_b).await?;
        Ok(h_a.is_similar(&h_b))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnalysisResult {
    pub ocr: Option<OcrResult>,
    pub caption: Option<CaptionResult>,
    pub embedding: Option<ImageEmbedding>,
    pub objects: Option<DetectionResult>,
    pub scene: Option<SceneResult>,
    pub quality: Option<crate::quality::QualityResult>,
    pub colors: Option<ColorResult>,
    pub tags: Option<TagsResult>,
    pub faces: Option<FaceDetectionResult>,
    pub hash: Option<ImageHash>,
    pub duration_ms: u64,
    pub analyzed_at: DateTime<Local>,
}
