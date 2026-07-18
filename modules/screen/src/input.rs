use nova_input::{ActionResult, InputAction, InputEngine, InputError, InputResult};
use nova_input::{KeyboardAction, MouseAction, MouseButton, TouchAction};
use std::sync::Arc;

use crate::error::ScreenError;
use crate::types::{GroundingResult, OCRResult, Point, Rect, UIElementRef, UIElementType};

/// Validated actionable target derived from screen understanding.
#[derive(Debug, Clone)]
pub enum ScreenInputTarget {
    Element(UIElementRef),
    Coordinate(Point),
    BoundsCenter(Rect),
}

/// Screen-aware input action combining a target and an interaction type.
#[derive(Debug, Clone)]
pub enum ScreenInputAction {
    Click {
        target: ScreenInputTarget,
    },
    DoubleClick {
        target: ScreenInputTarget,
    },
    RightClick {
        target: ScreenInputTarget,
    },
    DragTo {
        from: ScreenInputTarget,
        to: ScreenInputTarget,
    },
    FocusAndType {
        target: ScreenInputTarget,
        text: String,
    },
    Tap {
        target: ScreenInputTarget,
    },
    SwipeTo {
        from: ScreenInputTarget,
        to: ScreenInputTarget,
    },
    TypeText {
        text: String,
    },
}

/// Extension trait adding convenience methods to nova_input types.
pub trait InputActionExt {
    fn success(detail: impl Into<String>) -> Self;
    fn failure(detail: impl Into<String>) -> Self;
}

impl InputActionExt for ActionResult {
    fn success(detail: impl Into<String>) -> Self {
        ActionResult {
            success: true,
            detail: detail.into(),
        }
    }

    fn failure(detail: impl Into<String>) -> Self {
        ActionResult {
            success: false,
            detail: detail.into(),
        }
    }
}

/// Converts screen comprehension results into executable input actions.
///
/// Translates `GroundingResult`, `UIElementRef`, `Rect`, and `Point`
/// targets into platform-agnostic `InputAction` values that are
/// dispatched through an `InputEngine`.
pub struct ScreenInputBridge {
    engine: Arc<dyn InputEngine>,
}

impl ScreenInputBridge {
    pub fn new(engine: Arc<dyn InputEngine>) -> Self {
        Self { engine }
    }

    pub fn engine(&self) -> &Arc<dyn InputEngine> {
        &self.engine
    }

    // ------------------------------------------------------------------
    // Validation helpers
    // ------------------------------------------------------------------

    fn check_bounds(bounds: &Rect) -> Result<(), ScreenError> {
        if bounds.width == 0 || bounds.height == 0 {
            return Err(ScreenError::InvalidRegion(format!(
                "element bounds are empty ({}x{})",
                bounds.width, bounds.height
            )));
        }
        Ok(())
    }

    fn check_element_actionable(element: &UIElementRef) -> Result<(), ScreenError> {
        Self::check_bounds(&element.bounds)
    }

    fn check_element_text_input(element: &UIElementRef) -> Result<(), ScreenError> {
        Self::check_element_actionable(element)?;
        if !matches!(
            element.element_type,
            UIElementType::Edit | UIElementType::ComboBox
        ) {
            return Err(ScreenError::Unsupported(format!(
                "element type {:?} does not support text input; expected Edit or ComboBox",
                element.element_type
            )));
        }
        Ok(())
    }

    fn check_element_draggable(element: &UIElementRef) -> Result<(), ScreenError> {
        Self::check_element_actionable(element)
    }

    /// Returns `true` if the element has a non-empty bounding box.
    pub fn is_element_actionable(&self, element: &UIElementRef) -> bool {
        element.bounds.width > 0 && element.bounds.height > 0
    }

    /// Returns `true` if the element supports receiving text input.
    pub fn is_element_text_input_capable(&self, element: &UIElementRef) -> bool {
        matches!(
            element.element_type,
            UIElementType::Edit | UIElementType::ComboBox
        )
    }

