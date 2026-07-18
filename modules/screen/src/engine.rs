use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::capture::{ScreenCapture, ScreenCaptureFactory};
use crate::config::ScreenConfig;
use crate::error::ScreenResult;
use crate::grounding;
use crate::ocr;
use crate::permission::ScreenPermissionManager;
use crate::traits::{OCREngine, UITreeExtractor, VisualGrounding};
use crate::types::*;
use crate::ui_tree;

pub struct ScreenEngine {
    pub capture: Box<dyn ScreenCapture>,
    pub ui_tree: Arc<dyn UITreeExtractor>,
    pub ocr: Arc<dyn OCREngine>,
    pub grounding: Arc<dyn VisualGrounding>,
    pub permissions: Arc<ScreenPermissionManager>,
    pub config: Arc<parking_lot::RwLock<ScreenConfig>>,
}

impl ScreenEngine {
    pub fn new(
        config: ScreenConfig,
        permissions: Arc<ScreenPermissionManager>,
    ) -> ScreenResult<Self> {
        let capture = ScreenCaptureFactory::create()?;
        let ui_tree = ui_tree::create()?;
        let ocr = ocr::create()?;
        let grounding = grounding::create()?;

        Ok(Self {
            capture,
            ui_tree,
            ocr,
            grounding,
            permissions,
            config: Arc::new(parking_lot::RwLock::new(config)),
        })
    }

    pub async fn capture_frame(&mut self) -> ScreenResult<CapturedFrame> {
        self.capture.capture_frame().await
    }

    pub async fn extract_ui_tree(&self, frame: &CapturedFrame) -> ScreenResult<UITree> {
        self.ui_tree.extract_tree(frame).await
    }

    pub async fn recognize_text(&self, frame: &CapturedFrame) -> ScreenResult<OCRResult> {
        self.ocr.recognize(frame).await
    }

    pub async fn ground_element(
        &self,
        frame: &CapturedFrame,
        query: &GroundingQuery,
    ) -> ScreenResult<GroundingResult> {
        self.grounding.locate(frame, query).await
    }

    pub async fn full_analysis(&mut self) -> ScreenResult<ScreenAnalysis> {
        let frame = self.capture_frame().await?;
        let ui_tree = self.extract_ui_tree(&frame).await;
        let ocr = self.recognize_text(&frame).await;

        Ok(ScreenAnalysis {
            frame,
            ui_tree: ui_tree.ok(),
            ocr: ocr.ok(),
            grounded_elements: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenAnalysis {
    pub frame: CapturedFrame,
    pub ui_tree: Option<UITree>,
    pub ocr: Option<OCRResult>,
    pub grounded_elements: Vec<GroundingResult>,
}
