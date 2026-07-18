use crate::error::ScreenResult;
use crate::types::*;
use async_trait::async_trait;

/// UI Tree extraction trait
#[async_trait]
pub trait UITreeExtractor: Send + Sync {
    fn id(&self) -> &str;
    async fn extract_tree(&self, frame: &CapturedFrame) -> ScreenResult<UITree>;
    async fn find_element(&self, tree: &UITree, query: &GroundingQuery) -> ScreenResult<Option<UIElementRef>>;
    async fn get_element_bounds(&self, element: &UIElementRef) -> ScreenResult<Rect>;
}

/// OCR Engine trait
#[async_trait]
pub trait OCREngine: Send + Sync {
    fn id(&self) -> &str;
    async fn recognize(&self, frame: &CapturedFrame) -> ScreenResult<OCRResult>;
    async fn recognize_region(&self, frame: &CapturedFrame, region: Rect) -> ScreenResult<OCRResult>;
    fn supported_languages(&self) -> Vec<String>;
}

/// Visual Grounding trait
#[async_trait]
pub trait VisualGrounding: Send + Sync {
    fn id(&self) -> &str;
    async fn locate(&self, frame: &CapturedFrame, query: &GroundingQuery) -> ScreenResult<GroundingResult>;
    async fn locate_all(&self, frame: &CapturedFrame, query: &GroundingQuery) -> ScreenResult<Vec<GroundingResult>>;
}
