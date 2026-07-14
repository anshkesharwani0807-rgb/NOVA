use async_trait::async_trait;
use nova_kernel::{NovaError, Result};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::caption::{CaptionOptions, CaptionResult};
use crate::color::{ColorResult, DominantColor, RgbColor};
use crate::decoder::DecodedImage;
use crate::detection::{DetectionObject, DetectionResult, ObjectBoundingBox};
use crate::embedding::ImageEmbedding;
use crate::face::{
    DetectedFace, FaceClusteringResult, FaceDetectionResult, FaceEncoding, FaceLandmarks,
};
use crate::hashing::{AverageHasher, ImageHash, ImageHasher};
use crate::image_loader::LoadedImage;
use crate::metadata::ImageMetadata;
use crate::ocr::{OcrBlock, OcrOptions, OcrResult};
use crate::providers::VisionProvider;
use crate::quality::{BlurLevel, QualityResult};
use crate::scene::{SceneClassification, SceneLabel, SceneResult};
use crate::tags::{TagCategory, TagsResult, VisualTag};
use crate::thumbnail::{ThumbnailMode, ThumbnailResult};

pub struct MockVisionProvider {
    call_count: AtomicU64,
    fail_ocr: bool,
    fail_caption: bool,
    fail_embed: bool,
    fail_detect: bool,
    fail_face: bool,
    fail_scene: bool,
    fail_quality: bool,
    fail_color: bool,
    fail_tags: bool,
    fail_load: bool,
}

impl MockVisionProvider {
    pub fn new() -> Self {
        Self {
            call_count: AtomicU64::new(0),
            fail_ocr: false,
            fail_caption: false,
            fail_embed: false,
            fail_detect: false,
            fail_face: false,
            fail_scene: false,
            fail_quality: false,
            fail_color: false,
            fail_tags: false,
            fail_load: false,
        }
    }
}

impl Default for MockVisionProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockVisionProvider {
    pub fn with_fail_ocr(mut self) -> Self {
        self.fail_ocr = true;
        self
    }

    pub fn with_fail_caption(mut self) -> Self {
        self.fail_caption = true;
        self
    }

    pub fn with_fail_embed(mut self) -> Self {
        self.fail_embed = true;
        self
    }

    pub fn with_fail_load(mut self) -> Self {
        self.fail_load = true;
        self
    }