    // ------------------------------------------------------------------
    // Coordinate resolution
    // ------------------------------------------------------------------

    fn resolve_target_point(target: &ScreenInputTarget) -> Result<nova_input::Point, ScreenError> {
        match target {
            ScreenInputTarget::Element(e) => {
                Self::check_element_actionable(e)?;
                Ok(Self::screen_point_to_input(e.bounds.center()))
            }
            ScreenInputTarget::Coordinate(p) => Ok(Self::screen_point_to_input(*p)),
            ScreenInputTarget::BoundsCenter(r) => {
                Self::check_bounds(r)?;
                Ok(Self::screen_point_to_input(r.center()))
            }
        }
    }

    fn screen_point_to_input(p: Point) -> nova_input::Point {
        nova_input::Point { x: p.x, y: p.y }
    }

    // ------------------------------------------------------------------
    // Element-focused convenience methods
    // ------------------------------------------------------------------

    /// Click at the center of the given `UIElementRef`.
    pub async fn click_element(&self, element: &UIElementRef) -> InputResult<ActionResult> {
        Self::check_element_actionable(element)
            .map_err(|e| InputError::ProviderError(format!("element not actionable: {e}")))?;
        let point = Self::screen_point_to_input(element.bounds.center());
        self.engine
            .execute(&InputAction::Mouse(MouseAction::Click {
                point,
                button: MouseButton::Left,
                count: 1,
            }))
            .await
    }

    /// Click at the center of the element identified by a `GroundingResult`.
    pub async fn click_grounded(&self, result: &GroundingResult) -> InputResult<ActionResult> {
        self.click_element(&result.element).await
    }

    /// Click at the center of a bounding rectangle.
    pub async fn click_rect(&self, rect: &Rect) -> InputResult<ActionResult> {
        Self::check_bounds(rect)
            .map_err(|e| InputError::ProviderError(format!("invalid rect: {e}")))?;
        let point = Self::screen_point_to_input(rect.center());
        self.engine
            .execute(&InputAction::Mouse(MouseAction::Click {
                point,
                button: MouseButton::Left,
                count: 1,
            }))
            .await
    }

    /// Double-click at the center of the given element.
    pub async fn double_click_element(&self, element: &UIElementRef) -> InputResult<ActionResult> {
        Self::check_element_actionable(element)
            .map_err(|e| InputError::ProviderError(format!("element not actionable: {e}")))?;
        let point = Self::screen_point_to_input(element.bounds.center());
        self.engine
            .execute(&InputAction::Mouse(MouseAction::Click {
                point,
                button: MouseButton::Left,
                count: 2,
            }))
            .await
    }

    /// Right-click at the center of the given element.
    pub async fn right_click_element(&self, element: &UIElementRef) -> InputResult<ActionResult> {
        Self::check_element_actionable(element)
            .map_err(|e| InputError::ProviderError(format!("element not actionable: {e}")))?;
        let point = Self::screen_point_to_input(element.bounds.center());
        self.engine
            .execute(&InputAction::Mouse(MouseAction::Click {
                point,
                button: MouseButton::Right,
                count: 1,
            }))
            .await
    }

    /// Focus an element (click it), then type the given text.
    pub async fn focus_and_type(
        &self,
        element: &UIElementRef,
        text: &str,
    ) -> InputResult<ActionResult> {
        Self::check_element_text_input(element)
            .map_err(|e| InputError::ProviderError(format!("cannot type into element: {e}")))?;
        // Click first to focus.
        self.click_element(element).await?;
        self.engine
            .execute(&InputAction::Keyboard(KeyboardAction::TypeText {
                text: text.to_string(),
            }))
            .await
    }

    /// Type text directly (no target – just keystrokes at the current focus).
    pub async fn type_text(&self, text: &str) -> InputResult<ActionResult> {
        self.engine
            .execute(&InputAction::Keyboard(KeyboardAction::TypeText {
                text: text.to_string(),
            }))
            .await
    }

