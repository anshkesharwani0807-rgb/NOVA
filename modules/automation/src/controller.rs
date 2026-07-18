use parking_lot::RwLock;
use std::sync::Arc;

use nova_screen::{GroundingQuery, ScreenEngine, ScreenInputBridge};
use nova_input::InputEngine;

use crate::action::{ActionExecutor, ActionResult, DefaultActionExecutor, ActionType};

fn screen_err(e: nova_screen::ScreenError) -> String {
    format!("screen error: {e}")
}

fn input_err(e: nova_input::InputError) -> String {
    format!("input error: {e}")
}

pub struct ComputerController {
    screen: Option<Arc<RwLock<ScreenEngine>>>,
    input: Option<Arc<dyn InputEngine>>,
}

impl ComputerController {
    pub fn new() -> Self {
        Self {
            screen: None,
            input: None,
        }
    }

    pub fn with_screen(mut self, screen: Arc<RwLock<ScreenEngine>>) -> Self {
        self.screen = Some(screen);
        self
    }

    pub fn with_input(mut self, input: Arc<dyn InputEngine>) -> Self {
        self.input = Some(input);
        self
    }

    pub fn set_screen(&mut self, screen: Arc<RwLock<ScreenEngine>>) {
        self.screen = Some(screen);
    }

    pub fn set_input(&mut self, input: Arc<dyn InputEngine>) {
        self.input = Some(input);
    }

    fn require_screen(&self) -> Result<Arc<RwLock<ScreenEngine>>, String> {
        self.screen
            .clone()
            .ok_or_else(|| "screen engine not configured".to_string())
    }

    fn require_input(&self) -> Result<Arc<dyn InputEngine>, String> {
        self.input
            .clone()
            .ok_or_else(|| "input engine not configured".to_string())
    }

    /// Find text on screen using OCR and click it.
    #[allow(clippy::await_holding_lock)]
    pub async fn click_text(&self, target: &str) -> Result<ActionResult, String> {
        let screen = self.require_screen()?;
        let input = self.require_input()?;

        let frame = {
            let mut eng = screen.write();
            eng.capture_frame().await.map_err(screen_err)?
        };
        let ocr = {
            let eng = screen.read();
            eng.recognize_text(&frame).await.map_err(screen_err)?
        };

        let bridge = ScreenInputBridge::new(input);
        let result = bridge.click_ocr_text(&ocr, target).await.map_err(input_err)?;
        if result.success {
            Ok(ActionResult::success(format!("clicked '{}'", target)))
        } else {
            Err(format!("click failed: {}", result.detail))
        }
    }

    /// Find a text input element and type text into it.
    #[allow(clippy::await_holding_lock)]
    pub async fn type_text(&self, target: &str, text: &str) -> Result<ActionResult, String> {
        let screen = self.require_screen()?;
        let input = self.require_input()?;

        let frame = {
            let mut eng = screen.write();
            eng.capture_frame().await.map_err(screen_err)?
        };
        let grounding = {
            let eng = screen.read();
            let gq = GroundingQuery {
                query: target.to_string(),
                context: None,
                max_results: 1,
                confidence_threshold: 0.3,
            };
            eng.ground_element(&frame, &gq).await.map_err(screen_err)?
        };

        let bridge = ScreenInputBridge::new(input);
        let result = bridge.focus_and_type(&grounding.element, text).await
            .map_err(input_err)?;
        if result.success {
            Ok(ActionResult::success(format!("typed '{}' into '{}'", text, target)))
        } else {
            Err(format!("type failed: {}", result.detail))
        }
    }

    /// Open an application by searching for it via OCR or falling back to OpenApp action.
    pub async fn open_app(&self, name: &str) -> Result<ActionResult, String> {
        if self.screen.is_some() && self.input.is_some() {
            if let Ok(result) = self.click_text(name).await {
                return Ok(result);
            }
        }
        let fallback = DefaultActionExecutor;
        Ok(fallback.execute(&ActionType::OpenApp {
            app_id: name.to_string(),
            data: None,
        }))
    }

