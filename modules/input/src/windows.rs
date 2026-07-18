use async_trait::async_trait;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

use crate::error::{InputError, InputResult};
use crate::traits::InputEngine;
use crate::types::*;

pub struct WindowsInputProvider;

impl WindowsInputProvider {
    pub fn new() -> Self {
        Self
    }

    fn screen_size() -> (i32, i32) {
        unsafe {
            (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
        }
    }

    fn send(inputs: &[INPUT]) -> InputResult<u32> {
        let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent != inputs.len() as u32 {
            return Err(InputError::ProviderError(format!(
                "SendInput: expected {} INPUTs, got {}",
                inputs.len(),
                sent
            )));
        }
        Ok(sent)
    }

    fn normalize_coord(val: i32, max: i32) -> u32 {
        if max == 0 {
            return 0;
        }
        let v = val.max(0).min(max - 1);
        (v as f64 * 65535.0 / max as f64) as u32
    }

    fn mouse_input(dx: i32, dy: i32, mouse_data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: mouse_data,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn mouse_move_abs(x: i32, y: i32) -> INPUT {
        let (sw, sh) = Self::screen_size();
        let nx = Self::normalize_coord(x, sw);
        let ny = Self::normalize_coord(y, sh);
        Self::mouse_input(nx as i32, ny as i32, 0, MOUSE_EVENT_FLAGS(MOUSEEVENTF_MOVE.0 | MOUSEEVENTF_ABSOLUTE.0))
    }

    fn mouse_move_rel(dx: i32, dy: i32) -> INPUT {
        Self::mouse_input(dx, dy, 0, MOUSEEVENTF_MOVE)
    }

    fn mouse_down(button: &MouseButton) -> INPUT {
        let (flags, _data) = match button {
            MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, 0),
            MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, 0),
            MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, 0),
        };
        Self::mouse_input(0, 0, 0, flags)
    }

    fn mouse_up(button: &MouseButton) -> INPUT {
        let (flags, _data) = match button {
            MouseButton::Left => (MOUSEEVENTF_LEFTUP, 0),
            MouseButton::Right => (MOUSEEVENTF_RIGHTUP, 0),
            MouseButton::Middle => (MOUSEEVENTF_MIDDLEUP, 0),
        };
        Self::mouse_input(0, 0, 0, flags)
    }

    fn mouse_wheel(delta: i32) -> INPUT {
        Self::mouse_input(0, 0, delta as u32, MOUSEEVENTF_WHEEL)
    }

    fn mouse_hwheel(delta: i32) -> INPUT {
        Self::mouse_input(0, 0, delta as u32, MOUSEEVENTF_HWHEEL)
    }

    fn key_input(wvk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(wvk),
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn key_down_vk(vk: u16) -> INPUT {
        Self::key_input(vk, 0, KEYBD_EVENT_FLAGS(0))
    }

    fn key_up_vk(vk: u16) -> INPUT {
        Self::key_input(vk, 0, KEYEVENTF_KEYUP)
    }

    fn unicode_keydown(ch: u16) -> INPUT {
        Self::key_input(0, ch, KEYEVENTF_UNICODE)
    }

    fn unicode_keyup(ch: u16) -> INPUT {
        Self::key_input(0, ch, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)
    }

    fn key_to_vk(key: &str) -> Option<u16> {
        let k = key.to_lowercase();
        match k.as_str() {
            _ if k.len() == 1 && k.as_bytes()[0] >= b'a' && k.as_bytes()[0] <= b'z' => {
                Some(k.as_bytes()[0] as u16 - 0x20) // VK_A..VK_Z = 0x41..0x5A
            }
            "0" => Some(0x30),
            "1" => Some(0x31),
            "2" => Some(0x32),
            "3" => Some(0x33),
            "4" => Some(0x34),
            "5" => Some(0x35),
            "6" => Some(0x36),
            "7" => Some(0x37),
            "8" => Some(0x38),
            "9" => Some(0x39),
            "enter" => Some(0x0D),
            "return" => Some(0x0D),
            "tab" => Some(0x09),
            "space" => Some(0x20),
            "backspace" => Some(0x08),
            "escape" | "esc" => Some(0x1B),
            "delete" | "del" => Some(0x2E),
            "home" => Some(0x24),
            "end" => Some(0x23),
            "pageup" | "pgup" => Some(0x21),
            "pagedown" | "pgdn" => Some(0x22),
            "up" => Some(0x26),
            "down" => Some(0x28),
            "left" => Some(0x25),
            "right" => Some(0x27),
            "insert" | "ins" => Some(0x2D),
            "capslock" => Some(0x14),
            "printscreen" | "prtsc" => Some(0x2C),
            "scrolllock" => Some(0x91),
            "pause" | "break" => Some(0x13),
            "f1" => Some(0x70),
            "f2" => Some(0x71),
            "f3" => Some(0x72),
            "f4" => Some(0x73),
            "f5" => Some(0x74),
            "f6" => Some(0x75),
            "f7" => Some(0x76),
            "f8" => Some(0x77),
            "f9" => Some(0x78),
            "f10" => Some(0x79),
            "f11" => Some(0x7A),
            "f12" => Some(0x7B),
            "f13" => Some(0x7C),
            "f14" => Some(0x7D),
            "f15" => Some(0x7E),
            "f16" => Some(0x7F),
            "f17" => Some(0x80),
            "f18" => Some(0x81),
            "f19" => Some(0x82),
            "f20" => Some(0x83),
            "f21" => Some(0x84),
            "f22" => Some(0x85),
            "f23" => Some(0x86),
            "f24" => Some(0x87),
            "numlock" => Some(0x90),
            "numpad0" => Some(0x60),
            "numpad1" => Some(0x61),
            "numpad2" => Some(0x62),
            "numpad3" => Some(0x63),
            "numpad4" => Some(0x64),
            "numpad5" => Some(0x65),
            "numpad6" => Some(0x66),
            "numpad7" => Some(0x67),
            "numpad8" => Some(0x68),
            "numpad9" => Some(0x69),
            "multiply" | "numpad*" => Some(0x6A),
            "add" | "numpad+" => Some(0x6B),
            "separator" => Some(0x6C),
            "subtract" | "numpad-" => Some(0x6D),
            "decimal" | "numpad." => Some(0x6E),
            "divide" | "numpad/" => Some(0x6F),
            "oem_1" | ";" | ":" => Some(0xBA),
            "oem_plus" | "+" | "=" => Some(0xBB),
            "oem_comma" | "," | "<" => Some(0xBC),
            "oem_minus" | "-" | "_" => Some(0xBD),
            "oem_period" | "." | ">" => Some(0xBE),
            "oem_2" | "/" | "?" => Some(0xBF),
            "oem_3" | "`" | "~" => Some(0xC0),
            "oem_4" | "[" | "{" => Some(0xDB),
            "oem_5" | "\\" | "|" => Some(0xDC),
            "oem_6" | "]" | "}" => Some(0xDD),
            "oem_7" | "'" | "\"" => Some(0xDE),
            _ => None,
        }
    }

    fn modifier_to_vk(modifier: &Modifier) -> u16 {
        match modifier {
            Modifier::Ctrl => 0x11,
            Modifier::Alt => 0x12,
            Modifier::Shift => 0x10,
            Modifier::Win => 0x5B,
            Modifier::Meta => 0x5B,
        }
    }

    fn press_modifier(modifier: &Modifier) -> INPUT {
        Self::key_down_vk(Self::modifier_to_vk(modifier))
    }

    fn release_modifier(modifier: &Modifier) -> INPUT {
        Self::key_up_vk(Self::modifier_to_vk(modifier))
    }
}

