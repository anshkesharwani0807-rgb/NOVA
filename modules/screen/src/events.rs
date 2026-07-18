use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScreenEventPayload {
    CaptureStarted {
        width: u32,
        height: u32,
        fps: u32,
    },
    CaptureStopped {
        frames_captured: u64,
    },
    FrameCaptured {
        frame_id: String,
        width: u32,
        height: u32,
        size_bytes: u64,
    },
    OcrCompleted {
        text_len: usize,
        confidence: f32,
        language: String,
        duration_ms: u64,
    },
    UITreeExtracted {
        element_count: usize,
        depth: usize,
        duration_ms: u64,
    },
    GroundingCompleted {
        query: String,
        results: usize,
        confidence: f32,
        duration_ms: u64,
    },
    ElementFound {
        element_id: String,
        element_type: String,
        confidence: f32,
    },
    ScreenToolInvoked {
        tool: String,
        duration_ms: u64,
        success: bool,
    },
    AnalysisStarted {
        reason: String,
    },
    AnalysisFailed {
        reason: String,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenEvent {
    pub id: Uuid,
    pub correlation_id: Uuid,
    pub timestamp: DateTime<Local>,
    pub payload: ScreenEventPayload,
}

impl ScreenEvent {
    pub fn new(correlation_id: Uuid, payload: ScreenEventPayload) -> Self {
        Self {
            id: Uuid::new_v4(),
            correlation_id,
            timestamp: Local::now(),
            payload,
        }
    }

    pub fn action_name(&self) -> &'static str {
        match self.payload {
            ScreenEventPayload::CaptureStarted { .. } => "screen.capture_started",
            ScreenEventPayload::CaptureStopped { .. } => "screen.capture_stopped",
            ScreenEventPayload::FrameCaptured { .. } => "screen.frame_captured",
            ScreenEventPayload::OcrCompleted { .. } => "screen.ocr_completed",
            ScreenEventPayload::UITreeExtracted { .. } => "screen.ui_tree_extracted",
            ScreenEventPayload::GroundingCompleted { .. } => "screen.grounding_completed",
            ScreenEventPayload::ElementFound { .. } => "screen.element_found",
            ScreenEventPayload::ScreenToolInvoked { .. } => "screen.tool_invoked",
            ScreenEventPayload::AnalysisStarted { .. } => "screen.analysis_started",
            ScreenEventPayload::AnalysisFailed { .. } => "screen.analysis_failed",
        }
    }

    pub fn description(&self) -> String {
        match &self.payload {
            ScreenEventPayload::CaptureStarted { width, height, fps } => {
                format!("Screen capture started: {width}x{height} @ {fps}fps")
            }
            ScreenEventPayload::CaptureStopped { frames_captured } => {
                format!("Screen capture stopped: {frames_captured} frames")
            }
            ScreenEventPayload::FrameCaptured {
                frame_id,
                width,
                height,
                size_bytes,
            } => {
                format!("Frame {frame_id}: {width}x{height} ({size_bytes} bytes)")
            }
            ScreenEventPayload::OcrCompleted {
                text_len,
                confidence,
                language,
                duration_ms,
            } => {
                format!("OCR ({language}): {text_len} chars at {confidence:.2} in {duration_ms}ms")
            }
            ScreenEventPayload::UITreeExtracted {
                element_count,
                depth,
                duration_ms,
            } => {
                format!("UI tree: {element_count} elements, depth {depth} in {duration_ms}ms")
            }
            ScreenEventPayload::GroundingCompleted {
                query,
                results,
                confidence,
                duration_ms,
            } => {
                format!(
                    "Grounding '{query}': {results} results at {confidence:.2} in {duration_ms}ms"
                )
            }
            ScreenEventPayload::ElementFound {
                element_id,
                element_type,
                confidence,
            } => {
                format!("Element {element_id} ({element_type}) at {confidence:.2}")
            }
            ScreenEventPayload::ScreenToolInvoked {
                tool,
                duration_ms,
                success,
            } => {
                format!("Tool '{tool}': {duration_ms}ms, success={success}")
            }
            ScreenEventPayload::AnalysisStarted { reason } => {
                format!("Screen analysis started: {reason}")
            }
            ScreenEventPayload::AnalysisFailed { reason, error } => {
                format!("Screen analysis failed: {reason} - {error}")
            }
        }
    }
}
