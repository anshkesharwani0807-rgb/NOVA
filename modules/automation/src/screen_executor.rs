use parking_lot::RwLock;
use std::sync::Arc;

use nova_input::InputEngine;
use nova_screen::{GroundingQuery, ScreenEngine, ScreenError, ScreenInputBridge};

use crate::action::{ActionExecutor, ActionResult, ActionType, DefaultActionExecutor};

/// An `ActionExecutor` that uses screen capture, visual grounding, and OCR
/// to resolve screen-aware actions and delegates to the `InputEngine` for
/// execution.
///
/// This executor supports:
/// - clicking elements identified by a grounding query
/// - typing text into grounded text fields
/// - clicking text regions identified by OCR
/// - dragging and swiping between grounded elements
///
/// It falls back to `DefaultActionExecutor` for all other action types.
pub struct ScreenAwareExecutor {
    screen_engine: Option<Arc<RwLock<ScreenEngine>>>,
    input_engine: Option<Arc<dyn InputEngine>>,
}

impl ScreenAwareExecutor {
    pub fn new(
        screen_engine: Option<Arc<RwLock<ScreenEngine>>>,
        input_engine: Option<Arc<dyn InputEngine>>,
    ) -> Self {
        Self {
            screen_engine,
            input_engine,
        }
    }

    fn run_async<F, T>(f: F) -> Result<T, String>
    where
        F: std::future::Future<Output = Result<T, String>>,
    {
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|_| "no tokio runtime available".to_string())?;
        handle.block_on(f)
    }

    fn screen_err(e: ScreenError) -> String {
        format!("screen error: {e}")
    }

    fn input_err(e: nova_input::InputError) -> String {
        format!("input error: {e}")
    }

    #[allow(clippy::await_holding_lock)]
    fn execute_screen_action(&self, action: &ActionType) -> ActionResult {
        let screen = match self.screen_engine.as_ref() {
            Some(s) => s.clone(),
            None => return ActionResult::failure("screen engine not configured"),
        };
        let input = match self.input_engine.as_ref() {
            Some(e) => e.clone(),
            None => return ActionResult::failure("input engine not configured for screen actions"),
        };

        let bridge = ScreenInputBridge::new(input);

        match action {
            ActionType::ClickScreenElement { query } => Self::run_async(async {
                let frame = {
                    let mut eng = screen.write();
                    eng.capture_frame().await.map_err(Self::screen_err)?
                };
                let grounding = {
                    let eng = screen.read();
                    let gq = GroundingQuery {
                        query: query.clone(),
                        context: None,
                        max_results: 1,
                        confidence_threshold: 0.3,
                    };
                    eng.ground_element(&frame, &gq)
                        .await
                        .map_err(Self::screen_err)?
                };
                bridge
                    .click_grounded(&grounding)
                    .await
                    .map_err(Self::input_err)?;
                Ok(format!(
                    "clicked '{}' (confidence {:.2})",
                    query, grounding.confidence
                ))
            })
            .map(ActionResult::success)
            .unwrap_or_else(ActionResult::failure),

            ActionType::TypeIntoScreenElement { query, text } => Self::run_async(async {
                let frame = {
                    let mut eng = screen.write();
                    eng.capture_frame().await.map_err(Self::screen_err)?
                };
                let grounding = {
                    let eng = screen.read();
                    let gq = GroundingQuery {
                        query: query.clone(),
                        context: None,
                        max_results: 1,
                        confidence_threshold: 0.3,
                    };
                    eng.ground_element(&frame, &gq)
                        .await
                        .map_err(Self::screen_err)?
                };
                bridge
                    .focus_and_type(&grounding.element, text)
                    .await
                    .map_err(Self::input_err)?;
                Ok(format!(
                    "typed '{}' into '{}' (confidence {:.2})",
                    text, query, grounding.confidence
                ))
            })
            .map(ActionResult::success)
            .unwrap_or_else(ActionResult::failure),

            ActionType::ClickScreenText { text } => Self::run_async(async {
                let frame = {
                    let mut eng = screen.write();
                    eng.capture_frame().await.map_err(Self::screen_err)?
                };
                let ocr = {
                    let eng = screen.read();
                    eng.recognize_text(&frame).await.map_err(Self::screen_err)?
                };
                bridge
                    .click_ocr_text(&ocr, text)
                    .await
                    .map_err(Self::input_err)?;
                Ok(format!("clicked OCR text '{}'", text))
            })
            .map(ActionResult::success)
            .unwrap_or_else(ActionResult::failure),

            ActionType::DragScreenElements {
                from_query,
                to_query,
            } => Self::run_async(async {
                let frame = {
                    let mut eng = screen.write();
                    eng.capture_frame().await.map_err(Self::screen_err)?
                };
                let (from_g, to_g) = {
                    let eng = screen.read();
                    let gq_from = GroundingQuery {
                        query: from_query.clone(),
                        context: None,
                        max_results: 1,
                        confidence_threshold: 0.3,
                    };
                    let gq_to = GroundingQuery {
                        query: to_query.clone(),
                        context: None,
                        max_results: 1,
                        confidence_threshold: 0.3,
                    };
                    let from = eng
                        .ground_element(&frame, &gq_from)
                        .await
                        .map_err(Self::screen_err)?;
                    let to = eng
                        .ground_element(&frame, &gq_to)
                        .await
                        .map_err(Self::screen_err)?;
                    (from, to)
                };
                bridge
                    .drag_element_to(&from_g.element, &to_g.element)
                    .await
                    .map_err(Self::input_err)?;
                Ok(format!("dragged '{}' -> '{}'", from_query, to_query))
            })
            .map(ActionResult::success)
            .unwrap_or_else(ActionResult::failure),

            ActionType::SwipeScreenElements {
                from_query,
                to_query,
            } => Self::run_async(async {
                let frame = {
                    let mut eng = screen.write();
                    eng.capture_frame().await.map_err(Self::screen_err)?
                };
                let (from_g, to_g) = {
                    let eng = screen.read();
                    let gq_from = GroundingQuery {
                        query: from_query.clone(),
                        context: None,
                        max_results: 1,
                        confidence_threshold: 0.3,
                    };
                    let gq_to = GroundingQuery {
                        query: to_query.clone(),
                        context: None,
                        max_results: 1,
                        confidence_threshold: 0.3,
                    };
                    let from = eng
                        .ground_element(&frame, &gq_from)
                        .await
                        .map_err(Self::screen_err)?;
                    let to = eng
                        .ground_element(&frame, &gq_to)
                        .await
                        .map_err(Self::screen_err)?;
                    (from, to)
                };
                bridge
                    .swipe_element_to(&from_g.element, &to_g.element)
                    .await
                    .map_err(Self::input_err)?;
                Ok(format!("swiped '{}' -> '{}'", from_query, to_query))
            })
            .map(ActionResult::success)
            .unwrap_or_else(ActionResult::failure),

            _ => {
                let fallback = DefaultActionExecutor;
                fallback.execute(action)
            }
        }
    }
}

