use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Win,
    Meta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MouseAction {
    Click {
        point: Point,
        button: MouseButton,
        count: u32,
    },
    Move {
        point: Point,
        absolute: bool,
    },
    Drag {
        from: Point,
        to: Point,
        button: MouseButton,
    },
    Scroll {
        delta_x: i32,
        delta_y: i32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KeyboardAction {
    TypeText {
        text: String,
    },
    KeyPress {
        key: String,
        modifiers: Vec<Modifier>,
    },
    KeyRelease {
        key: String,
    },
    Hotkey {
        keys: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TouchAction {
    Tap {
        point: Point,
    },
    DoubleTap {
        point: Point,
    },
    LongPress {
        point: Point,
        duration_ms: u64,
    },
    Swipe {
        from: Point,
        to: Point,
        duration_ms: u64,
    },
    Pinch {
        center: Point,
        scale: f32,
        duration_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GestureAction {
    Scroll {
        delta_x: i32,
        delta_y: i32,
        smooth: bool,
    },
    Zoom {
        factor: f32,
    },
    ThreeFingerSwipe {
        direction: SwipeDirection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputAction {
    Mouse(MouseAction),
    Keyboard(KeyboardAction),
    Touch(TouchAction),
    Gesture(GestureAction),
    Wait {
        duration_ms: u64,
    },
}

impl InputAction {
    pub fn label(&self) -> &'static str {
        match self {
            InputAction::Mouse(a) => match a {
                MouseAction::Click { .. } => "input.mouse.click",
                MouseAction::Move { .. } => "input.mouse.move",
                MouseAction::Drag { .. } => "input.mouse.drag",
                MouseAction::Scroll { .. } => "input.mouse.scroll",
            },
            InputAction::Keyboard(a) => match a {
                KeyboardAction::TypeText { .. } => "input.keyboard.type",
                KeyboardAction::KeyPress { .. } => "input.keyboard.press",
                KeyboardAction::KeyRelease { .. } => "input.keyboard.release",
                KeyboardAction::Hotkey { .. } => "input.keyboard.hotkey",
            },
            InputAction::Touch(a) => match a {
                TouchAction::Tap { .. } => "input.touch.tap",
                TouchAction::DoubleTap { .. } => "input.touch.double_tap",
                TouchAction::LongPress { .. } => "input.touch.long_press",
                TouchAction::Swipe { .. } => "input.touch.swipe",
                TouchAction::Pinch { .. } => "input.touch.pinch",
            },
            InputAction::Gesture(a) => match a {
                GestureAction::Scroll { .. } => "input.gesture.scroll",
                GestureAction::Zoom { .. } => "input.gesture.zoom",
                GestureAction::ThreeFingerSwipe { .. } => "input.gesture.three_finger_swipe",
            },
            InputAction::Wait { .. } => "input.wait",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResult {
    pub success: bool,
    pub detail: String,
}

impl ActionResult {
    pub fn success(detail: impl Into<String>) -> Self {
        Self {
            success: true,
            detail: detail.into(),
        }
    }

    pub fn failure(detail: impl Into<String>) -> Self {
        Self {
            success: false,
            detail: detail.into(),
        }
    }
}
