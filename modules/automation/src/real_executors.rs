use parking_lot::RwLock;
use std::sync::Arc;

use nova_screen::{
    GroundingQuery, ScreenEngine, ScreenInputBridge,
};
use nova_input::InputEngine;

use crate::action::{
    ActionExecutor, ActionType, ActionResult, DefaultActionExecutor,
};

fn screen_err(e: nova_screen::ScreenError) -> String {
    format!("screen error: {e}")
}

fn run_async_screen<R>(f: impl std::future::Future<Output = Result<R, nova_screen::ScreenError>>) -> Result<R, String> {
    let handle = tokio::runtime::Handle::try_current()
        .map_err(|_| "no tokio runtime available".to_string())?;
    handle.block_on(f).map_err(screen_err)
}

fn run_async_input(
    f: impl std::future::Future<Output = Result<nova_input::ActionResult, nova_input::InputError>>,
) -> Result<String, String> {
    let handle = tokio::runtime::Handle::try_current()
        .map_err(|_| "no tokio runtime available".to_string())?;
    let result = handle.block_on(f).map_err(|e| format!("input error: {e}"))?;
    if result.success {
        Ok(result.detail)
    } else {
        Err(format!("action failed: {}", result.detail))
    }
}

#[allow(clippy::await_holding_lock)]
fn capture_and_ground(
    screen: &Arc<RwLock<ScreenEngine>>,
    query: &str,
) -> Result<nova_screen::GroundingResult, String> {
    run_async_screen(async {
        let frame = {
            let mut eng = screen.write();
            eng.capture_frame().await?
        };
        let eng = screen.read();
        let gq = GroundingQuery {
            query: query.to_string(),
            context: None,
            max_results: 1,
            confidence_threshold: 0.3,
        };
        eng.ground_element(&frame, &gq).await
    })
}

#[allow(clippy::await_holding_lock)]
fn capture_and_ocr(
    screen: &Arc<RwLock<ScreenEngine>>,
) -> Result<nova_screen::OCRResult, String> {
    run_async_screen(async {
        let frame = {
            let mut eng = screen.write();
            eng.capture_frame().await?
        };
        let eng = screen.read();
        eng.recognize_text(&frame).await
    })
}

pub struct ScreenClickExecutor {
    screen: Arc<RwLock<ScreenEngine>>,
    input: Arc<dyn InputEngine>,
}

impl ScreenClickExecutor {
    pub fn new(screen: Arc<RwLock<ScreenEngine>>, input: Arc<dyn InputEngine>) -> Self {
        Self { screen, input }
    }
}

impl ActionExecutor for ScreenClickExecutor {
    fn execute(&self, action: &ActionType) -> ActionResult {
        match action {
            ActionType::ClickScreenElement { query } => {
                let grounding = match capture_and_ground(&self.screen, query) {
                    Ok(g) => g,
                    Err(e) => return ActionResult::failure(e),
                };
                let bridge = ScreenInputBridge::new(self.input.clone());
                match run_async_input(bridge.click_grounded(&grounding)) {
                    Ok(detail) => ActionResult::success(format!(
                        "clicked '{}' (confidence {:.2}): {detail}", query, grounding.confidence
                    )),
                    Err(e) => ActionResult::failure(e),
                }
            }
            ActionType::ClickScreenText { text } => {
                let ocr = match capture_and_ocr(&self.screen) {
                    Ok(o) => o,
                    Err(e) => return ActionResult::failure(e),
                };
                let bridge = ScreenInputBridge::new(self.input.clone());
                match run_async_input(bridge.click_ocr_text(&ocr, text)) {
                    Ok(detail) => ActionResult::success(format!("clicked OCR text '{}': {detail}", text)),
                    Err(e) => ActionResult::failure(e),
                }
            }
            _ => {
                let fallback = DefaultActionExecutor;
                fallback.execute(action)
            }
        }
    }

    fn kind(&self) -> &'static str {
        "screen-click"
    }
}

pub struct ScreenTypeExecutor {
    screen: Arc<RwLock<ScreenEngine>>,
    input: Arc<dyn InputEngine>,
}

impl ScreenTypeExecutor {
    pub fn new(screen: Arc<RwLock<ScreenEngine>>, input: Arc<dyn InputEngine>) -> Self {
        Self { screen, input }
    }
}

impl ActionExecutor for ScreenTypeExecutor {
    fn execute(&self, action: &ActionType) -> ActionResult {
        match action {
            ActionType::TypeIntoScreenElement { query, text } => {
                let grounding = match capture_and_ground(&self.screen, query) {
                    Ok(g) => g,
                    Err(e) => return ActionResult::failure(e),
                };
                let bridge = ScreenInputBridge::new(self.input.clone());
                match run_async_input(bridge.focus_and_type(&grounding.element, text)) {
                    Ok(detail) => ActionResult::success(format!(
                        "typed '{}' into '{}' (confidence {:.2}): {detail}",
                        text, query, grounding.confidence
                    )),
                    Err(e) => ActionResult::failure(format!("type failed: {e}")),
                }
            }
            _ => {
                let fallback = DefaultActionExecutor;
                fallback.execute(action)
            }
        }
    }

    fn kind(&self) -> &'static str {
        "screen-type"
    }
}

pub struct ScreenDragExecutor {
    screen: Arc<RwLock<ScreenEngine>>,
    input: Arc<dyn InputEngine>,
}

impl ScreenDragExecutor {
    pub fn new(screen: Arc<RwLock<ScreenEngine>>, input: Arc<dyn InputEngine>) -> Self {
        Self { screen, input }
    }
}