    /// Drag from one element to another.
    pub async fn drag_element_to(
        &self,
        from: &UIElementRef,
        to: &UIElementRef,
    ) -> InputResult<ActionResult> {
        Self::check_element_draggable(from).map_err(|e| {
            InputError::ProviderError(format!("source element not actionable: {e}"))
        })?;
        Self::check_element_draggable(to).map_err(|e| {
            InputError::ProviderError(format!("target element not actionable: {e}"))
        })?;
        let from_pt = Self::screen_point_to_input(from.bounds.center());
        let to_pt = Self::screen_point_to_input(to.bounds.center());
        self.engine
            .execute(&InputAction::Mouse(MouseAction::Drag {
                from: from_pt,
                to: to_pt,
                button: MouseButton::Left,
            }))
            .await
    }

    /// Tap (touch) at the center of the given element.
    pub async fn tap_element(&self, element: &UIElementRef) -> InputResult<ActionResult> {
        Self::check_element_actionable(element)
            .map_err(|e| InputError::ProviderError(format!("element not actionable: {e}")))?;
        let point = Self::screen_point_to_input(element.bounds.center());
        self.engine
            .execute(&InputAction::Touch(TouchAction::Tap { point }))
            .await
    }

    /// Swipe from one element to another.
    pub async fn swipe_element_to(
        &self,
        from: &UIElementRef,
        to: &UIElementRef,
    ) -> InputResult<ActionResult> {
        Self::check_element_actionable(from).map_err(|e| {
            InputError::ProviderError(format!("source element not actionable: {e}"))
        })?;
        Self::check_element_actionable(to).map_err(|e| {
            InputError::ProviderError(format!("target element not actionable: {e}"))
        })?;
        let from_pt = Self::screen_point_to_input(from.bounds.center());
        let to_pt = Self::screen_point_to_input(to.bounds.center());
        self.engine
            .execute(&InputAction::Touch(TouchAction::Swipe {
                from: from_pt,
                to: to_pt,
                duration_ms: 200,
            }))
            .await
    }

    // ------------------------------------------------------------------
    // Generic action dispatcher
    // ------------------------------------------------------------------