    pub fn call_count(&self) -> u64 {
        self.call_count.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl VisionProvider for MockVisionProvider {
    async fn load_image(&self, path: &str) -> Result<LoadedImage> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_load {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_MOCK_LOAD_FAIL",
                &format!("Mock load failed for {path}"),
            ));
        }
        Ok(LoadedImage {
            data: vec![0u8; 64 * 64 * 4],
            width: 64,
            height: 64,
            format: "rgba".to_string(),
            mime_type: "image/png".to_string(),
        })
    }

    async fn load_image_from_bytes(&self, _bytes: &[u8]) -> Result<LoadedImage> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_load {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_MOCK_LOAD_FAIL",
                "Mock load from bytes failed",
            ));
        }
        Ok(LoadedImage {
            data: vec![0u8; 32 * 32 * 4],
            width: 32,
            height: 32,
            format: "rgba".to_string(),
            mime_type: "image/png".to_string(),
        })
    }

    async fn decode_rgba(&self, _bytes: &[u8]) -> Result<DecodedImage> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(DecodedImage {
            data: vec![0u8; 64 * 64 * 4],
            width: 64,
            height: 64,
            channels: 4,
            color_space: "sRGB".to_string(),
        })
    }

    async fn decode_grayscale(&self, _bytes: &[u8]) -> Result<DecodedImage> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(DecodedImage {
            data: vec![0u8; 64 * 64],
            width: 64,
            height: 64,
            channels: 1,
            color_space: "Gray".to_string(),
        })
    }

    async fn read_metadata(&self, _bytes: &[u8]) -> Result<ImageMetadata> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(ImageMetadata {
            width: 64,
            height: 64,
            format: "png".to_string(),
            color_space: "sRGB".to_string(),
            file_size: 4096,
            orientation: 1,
            has_exif: false,
            exif_fields: vec![],
            has_gps: false,
            gps_latitude: None,
            gps_longitude: None,
            bits_per_pixel: 24,
            is_animated: false,
        })
    }

    async fn thumbnail(
        &self,
        _bytes: &[u8],
        max_w: u32,
        max_h: u32,
        _mode: ThumbnailMode,
    ) -> Result<ThumbnailResult> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(ThumbnailResult {
            data: vec![0u8; (max_w * max_h * 4) as usize],
            width: max_w,
            height: max_h,
            size_bytes: (max_w * max_h * 4) as u64,
            format: "jpeg".to_string(),
        })
    }

    async fn hash_image(&self, bytes: &[u8]) -> Result<ImageHash> {
        let hasher = AverageHasher::new();
        hasher.hash_image(bytes).await
    }

    async fn ocr(&self, _bytes: &[u8], _options: OcrOptions) -> Result<OcrResult> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_ocr {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_MOCK_OCR_FAIL",
                "Mock OCR failed",
            ));
        }
        Ok(OcrResult {
            text: "Mock OCR text".to_string(),
            confidence: 0.95,
            blocks: vec![OcrBlock {
                text: "Mock OCR text".to_string(),
                confidence: 0.95,
                bounding_box: crate::ocr::BoundingBox {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 20.0,
                },
            }],
            language: "eng".to_string(),
            duration_ms: 10,
        })
    }

    async fn detect_objects(&self, _bytes: &[u8]) -> Result<DetectionResult> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_detect {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_MOCK_DETECT_FAIL",
                "Mock detection failed",
            ));
        }
        Ok(DetectionResult {
            objects: vec![DetectionObject {
                label: "person".to_string(),
                confidence: 0.95,
                bounding_box: ObjectBoundingBox {
                    x: 10.0,
                    y: 10.0,
                    w: 50.0,
                    h: 100.0,
                },
            }],
        })
    }

    async fn classify_scene(&self, _bytes: &[u8]) -> Result<SceneResult> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_scene {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_MOCK_SCENE_FAIL",
                "Mock scene classification failed",
            ));
        }
        Ok(SceneResult {
            scenes: vec![SceneClassification {
                label: SceneLabel::Outdoors,
                confidence: 0.88,
            }],
        })
    }

    async fn caption(&self, _bytes: &[u8], _options: CaptionOptions) -> Result<CaptionResult> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_caption {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_MOCK_CAPTION_FAIL",
                "Mock caption failed",
            ));
        }
        Ok(CaptionResult {
            caption: "A mock caption describing the image".to_string(),
            confidence: 0.85,
            duration_ms: 20,
        })
    }

    async fn embed(&self, _bytes: &[u8]) -> Result<ImageEmbedding> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_embed {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_MOCK_EMBED_FAIL",
                "Mock embedding failed",
            ));
        }
        Ok(ImageEmbedding {
            vector: vec![0.1f32; 384],
            dim: 384,
            version: "mock-v1".to_string(),
        })
    }

    async fn detect_faces(&self, _bytes: &[u8]) -> Result<FaceDetectionResult> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_face {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_MOCK_FACE_FAIL",
                "Mock face detection failed",
            ));
        }
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

    async fn cluster_faces(&self, _encodings: Vec<Vec<f64>>) -> Result<FaceClusteringResult> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(FaceClusteringResult {
            clusters: vec![],
            num_clusters: 0,
        })
    }

    async fn analyze_quality(&self, _bytes: &[u8]) -> Result<QualityResult> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_quality {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_MOCK_QUALITY_FAIL",
                "Mock quality analysis failed",
            ));
        }
        Ok(QualityResult {
            blur_score: 0.1,
            brightness: 0.5,
            contrast: 0.6,
            noise: 0.05,
            aesthetics: 0.7,
            is_blurry: false,
            overall: crate::quality::OverallQuality::Good,
            blur_level: BlurLevel::None,
        })
    }

    async fn analyze_colors(&self, _bytes: &[u8]) -> Result<ColorResult> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_color {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_MOCK_COLOR_FAIL",
                "Mock color analysis failed",
            ));
        }
        Ok(ColorResult {
            dominant_colors: vec![DominantColor {
                color: RgbColor {
                    r: 100,
                    g: 150,
                    b: 200,
                },
                percentage: 0.4,
                name: "blue".to_string(),
            }],
            palette: vec![],
            average_color: RgbColor {
                r: 100,
                g: 150,
                b: 200,
            },
            colorfulness: 0.6,
        })
    }

    async fn generate_tags(&self, _bytes: &[u8]) -> Result<TagsResult> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_tags {
            return Err(NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_MOCK_TAGS_FAIL",
                "Mock tag generation failed",
            ));
        }
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
                    tag: "blue".to_string(),
                    confidence: 0.6,
                    category: TagCategory::Color,
                },
            ],
        })
    }
}
