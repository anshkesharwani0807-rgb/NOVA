use serde::{Deserialize, Serialize};

pub const PERM_INPUT_MOUSE: &str = "input.mouse";
pub const PERM_INPUT_KEYBOARD: &str = "input.keyboard";
pub const PERM_INPUT_TOUCH: &str = "input.touch";
pub const PERM_INPUT_GESTURE: &str = "input.gesture";

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputCapability {
    MouseInput,
    KeyboardInput,
    TouchInput,
    GestureInput,
}

impl InputCapability {
    pub fn name(&self) -> &'static str {
        match self {
            InputCapability::MouseInput => "input_mouse",
            InputCapability::KeyboardInput => "input_keyboard",
            InputCapability::TouchInput => "input_touch",
            InputCapability::GestureInput => "input_gesture",
        }
    }

    pub fn required_permission(&self) -> &'static str {
        match self {
            InputCapability::MouseInput => PERM_INPUT_MOUSE,
            InputCapability::KeyboardInput => PERM_INPUT_KEYBOARD,
            InputCapability::TouchInput => PERM_INPUT_TOUCH,
            InputCapability::GestureInput => PERM_INPUT_GESTURE,
        }
    }
}