    /// Execute a `ScreenInputAction` by resolving all targets and
    /// dispatching the corresponding `InputAction` through the engine.
    pub async fn execute_screen_action(
        &self,
        action: &ScreenInputAction,
    ) -> InputResult<ActionResult> {
        match action {
            ScreenInputAction::Click { target } => {
                let point = Self::resolve_target_point(target)
                    .map_err(|e| InputError::ProviderError(format!("invalid target: {e}")))?;
                self.engine
                    .execute(&InputAction::Mouse(MouseAction::Click {
                        point,
                        button: MouseButton::Left,
                        count: 1,
                    }))
                    .await
            }
            ScreenInputAction::DoubleClick { target } => {
                let point = Self::resolve_target_point(target)
                    .map_err(|e| InputError::ProviderError(format!("invalid target: {e}")))?;
                self.engine
                    .execute(&InputAction::Mouse(MouseAction::Click {
                        point,
                        button: MouseButton::Left,
                        count: 2,
                    }))
                    .await
            }
            ScreenInputAction::RightClick { target } => {
                let point = Self::resolve_target_point(target)
                    .map_err(|e| InputError::ProviderError(format!("invalid target: {e}")))?;
                self.engine
                    .execute(&InputAction::Mouse(MouseAction::Click {
                        point,
                        button: MouseButton::Right,
                        count: 1,
                    }))
                    .await
            }
            ScreenInputAction::DragTo { from, to } => {
                let from_pt = Self::resolve_target_point(from)
                    .map_err(|e| InputError::ProviderError(format!("invalid source: {e}")))?;
                let to_pt = Self::resolve_target_point(to)
                    .map_err(|e| InputError::ProviderError(format!("invalid target: {e}")))?;
                self.engine
                    .execute(&InputAction::Mouse(MouseAction::Drag {
                        from: from_pt,
                        to: to_pt,
                        button: MouseButton::Left,
                    }))
                    .await
            }
            ScreenInputAction::FocusAndType { target, text } => {
                if let ScreenInputTarget::Element(ref e) = target {
                    Self::check_element_text_input(e).map_err(|e| {
                        InputError::ProviderError(format!("element not editable: {e}"))
                    })?;
                }
                let point = Self::resolve_target_point(target)
                    .map_err(|e| InputError::ProviderError(format!("invalid target: {e}")))?;
                self.engine
                    .execute(&InputAction::Mouse(MouseAction::Click {
                        point,
                        button: MouseButton::Left,
                        count: 1,
                    }))
                    .await?;
                self.engine
                    .execute(&InputAction::Keyboard(KeyboardAction::TypeText {
                        text: text.clone(),
                    }))
                    .await
            }
            ScreenInputAction::Tap { target } => {
                let point = Self::resolve_target_point(target)
                    .map_err(|e| InputError::ProviderError(format!("invalid target: {e}")))?;
                self.engine
                    .execute(&InputAction::Touch(TouchAction::Tap { point }))
                    .await
            }
            ScreenInputAction::SwipeTo { from, to } => {
                let from_pt = Self::resolve_target_point(from)
                    .map_err(|e| InputError::ProviderError(format!("invalid source: {e}")))?;
                let to_pt = Self::resolve_target_point(to)
                    .map_err(|e| InputError::ProviderError(format!("invalid target: {e}")))?;
                self.engine
                    .execute(&InputAction::Touch(TouchAction::Swipe {
                        from: from_pt,
                        to: to_pt,
                        duration_ms: 200,
                    }))
                    .await
            }
            ScreenInputAction::TypeText { text } => {
                self.engine
                    .execute(&InputAction::Keyboard(KeyboardAction::TypeText {
                        text: text.clone(),
                    }))
                    .await
            }
        }
    }

    // ------------------------------------------------------------------
    // OCR‑driven convenience methods
    // ------------------------------------------------------------------

    /// Click the center of the first OCR region that contains the given
    /// substring in its text.
    pub async fn click_ocr_text(
        &self,
        ocr: &OCRResult,
        text_substring: &str,
    ) -> InputResult<ActionResult> {
        let region = ocr
            .regions
            .iter()
            .find(|r| r.text.contains(text_substring))
            .ok_or_else(|| {
                InputError::ProviderError(format!("no OCR region contains \"{text_substring}\""))
            })?;
        self.click_rect(&region.bounds).await
    }

    /// Type text into the first OCR region that looks like an editable
    /// area (by clicking its center first).
    pub async fn type_into_ocr_region(
        &self,
        ocr: &OCRResult,
        region_index: usize,
        text: &str,
    ) -> InputResult<ActionResult> {
        let region = ocr.regions.get(region_index).ok_or_else(|| {
            InputError::ProviderError(format!(
                "OCR region index {region_index} out of range (max {})",
                ocr.regions.len().saturating_sub(1)
            ))
        })?;
        Self::check_bounds(&region.bounds)
            .map_err(|e| InputError::ProviderError(format!("invalid OCR region bounds: {e}")))?;
        self.click_rect(&region.bounds).await?;
        self.engine
            .execute(&InputAction::Keyboard(KeyboardAction::TypeText {
                text: text.to_string(),
            }))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GroundingResult, OCRRegion, Point, Rect, UIElementRef, UIElementType};
    use nova_input::MockInputProvider;
    use std::collections::HashMap;

    fn sample_element() -> UIElementRef {
        UIElementRef {
            element_id: "btn_ok".into(),
            element_type: UIElementType::Button,
            bounds: Rect {
                x: 100,
                y: 200,
                width: 80,
                height: 30,
            },
            text: Some("OK".into()),
            attributes: HashMap::new(),
        }
    }