impl Default for WindowsInputProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InputEngine for WindowsInputProvider {
    fn engine_name(&self) -> &'static str {
        "windows-sendinput"
    }

    async fn execute(&self, action: &InputAction) -> InputResult<ActionResult> {
        match action {
            InputAction::Mouse(MouseAction::Click { point, button, count }) => {
                let mut inputs = Vec::new();
                inputs.push(Self::mouse_move_abs(point.x, point.y));
                for _ in 0..*count {
                    inputs.push(Self::mouse_down(button));
                    inputs.push(Self::mouse_up(button));
                }
                Self::send(&inputs)?;
                Ok(ActionResult::success(format!(
                    "{}x{} click at ({}, {})",
                    count, button_name(button), point.x, point.y
                )))
            }

            InputAction::Mouse(MouseAction::Move { point, absolute }) => {
                let input = if *absolute {
                    Self::mouse_move_abs(point.x, point.y)
                } else {
                    Self::mouse_move_rel(point.x, point.y)
                };
                Self::send(&[input])?;
                Ok(ActionResult::success(format!(
                    "mouse moved to ({}, {})",
                    point.x, point.y
                )))
            }

            InputAction::Mouse(MouseAction::Drag { from, to, button }) => {
                let inputs = vec![
                    Self::mouse_move_abs(from.x, from.y),
                    Self::mouse_down(button),
                    Self::mouse_move_abs(to.x, to.y),
                    Self::mouse_up(button),
                ];
                Self::send(&inputs)?;
                Ok(ActionResult::success(format!(
                    "drag from ({}, {}) to ({}, {})",
                    from.x, from.y, to.x, to.y
                )))
            }

            InputAction::Mouse(MouseAction::Scroll { delta_x, delta_y }) => {
                let mut inputs = Vec::new();
                if *delta_y != 0 {
                    inputs.push(Self::mouse_wheel(*delta_y));
                }
                if *delta_x != 0 {
                    inputs.push(Self::mouse_hwheel(*delta_x));
                }
                if inputs.is_empty() {
                    return Ok(ActionResult::success("no scroll delta".to_string()));
                }
                Self::send(&inputs)?;
                Ok(ActionResult::success(format!(
                    "scroll dx={} dy={}",
                    delta_x, delta_y
                )))
            }

            InputAction::Keyboard(KeyboardAction::TypeText { text }) => {
                type_unicode_text(text)?;
                Ok(ActionResult::success(format!(
                    "typed {} characters: '{}'",
                    text.len(),
                    truncate(text, 40)
                )))
            }

            InputAction::Keyboard(KeyboardAction::KeyPress { key, modifiers }) => {
                let vk = Self::key_to_vk(key).ok_or_else(|| {
                    InputError::UnsupportedAction(format!("unknown key: {}", key))
                })?;
                let mut inputs = Vec::new();
                for m in modifiers {
                    inputs.push(Self::press_modifier(m));
                }
                inputs.push(Self::key_down_vk(vk));
                inputs.push(Self::key_up_vk(vk));
                for m in modifiers.iter().rev() {
                    inputs.push(Self::release_modifier(m));
                }
                Self::send(&inputs)?;
                Ok(ActionResult::success(format!("key press: {}", key)))
            }

            InputAction::Keyboard(KeyboardAction::KeyRelease { key }) => {
                let vk = Self::key_to_vk(key).ok_or_else(|| {
                    InputError::UnsupportedAction(format!("unknown key: {}", key))
                })?;
                let input = Self::key_up_vk(vk);
                Self::send(&[input])?;
                Ok(ActionResult::success(format!("key release: {}", key)))
            }

            InputAction::Keyboard(KeyboardAction::Hotkey { keys }) => {
                if keys.is_empty() {
                    return Ok(ActionResult::success("no keys in hotkey".to_string()));
                }
                let mut inputs = Vec::new();
                for k in keys {
                    if let Some(vk) = Self::key_to_vk(k) {
                        inputs.push(Self::key_down_vk(vk));
                    }
                }
                for k in keys.iter().rev() {
                    if let Some(vk) = Self::key_to_vk(k) {
                        inputs.push(Self::key_up_vk(vk));
                    }
                }
                Self::send(&inputs)?;
                Ok(ActionResult::success(format!("hotkey: {}", keys.join("+"))))
            }

            _ => Err(InputError::UnsupportedAction(action.label().to_string())),
        }
    }
}

fn button_name(button: &MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

/// Type Unicode text character by character using KEYEVENTF_UNICODE.
fn type_unicode_text(text: &str) -> InputResult<()> {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(text.len() * 2);
    for ch in text.chars() {
        let mut buf = [0u16; 2];
        for unit in ch.encode_utf16(&mut buf) {
            let code = *unit;
            inputs.push(WindowsInputProvider::unicode_keydown(code));
            inputs.push(WindowsInputProvider::unicode_keyup(code));
        }
    }
    WindowsInputProvider::send(&inputs).map(|_| ())
}
