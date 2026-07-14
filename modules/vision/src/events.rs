use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VisionEventPayload {
    ImageLoaded {
        path: String,
        width: u32,
        height: u32,
    },
    ImageDecoded {
        format: String,
        size_bytes: u64,
    },
    ImageAnalyzed {
        path: String,
        duration_ms: u64,
    },
    OcrCompleted {
        text_len: usize,
        confidence: f64,
        duration_ms: u64,
    },
    CaptionGenerated {
        caption_len: usize,
        confidence: f64,
        duration_ms: u64,
    },
    EmbeddingCreated {
        dim: usize,
        duration_ms: u64,
    },
    ObjectsDetected {
        count: usize,
        duration_ms: u64,
    },
    FacesDetected {
        count: usize,
        duration_ms: u64,
    },
    SceneClassified {
        label: String,
        confidence: f64,
    },
    QualityAnalyzed {
        blur_score: f64,
        is_blurry: bool,
    },
    ColorsExtracted {
        dominant_count: usize,
    },
    TagsGenerated {
        count: usize,
    },
    VisualSearchCompleted {
        query: String,
        results: usize,
        duration_ms: u64,
    },
    CacheHit {
        key: String,
    },
    CacheMiss {
        key: String,
    },
    ThumbnailGenerated {
        path: String,
        size_bytes: u64,
    },
    ImageHashComputed {
        hash: u64,
        algorithm: String,
    },
    FaceEncodingCreated {
        face_id: String,
    },
    BatchAnalysisCompleted {
        count: usize,
        duration_ms: u64,
    },
    VisionToolInvoked {
        tool: String,
        duration_ms: u64,
        success: bool,
    },
    DuplicateFound {
        path: String,
        duplicate_of: String,
        similarity: f64,
    },
    ScreenshotAnalyzed {
        element_count: usize,
        button_count: usize,
        dialog_count: usize,
        duration_ms: u64,
    },
    VisionContextBuilt {
        has_ocr: bool,
        has_caption: bool,
        has_screenshot: bool,
    },
    PreprocessorTransform {
        original_w: u32,
        original_h: u32,
        result_w: u32,
        result_h: u32,
        mode: String,
    },
    AnalysisStarted {
        path: String,
    },
    AnalysisFailed {
        path: String,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionEvent {
    pub id: Uuid,
    pub correlation_id: Uuid,
    pub timestamp: DateTime<Local>,
    pub payload: VisionEventPayload,
}

impl VisionEvent {
    pub fn new(correlation_id: Uuid, payload: VisionEventPayload) -> Self {
        Self {
            id: Uuid::new_v4(),
            correlation_id,
            timestamp: Local::now(),
            payload,
        }
    }

    pub fn action_name(&self) -> &'static str {
        match self.payload {
            VisionEventPayload::ImageLoaded { .. } => "vision.image_loaded",
            VisionEventPayload::ImageDecoded { .. } => "vision.image_decoded",
            VisionEventPayload::ImageAnalyzed { .. } => "vision.image_analyzed",
            VisionEventPayload::OcrCompleted { .. } => "vision.ocr_completed",
            VisionEventPayload::CaptionGenerated { .. } => "vision.caption_generated",
            VisionEventPayload::EmbeddingCreated { .. } => "vision.embedding_created",
            VisionEventPayload::ObjectsDetected { .. } => "vision.objects_detected",
            VisionEventPayload::FacesDetected { .. } => "vision.faces_detected",
            VisionEventPayload::SceneClassified { .. } => "vision.scene_classified",
            VisionEventPayload::QualityAnalyzed { .. } => "vision.quality_analyzed",
            VisionEventPayload::ColorsExtracted { .. } => "vision.colors_extracted",
            VisionEventPayload::TagsGenerated { .. } => "vision.tags_generated",
            VisionEventPayload::VisualSearchCompleted { .. } => "vision.visual_search_completed",
            VisionEventPayload::CacheHit { .. } => "vision.cache_hit",
            VisionEventPayload::CacheMiss { .. } => "vision.cache_miss",
            VisionEventPayload::ThumbnailGenerated { .. } => "vision.thumbnail_generated",
            VisionEventPayload::ImageHashComputed { .. } => "vision.image_hash_computed",
            VisionEventPayload::FaceEncodingCreated { .. } => "vision.face_encoding_created",
            VisionEventPayload::BatchAnalysisCompleted { .. } => "vision.batch_analysis_completed",
            VisionEventPayload::VisionToolInvoked { .. } => "vision.tool_invoked",
            VisionEventPayload::DuplicateFound { .. } => "vision.duplicate_found",
            VisionEventPayload::ScreenshotAnalyzed { .. } => "vision.screenshot_analyzed",
            VisionEventPayload::VisionContextBuilt { .. } => "vision.context_built",
            VisionEventPayload::PreprocessorTransform { .. } => "vision.preprocessor_transform",
            VisionEventPayload::AnalysisStarted { .. } => "vision.analysis_started",
            VisionEventPayload::AnalysisFailed { .. } => "vision.analysis_failed",
        }
    }

    pub fn description(&self) -> String {
        match &self.payload {
            VisionEventPayload::ImageLoaded {
                path,
                width,
                height,
            } => format!("Image loaded: {path} ({width}x{height})"),
            VisionEventPayload::ImageDecoded { format, size_bytes } => {
                format!("Image decoded as {format} ({size_bytes} bytes)")
            }
            VisionEventPayload::ImageAnalyzed { path, duration_ms } => {
                format!("Image analyzed: {path} in {duration_ms}ms")
            }
            VisionEventPayload::OcrCompleted {
                text_len,
                confidence,
                duration_ms,
            } => format!("OCR: {text_len} chars at {confidence:.2} confidence in {duration_ms}ms"),
            VisionEventPayload::CaptionGenerated {
                caption_len,
                confidence,
                duration_ms,
            } => format!("Caption: {caption_len} chars at {confidence:.2} in {duration_ms}ms"),
            VisionEventPayload::EmbeddingCreated { dim, duration_ms } => {
                format!("Embedding ({dim}d) created in {duration_ms}ms")
            }
            VisionEventPayload::ObjectsDetected { count, duration_ms } => {
                format!("{count} object(s) detected in {duration_ms}ms")
            }
            VisionEventPayload::FacesDetected { count, duration_ms } => {
                format!("{count} face(s) detected in {duration_ms}ms")
            }
            VisionEventPayload::SceneClassified { label, confidence } => {
                format!("Scene: {label} ({confidence:.2})")
            }
            VisionEventPayload::QualityAnalyzed {
                blur_score,
                is_blurry,
            } => format!("Quality: blur={blur_score:.2}, blurry={is_blurry}"),
            VisionEventPayload::ColorsExtracted { dominant_count } => {
                format!("{dominant_count} dominant colors extracted")
            }
            VisionEventPayload::TagsGenerated { count } => format!("{count} tags generated"),
            VisionEventPayload::VisualSearchCompleted {
                query,
                results,
                duration_ms,
            } => format!("Search '{query}': {results} results in {duration_ms}ms"),
            VisionEventPayload::CacheHit { key } => format!("Cache hit: {key}"),
            VisionEventPayload::CacheMiss { key } => format!("Cache miss: {key}"),
            VisionEventPayload::ThumbnailGenerated { path, size_bytes } => {
                format!("Thumbnail: {path} ({size_bytes} bytes)")
            }
            VisionEventPayload::ImageHashComputed { hash, algorithm } => {
                format!("Hash ({algorithm}): {hash:#018x}")
            }
            VisionEventPayload::FaceEncodingCreated { face_id } => {
                format!("Face encoding: {face_id}")
            }
            VisionEventPayload::BatchAnalysisCompleted { count, duration_ms } => {
                format!("Batch: {count} images in {duration_ms}ms")
            }
            VisionEventPayload::VisionToolInvoked {
                tool,
                duration_ms,
                success,
            } => format!("Tool '{tool}': {duration_ms}ms, success={success}"),
            VisionEventPayload::DuplicateFound {
                path,
                duplicate_of,
                similarity,
            } => format!("Duplicate: {path} matches {duplicate_of} ({similarity:.2})"),
            VisionEventPayload::ScreenshotAnalyzed {
                element_count,
                button_count,
                dialog_count,
                duration_ms,
            } => format!(
                "Screenshot: {element_count} elements ({button_count} buttons, {dialog_count} dialogs) in {duration_ms}ms"
            ),
            VisionEventPayload::VisionContextBuilt {
                has_ocr,
                has_caption,
                has_screenshot,
            } => format!(
                "Context built: ocr={has_ocr}, caption={has_caption}, screenshot={has_screenshot}"
            ),
            VisionEventPayload::PreprocessorTransform {
                original_w,
                original_h,
                result_w,
                result_h,
                mode,
            } => format!("Preprocessor: {original_w}x{original_h} -> {result_w}x{result_h} ({mode})"),
            VisionEventPayload::AnalysisStarted { path } => format!("Analysis started: {path}"),
            VisionEventPayload::AnalysisFailed { path, error } => {
                format!("Analysis failed: {path} - {error}")
            }
        }
    }
}
