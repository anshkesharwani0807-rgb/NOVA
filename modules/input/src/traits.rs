use async_trait::async_trait;

use crate::error::InputResult;
use crate::types::*;

#[async_trait]
pub trait InputEngine: Send + Sync {
    fn engine_name(&self) -> &'static str;

    async fn execute(&self, action: &InputAction) -> InputResult<ActionResult>;

    async fn execute_batch(&self, actions: &[InputAction]) -> Vec<InputResult<ActionResult>> {
        let mut results = Vec::with_capacity(actions.len());
        for action in actions {
            results.push(self.execute(action).await);
        }
        results
    }

    fn supported_actions(&self) -> Vec<&'static str> {
        vec![
            "input.mouse.click",
            "input.mouse.move",
            "input.mouse.drag",
            "input.mouse.scroll",
            "input.keyboard.type",
            "input.keyboard.press",
            "input.keyboard.release",
            "input.keyboard.hotkey",
            "input.touch.tap",
            "input.touch.double_tap",
            "input.touch.long_press",
            "input.touch.swipe",
            "input.touch.pinch",
            "input.gesture.scroll",
            "input.gesture.zoom",
            "input.gesture.three_finger_swipe",
        ]
    }

    fn is_action_supported(&self, action: &InputAction) -> bool {
        self.supported_actions().contains(&action.label())
    }
}