    fn sample_edit_element() -> UIElementRef {
        UIElementRef {
            element_id: "txt_name".into(),
            element_type: UIElementType::Edit,
            bounds: Rect {
                x: 50,
                y: 100,
                width: 200,
                height: 24,
            },
            text: Some("".into()),
            attributes: HashMap::new(),
        }
    }

    fn sample_zero_element() -> UIElementRef {
        UIElementRef {
            element_id: "empty".into(),
            element_type: UIElementType::Pane,
            bounds: Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            text: None,
            attributes: HashMap::new(),
        }
    }

    fn sample_grounding_result() -> GroundingResult {
        GroundingResult {
            element: sample_element(),
            confidence: 0.95,
            match_reason: "exact name match".into(),
        }
    }

    fn sample_ocr_result() -> OCRResult {
        OCRResult {
            text: "Hello World Submit".into(),
            confidence: 0.9,
            language: "en".into(),
            regions: vec![
                OCRRegion {
                    text: "Hello".into(),
                    confidence: 0.95,
                    bounds: Rect {
                        x: 10,
                        y: 10,
                        width: 50,
                        height: 20,
                    },
                },
                OCRRegion {
                    text: "World".into(),
                    confidence: 0.85,
                    bounds: Rect {
                        x: 70,
                        y: 10,
                        width: 60,
                        height: 20,
                    },
                },
                OCRRegion {
                    text: "Submit".into(),
                    confidence: 0.9,
                    bounds: Rect {
                        x: 140,
                        y: 10,
                        width: 70,
                        height: 20,
                    },
                },
            ],
        }
    }

    fn make_bridge() -> ScreenInputBridge {
        ScreenInputBridge::new(Arc::new(MockInputProvider::new()))
    }

    #[tokio::test]
    async fn test_click_element_success() {
        let bridge = make_bridge();
        let result = bridge.click_element(&sample_element()).await;
        assert!(result.is_ok());
        let ar = result.unwrap();
        assert!(ar.success);
    }