impl ActionExecutor for ScreenDragExecutor {
    fn execute(&self, action: &ActionType) -> ActionResult {
        match action {
            ActionType::DragScreenElements { from_query, to_query } => {
                let (from, to) = match capture_and_ground_two(&self.screen, from_query, to_query) {
                    Ok(pair) => pair,
                    Err(e) => return ActionResult::failure(e),
                };
                let bridge = ScreenInputBridge::new(self.input.clone());
                match run_async_input(bridge.drag_element_to(&from.element, &to.element)) {
                    Ok(detail) => ActionResult::success(format!(
                        "dragged '{}' -> '{}': {detail}", from_query, to_query
                    )),
                    Err(e) => ActionResult::failure(format!("drag failed: {e}")),
                }
            }
            _ => {
                let fallback = DefaultActionExecutor;
                fallback.execute(action)
            }
        }
    }

    fn kind(&self) -> &'static str {
        "screen-drag"
    }
}

pub struct ScreenSwipeExecutor {
    screen: Arc<RwLock<ScreenEngine>>,
    input: Arc<dyn InputEngine>,
}

impl ScreenSwipeExecutor {
    pub fn new(screen: Arc<RwLock<ScreenEngine>>, input: Arc<dyn InputEngine>) -> Self {
        Self { screen, input }
    }
}

impl ActionExecutor for ScreenSwipeExecutor {
    fn execute(&self, action: &ActionType) -> ActionResult {
        match action {
            ActionType::SwipeScreenElements { from_query, to_query } => {
                let (from, to) = match capture_and_ground_two(&self.screen, from_query, to_query) {
                    Ok(pair) => pair,
                    Err(e) => return ActionResult::failure(e),
                };
                let bridge = ScreenInputBridge::new(self.input.clone());
                match run_async_input(bridge.swipe_element_to(&from.element, &to.element)) {
                    Ok(detail) => ActionResult::success(format!(
                        "swiped '{}' -> '{}': {detail}", from_query, to_query
                    )),
                    Err(e) => ActionResult::failure(format!("swipe failed: {e}")),
                }
            }
            _ => {
                let fallback = DefaultActionExecutor;
                fallback.execute(action)
            }
        }
    }

    fn kind(&self) -> &'static str {
        "screen-swipe"
    }
}

#[allow(clippy::await_holding_lock)]
fn capture_and_ground_two(
    screen: &Arc<RwLock<ScreenEngine>>,
    query_a: &str,
    query_b: &str,
) -> Result<(nova_screen::GroundingResult, nova_screen::GroundingResult), String> {
    run_async_screen(async {
        let frame = {
            let mut eng = screen.write();
            eng.capture_frame().await?
        };
        let eng = screen.read();
        let gq_a = GroundingQuery {
            query: query_a.to_string(),
            context: None,
            max_results: 1,
            confidence_threshold: 0.3,
        };
        let gq_b = GroundingQuery {
            query: query_b.to_string(),
            context: None,
            max_results: 1,
            confidence_threshold: 0.3,
        };
        let a = eng.ground_element(&frame, &gq_a).await?;
        let b = eng.ground_element(&frame, &gq_b).await?;
        Ok((a, b))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_input::MockInputProvider;

    type EnginePair = (Arc<RwLock<ScreenEngine>>, Arc<dyn InputEngine>);

    fn try_engines() -> Option<EnginePair> {
        let config = nova_screen::ScreenConfig::default();
        let perms = Arc::new(nova_screen::ScreenPermissionManager::new());
        match ScreenEngine::new(config, perms) {
            Ok(engine) => {
                let input: Arc<dyn InputEngine> = Arc::new(MockInputProvider::new());
                Some((Arc::new(RwLock::new(engine)), input))
            }
            Err(_) => None,
        }
    }

    #[test]
    fn test_click_executor_falls_through_for_non_screen_actions() {
        let engines = try_engines();
        let (screen, input) = match engines {
            Some(ref pair) => (pair.0.clone(), pair.1.clone()),
            None => return,
        };
        let exec = ScreenClickExecutor::new(screen, input);
        let result = exec.execute(&ActionType::Speak { text: "hi".into() });
        assert!(result.success);
    }

    #[test]
    fn test_type_executor_falls_through() {
        let engines = try_engines();
        let (screen, input) = match engines {
            Some(ref pair) => (pair.0.clone(), pair.1.clone()),
            None => return,
        };
        let exec = ScreenTypeExecutor::new(screen, input);
        let result = exec.execute(&ActionType::Notify {
            title: "t".into(), body: "b".into(),
            priority: crate::action::NotifyPriority::Normal,
        });
        assert!(result.success);
    }

    #[test]
    fn test_drag_executor_kind() {
        let (screen, input) = match try_engines() {
            Some(ref pair) => (pair.0.clone(), pair.1.clone()),
            None => return,
        };
        let exec = ScreenDragExecutor::new(screen, input);
        assert_eq!(exec.kind(), "screen-drag");
    }

    #[test]
    fn test_swipe_executor_kind() {
        let (screen, input) = match try_engines() {
            Some(ref pair) => (pair.0.clone(), pair.1.clone()),
            None => return,
        };
        let exec = ScreenSwipeExecutor::new(screen, input);
        assert_eq!(exec.kind(), "screen-swipe");
    }

    #[test]
    fn test_click_executor_kind() {
        let (screen, input) = match try_engines() {
            Some(ref pair) => (pair.0.clone(), pair.1.clone()),
            None => return,
        };
        let exec = ScreenClickExecutor::new(screen, input);
        assert_eq!(exec.kind(), "screen-click");
    }

    #[test]
    fn test_type_executor_kind() {
        let (screen, input) = match try_engines() {
            Some(ref pair) => (pair.0.clone(), pair.1.clone()),
            None => return,
        };
        let exec = ScreenTypeExecutor::new(screen, input);
        assert_eq!(exec.kind(), "screen-type");
    }
}