impl ActionExecutor for ScreenAwareExecutor {
    fn execute(&self, action: &ActionType) -> ActionResult {
        match action {
            ActionType::ClickScreenElement { .. }
            | ActionType::TypeIntoScreenElement { .. }
            | ActionType::ClickScreenText { .. }
            | ActionType::DragScreenElements { .. }
            | ActionType::SwipeScreenElements { .. } => self.execute_screen_action(action),
            ActionType::InputInjection(_params) => {
                // Re-use InputAwareExecutor for raw input actions.
                let input = match self.input_engine.as_ref() {
                    Some(e) => e.clone(),
                    None => {
                        return ActionResult::failure(
                            "input engine not configured for InputInjection",
                        )
                    }
                };
                let executor = super::execution::InputAwareExecutor::new(Some(input));
                executor.execute(action)
            }
            other => {
                let fallback = DefaultActionExecutor;
                fallback.execute(other)
            }
        }
    }

    fn kind(&self) -> &'static str {
        "screen-aware"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_aware_executor_reports_missing_engine() {
        let executor = ScreenAwareExecutor::new(None, None);
        let result = executor.execute(&ActionType::ClickScreenElement {
            query: "button".into(),
        });
        assert!(!result.success);
        assert!(result.message.contains("screen engine not configured"));
    }

    #[test]
    fn test_screen_aware_executor_reports_missing_input() {
        let executor = ScreenAwareExecutor::new(None, None);
        let result = executor.execute(&ActionType::InputInjection(
            crate::action::InputInjectionParams {
                action_type: "click".into(),
                params: Default::default(),
            },
        ));
        assert!(!result.success);
        assert!(result.message.contains("input engine not configured"));
    }

    #[test]
    fn test_screen_aware_executor_falls_back_to_default() {
        let executor = ScreenAwareExecutor::new(None, None);
        let result = executor.execute(&ActionType::Speak {
            text: "hello".into(),
        });
        assert!(result.success);
    }

    #[test]
    fn test_screen_aware_executor_kind() {
        let executor = ScreenAwareExecutor::new(None, None);
        assert_eq!(executor.kind(), "screen-aware");
    }
}