    #[tokio::test]
    async fn test_click_element_zero_bounds_fails() {
        let bridge = make_bridge();
        let result = bridge.click_element(&sample_zero_element()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            InputError::ProviderError(msg) => {
                assert!(msg.contains("not actionable"), "got: {msg}");
            }
            other => panic!("expected ProviderError, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_click_grounded() {
        let bridge = make_bridge();
        let result = bridge.click_grounded(&sample_grounding_result()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_click_rect() {
        let bridge = make_bridge();
        let rect = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        };
        let result = bridge.click_rect(&rect).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_click_rect_zero_bounds_fails() {
        let bridge = make_bridge();
        let rect = Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        let result = bridge.click_rect(&rect).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_double_click_element() {
        let bridge = make_bridge();
        let result = bridge.double_click_element(&sample_element()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_right_click_element() {
        let bridge = make_bridge();
        let result = bridge.right_click_element(&sample_element()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_focus_and_type_into_edit() {
        let bridge = make_bridge();
        let result = bridge.focus_and_type(&sample_edit_element(), "test").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_focus_and_type_button_fails() {
        let bridge = make_bridge();
        let result = bridge.focus_and_type(&sample_element(), "test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_type_text() {
        let bridge = make_bridge();
        let result = bridge.type_text("hello").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_drag_element_to() {
        let bridge = make_bridge();
        let from = sample_element();
        let to = sample_edit_element();
        let result = bridge.drag_element_to(&from, &to).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_drag_from_zero_bounds_fails() {
        let bridge = make_bridge();
        let result = bridge
            .drag_element_to(&sample_zero_element(), &sample_element())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tap_element() {
        let bridge = make_bridge();
        let result = bridge.tap_element(&sample_element()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_swipe_element_to() {
        let bridge = make_bridge();
        let from = sample_edit_element();
        let to = sample_element();
        let result = bridge.swipe_element_to(&from, &to).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_screen_action_click() {
        let bridge = make_bridge();
        let target = ScreenInputTarget::Element(sample_element());
        let action = ScreenInputAction::Click { target };
        let result = bridge.execute_screen_action(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_screen_action_double_click() {
        let bridge = make_bridge();
        let target = ScreenInputTarget::Element(sample_element());
        let action = ScreenInputAction::DoubleClick { target };
        let result = bridge.execute_screen_action(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_screen_action_right_click() {
        let bridge = make_bridge();
        let target = ScreenInputTarget::Element(sample_element());
        let action = ScreenInputAction::RightClick { target };
        let result = bridge.execute_screen_action(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_screen_action_drag() {
        let bridge = make_bridge();
        let from = ScreenInputTarget::Element(sample_edit_element());
        let to = ScreenInputTarget::Element(sample_element());
        let action = ScreenInputAction::DragTo { from, to };
        let result = bridge.execute_screen_action(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_screen_action_focus_and_type() {
        let bridge = make_bridge();
        let target = ScreenInputTarget::Element(sample_edit_element());
        let action = ScreenInputAction::FocusAndType {
            target,
            text: "hello".into(),
        };
        let result = bridge.execute_screen_action(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_screen_action_tap() {
        let bridge = make_bridge();
        let target = ScreenInputTarget::Coordinate(Point { x: 100, y: 200 });
        let action = ScreenInputAction::Tap { target };
        let result = bridge.execute_screen_action(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_screen_action_swipe() {
        let bridge = make_bridge();
        let from = ScreenInputTarget::Coordinate(Point { x: 100, y: 200 });
        let to = ScreenInputTarget::Coordinate(Point { x: 300, y: 200 });
        let action = ScreenInputAction::SwipeTo { from, to };
        let result = bridge.execute_screen_action(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_screen_action_type_text() {
        let bridge = make_bridge();
        let action = ScreenInputAction::TypeText {
            text: "direct text".into(),
        };
        let result = bridge.execute_screen_action(&action).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_click_ocr_text_found() {
        let bridge = make_bridge();
        let ocr = sample_ocr_result();
        let result = bridge.click_ocr_text(&ocr, "Submit").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_click_ocr_text_not_found() {
        let bridge = make_bridge();
        let ocr = sample_ocr_result();
        let result = bridge.click_ocr_text(&ocr, "NONEXISTENT").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_type_into_ocr_region() {
        let bridge = make_bridge();
        let ocr = sample_ocr_result();
        let result = bridge.type_into_ocr_region(&ocr, 0, "typed text").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_type_into_ocr_region_bad_index() {
        let bridge = make_bridge();
        let ocr = sample_ocr_result();
        let result = bridge.type_into_ocr_region(&ocr, 99, "x").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_is_element_actionable() {
        let bridge = make_bridge();
        assert!(bridge.is_element_actionable(&sample_element()));
        assert!(!bridge.is_element_actionable(&sample_zero_element()));
    }

    #[tokio::test]
    async fn test_is_element_text_input_capable() {
        let bridge = make_bridge();
        assert!(bridge.is_element_text_input_capable(&sample_edit_element()));
        assert!(!bridge.is_element_text_input_capable(&sample_element()));
    }

    #[tokio::test]
    async fn test_input_action_ext() {
        let s = <ActionResult as InputActionExt>::success("ok");
        assert!(s.success);
        assert_eq!(s.detail, "ok");
        let f = <ActionResult as InputActionExt>::failure("fail");
        assert!(!f.success);
        assert_eq!(f.detail, "fail");
    }

    #[tokio::test]
    async fn test_click_invalid_target_via_screen_action() {
        let bridge = make_bridge();
        let target = ScreenInputTarget::BoundsCenter(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        let action = ScreenInputAction::Click { target };
        let result = bridge.execute_screen_action(&action).await;
        assert!(result.is_err());
    }
}
