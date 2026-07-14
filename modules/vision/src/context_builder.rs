use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::engine::{AnalysisResult, VisionEngine};
use crate::metadata::ImageMetadata;
use crate::screenshot::{ScreenshotAnalysis, ScreenshotAnalyzer};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionContext {
    pub has_visual_data: bool,
    pub ocr_text: Option<String>,
    pub image_metadata: Option<ImageMetadata>,
    pub scene_description: Option<String>,
    pub screenshot_analysis: Option<ScreenshotAnalysis>,
    pub objects_detected: Vec<String>,
    pub tags: Vec<String>,
    pub dominant_colors: Vec<String>,
    pub embedding_available: bool,
    pub caption: Option<String>,
    pub image_quality: Option<String>,
    pub face_count: usize,
}

impl VisionContext {
    pub fn new() -> Self {
        Self {
            has_visual_data: false,
            ocr_text: None,
            image_metadata: None,
            scene_description: None,
            screenshot_analysis: None,
            objects_detected: vec![],
            tags: vec![],
            dominant_colors: vec![],
            embedding_available: false,
            caption: None,
            image_quality: None,
            face_count: 0,
        }
    }

    pub fn to_prompt_context(&self) -> String {
        let mut parts: Vec<String> =
            vec!["You have been provided with visual context from an image:".to_string()];

        if let Some(ref caption) = self.caption {
            parts.push(format!("- Caption: {}", caption));
        }

        if let Some(ref ocr) = self.ocr_text {
            parts.push(format!("- OCR text found in image: {}", ocr));
        }

        if let Some(ref meta) = self.image_metadata {
            parts.push(format!(
                "- Image dimensions: {}x{} (format: {})",
                meta.width, meta.height, meta.format
            ));
        }

        if let Some(ref scene) = self.scene_description {
            parts.push(format!("- Scene: {}", scene));
        }

        if !self.objects_detected.is_empty() {
            parts.push(format!(
                "- Objects detected: {}",
                self.objects_detected.join(", ")
            ));
        }

        if !self.tags.is_empty() {
            parts.push(format!("- Tags: {}", self.tags.join(", ")));
        }

        if !self.dominant_colors.is_empty() {
            parts.push(format!(
                "- Dominant colors: {}",
                self.dominant_colors.join(", ")
            ));
        }

        if let Some(ref quality) = self.image_quality {
            parts.push(format!("- Quality: {}", quality));
        }

        if self.face_count > 0 {
            parts.push(format!("- Faces detected: {}", self.face_count));
        }

        if let Some(ref ss) = self.screenshot_analysis {
            parts.push(format!("- Screenshot analysis: {}", ss.summary()));
            if ss.has_errors() {
                parts.push("- WARNING: Screenshot contains error dialogs".to_string());
            }
            if ss.has_permission_request() {
                parts.push("- WARNING: Screenshot contains permission requests".to_string());
            }
        }

        parts.join("\n")
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

impl Default for VisionContext {
    fn default() -> Self {
        Self::new()
    }
}

pub struct VisionContextBuilder {
    engine: Option<Arc<VisionEngine>>,
}

impl VisionContextBuilder {
    pub fn new(engine: Option<Arc<VisionEngine>>) -> Self {
        Self { engine }
    }

    pub async fn from_analysis(
        &self,
        analysis: &AnalysisResult,
        metadata: Option<ImageMetadata>,
    ) -> VisionContext {
        let mut ctx = VisionContext::new();
        ctx.has_visual_data = true;

        if let Some(ref ocr) = analysis.ocr {
            ctx.ocr_text = Some(ocr.text.clone());
        }

        if let Some(ref caption) = analysis.caption {
            ctx.caption = Some(caption.caption.clone());
        }

        if let Some(ref scene) = analysis.scene {
            let descs: Vec<String> = scene
                .scenes
                .iter()
                .map(|s| format!("{} ({:.1}%)", s.label.as_str(), s.confidence * 100.0))
                .collect();
            ctx.scene_description = Some(descs.join(", "));
        }

        if let Some(ref objects) = analysis.objects {
            ctx.objects_detected = objects.objects.iter().map(|o| o.label.clone()).collect();
        }

        if let Some(ref tags_result) = analysis.tags {
            ctx.tags = tags_result.tags.iter().map(|t| t.tag.clone()).collect();
        }

        if let Some(ref colors) = analysis.colors {
            ctx.dominant_colors = colors
                .dominant_colors
                .iter()
                .map(|c| c.name.clone())
                .collect();
        }

        if let Some(ref quality) = analysis.quality {
            let blur = if quality.is_blurry { "blurry" } else { "sharp" };
            ctx.image_quality = Some(format!(
                "{} (aesthetics: {:.2}, noise: {:.2})",
                blur, quality.aesthetics, quality.noise
            ));
        }

        ctx.embedding_available = analysis.embedding.is_some();

        if let Some(ref faces) = analysis.faces {
            ctx.face_count = faces.faces.len();
        }

        ctx.image_metadata = metadata;

        ctx
    }

    pub async fn from_screenshot(
        &self,
        screenshot: ScreenshotAnalysis,
        analysis: &AnalysisResult,
        metadata: Option<ImageMetadata>,
    ) -> VisionContext {
        let mut ctx = self.from_analysis(analysis, metadata).await;
        ctx.screenshot_analysis = Some(screenshot);
        ctx
    }

    pub async fn build_context(
        &self,
        image_bytes: &[u8],
        screenshot_bytes: Option<&[u8]>,
    ) -> VisionContext {
        let mut ctx = VisionContext::new();

        if let Some(ref engine) = self.engine {
            if !image_bytes.is_empty() {
                ctx.has_visual_data = true;
                let analysis = engine.analyze(image_bytes).await.ok();
                let meta = engine.metadata.read_metadata(image_bytes).await.ok();

                if let (Some(ref a), m) = (analysis, meta) {
                    ctx = self.from_analysis(a, m).await;
                }
            }

            if let Some(ss_bytes) = screenshot_bytes {
                if !ss_bytes.is_empty() {
                    if let Ok(screenshot) = crate::screenshot::MockScreenshotAnalyzer::new()
                        .analyze_screenshot(ss_bytes)
                        .await
                    {
                        ctx.screenshot_analysis = Some(screenshot);
                    }
                }
            }
        }

        ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caption::CaptionResult;
    use crate::color::ColorResult;
    use crate::detection::{DetectionObject, DetectionResult, ObjectBoundingBox};
    use crate::embedding::ImageEmbedding;
    use crate::ocr::OcrResult;
    use crate::quality::QualityResult;
    use crate::scene::{SceneClassification, SceneLabel, SceneResult};
    use crate::tags::TagsResult;

    fn make_sample_analysis() -> AnalysisResult {
        AnalysisResult {
            ocr: Some(OcrResult {
                text: "Sample OCR text".to_string(),
                confidence: 0.95,
                blocks: vec![],
                language: "eng".to_string(),
                duration_ms: 10,
            }),
            caption: Some(CaptionResult {
                caption: "A sample image".to_string(),
                confidence: 0.85,
                duration_ms: 20,
            }),
            embedding: Some(ImageEmbedding {
                vector: vec![0.1; 384],
                dim: 384,
                version: "test".to_string(),
            }),
            objects: Some(DetectionResult {
                objects: vec![DetectionObject {
                    label: "person".to_string(),
                    confidence: 0.95,
                    bounding_box: ObjectBoundingBox {
                        x: 0.0,
                        y: 0.0,
                        w: 10.0,
                        h: 10.0,
                    },
                }],
            }),
            scene: Some(SceneResult {
                scenes: vec![SceneClassification {
                    label: SceneLabel::Indoors,
                    confidence: 0.9,
                }],
            }),
            quality: Some(QualityResult {
                blur_score: 0.1,
                brightness: 0.5,
                contrast: 0.6,
                noise: 0.05,
                aesthetics: 0.8,
                is_blurry: false,
                overall: crate::quality::OverallQuality::Good,
                blur_level: crate::quality::BlurLevel::None,
            }),
            colors: Some(ColorResult {
                dominant_colors: vec![crate::color::DominantColor {
                    color: crate::color::RgbColor {
                        r: 100,
                        g: 150,
                        b: 200,
                    },
                    percentage: 0.5,
                    name: "blue".to_string(),
                }],
                palette: vec![],
                average_color: crate::color::RgbColor {
                    r: 100,
                    g: 150,
                    b: 200,
                },
                colorfulness: 0.6,
            }),
            tags: Some(TagsResult {
                tags: vec![crate::tags::VisualTag {
                    tag: "indoor".to_string(),
                    confidence: 0.9,
                    category: crate::tags::TagCategory::Scene,
                }],
            }),
            faces: Some(crate::face::FaceDetectionResult {
                faces: vec![crate::face::DetectedFace {
                    bounding_box: ObjectBoundingBox {
                        x: 0.0,
                        y: 0.0,
                        w: 10.0,
                        h: 10.0,
                    },
                    landmarks: None,
                    confidence: 0.98,
                    encoding: None,
                }],
            }),
            hash: None,
            duration_ms: 100,
            analyzed_at: chrono::Local::now(),
        }
    }

    #[tokio::test]
    async fn test_context_builder_from_analysis() {
        let builder = VisionContextBuilder::new(None);
        let analysis = make_sample_analysis();
        let ctx = builder.from_analysis(&analysis, None).await;

        assert!(ctx.has_visual_data);
        assert_eq!(ctx.ocr_text, Some("Sample OCR text".to_string()));
        assert_eq!(ctx.caption, Some("A sample image".to_string()));
        assert!(!ctx.objects_detected.is_empty());
        assert_eq!(ctx.objects_detected[0], "person");
        assert!(ctx.embedding_available);
        assert_eq!(ctx.face_count, 1);
    }

    #[tokio::test]
    async fn test_context_to_prompt() {
        let builder = VisionContextBuilder::new(None);
        let analysis = make_sample_analysis();
        let ctx = builder.from_analysis(&analysis, None).await;
        let prompt = ctx.to_prompt_context();

        assert!(prompt.contains("Caption"));
        assert!(prompt.contains("OCR text"));
        assert!(prompt.contains("person"));
    }

    #[test]
    fn test_empty_context() {
        let ctx = VisionContext::new();
        assert!(!ctx.has_visual_data);
        assert!(ctx.ocr_text.is_none());
    }
}