    /// Scroll in a direction until a target text is visible.
    #[allow(clippy::await_holding_lock)]
    pub async fn scroll_to(&self, target: &str, max_scrolls: u32) -> Result<ActionResult, String> {
        let screen = self.require_screen()?;
        let input = self.require_input()?;
        let bridge = ScreenInputBridge::new(input);

        for attempt in 0..max_scrolls {
            let frame = {
                let mut eng = screen.write();
                eng.capture_frame().await.map_err(screen_err)?
            };
            let ocr = {
                let eng = screen.read();
                eng.recognize_text(&frame).await.map_err(screen_err)?
            };

            if ocr.text.contains(target) {
                let result = bridge.click_ocr_text(&ocr, target).await
                    .map_err(input_err)?;
                if result.success {
                    return Ok(ActionResult::success(format!("found and clicked '{}' after scroll {attempt}", target)));
                }
            }

            if attempt + 1 < max_scrolls {
                bridge
                    .engine()
                    .execute(&nova_input::InputAction::Mouse(
                        nova_input::MouseAction::Scroll {
                            delta_x: 0,
                            delta_y: -3,
                        },
                    ))
                    .await
                    .map_err(input_err)?;
            }
        }

        Err(format!(
            "target '{}' not found after {max_scrolls} scroll attempts",
            target
        ))
    }

    /// Execute a multi-step UI navigation sequence.
    pub async fn navigate(&self, path: &[NavigationStep]) -> Result<ActionResult, String> {
        for (i, step) in path.iter().enumerate() {
            match step {
                NavigationStep::ClickText(text) => {
                    self.click_text(text).await.map_err(|e| {
                        format!("navigation step {i} (click '{text}') failed: {e}")
                    })?;
                }
                NavigationStep::TypeText {
                    target,
                    text,
                } => {
                    self.type_text(target, text).await.map_err(|e| {
                        format!("navigation step {i} (type into '{target}') failed: {e}")
                    })?;
                }
                NavigationStep::Wait(duration_ms) => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(*duration_ms)).await;
                }
            }
        }
        Ok(ActionResult::success(format!(
            "completed {} navigation step(s)",
            path.len()
        )))
    }
}

impl Default for ComputerController {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum NavigationStep {
    ClickText(String),
    TypeText { target: String, text: String },
    Wait(u64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_input::MockInputProvider;
    use nova_screen::ScreenPermissionManager;

    fn make_controller() -> ComputerController {
        let config = nova_screen::ScreenConfig::default();
        let perms = Arc::new(ScreenPermissionManager::new());
        if let Ok(engine) = ScreenEngine::new(config, perms) {
            let input: Arc<dyn InputEngine> = Arc::new(MockInputProvider::new());
            ComputerController::new()
                .with_screen(Arc::new(RwLock::new(engine)))
                .with_input(input)
        } else {
            ComputerController::new()
        }
    }

    #[tokio::test]
    async fn test_controller_missing_engines() {
        let ctrl = ComputerController::new();
        let result = ctrl.click_text("anything").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("screen engine not configured"));
    }

    #[tokio::test]
    async fn test_open_app_fallback() {
        let ctrl = ComputerController::new()
            .with_input(Arc::new(MockInputProvider::new()));
        let result = ctrl.open_app("Calculator").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_navigate_empty_path() {
        let ctrl = make_controller();
        let result = ctrl.navigate(&[]).await;
        assert!(result.is_ok());
        assert!(result.unwrap().success);
    }

    #[test]
    fn test_controller_default() {
        let _ctrl = ComputerController::default();
    }

    #[test]
    fn test_controller_set_engines() {
        let mut ctrl = ComputerController::new();
        let input: Arc<dyn InputEngine> = Arc::new(MockInputProvider::new());
        ctrl.set_input(input);
    }
}
