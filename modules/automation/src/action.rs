use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parameters for injecting an input action through the InputEngine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputInjectionParams {
    /// Action type string — one of:
    ///   click, double_click, right_click, type, key_press, key_release,
    ///   hotkey, scroll, tap, double_tap, long_press, swipe, pinch,
    ///   move, drag, back, home, recents, wait
    pub action_type: String,
    /// Key-value parameters for the action.
    /// Common keys: x, y, button, text, key, keys, duration_ms,
    ///   from_x, from_y, to_x, to_y, delta_x, delta_y, count, scale
    pub params: HashMap<String, String>,
}

impl InputInjectionParams {
    pub fn get_i32(&self, key: &str, default: i32) -> i32 {
        self.params
            .get(key)
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(default)
    }

    pub fn get_u64(&self, key: &str, default: u64) -> u64 {
        self.params
            .get(key)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(default)
    }

    pub fn get_f32(&self, key: &str, default: f32) -> f32 {
        self.params
            .get(key)
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(default)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionType {
    Speak {
        text: String,
    },
    Notify {
        title: String,
        body: String,
        priority: NotifyPriority,
    },
    OpenApp {
        app_id: String,
        data: Option<String>,
    },
    LaunchActivity {
        package: String,
        activity: String,
        data: Option<String>,
    },
    Clipboard {
        action: ClipboardAction,
        text: Option<String>,
    },
    CreateMemory {
        title: String,
        content: String,
        category: String,
        tags: Vec<String>,
        importance: i32,
    },
    SearchMemory {
        query: String,
        max_results: usize,
    },
    RunAI {
        prompt: String,
        session_id: Option<String>,
    },
    CaptureVoice {
        duration_secs: Option<u64>,
    },
    AnalyzeImage {
        image_path: String,
        analysis_type: String,
    },
    DeviceControl {
        control: DeviceControl,
    },
    PluginInvocation {
        plugin_id: String,
        method: String,
        parameters: HashMap<String, String>,
    },
    Wait {
        duration_ms: u64,
    },
    SubWorkflow {
        workflow_id: String,
    },
    InputInjection(InputInjectionParams),
    ClickScreenElement {
        query: String,
    },
    TypeIntoScreenElement {
        query: String,
        text: String,
    },
    ClickScreenText {
        text: String,
    },
    DragScreenElements {
        from_query: String,
        to_query: String,
    },
    SwipeScreenElements {
        from_query: String,
        to_query: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NotifyPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClipboardAction {
    Copy,
    Paste,
    Clear,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceControl {
    SetBrightness(u32),
    SetVolume(u32),
    ToggleWiFi(bool),
    ToggleBluetooth(bool),
    ToggleDND(bool),
    LockScreen,
    PowerSave(bool),
    SetProfile(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub success: bool,
    pub message: String,
    pub data: Option<String>,
}

impl ActionResult {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: None,
        }
    }

    pub fn success_with_data(message: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: Some(data.into()),
        }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}

pub trait ActionExecutor: Send + Sync {
    fn execute(&self, action: &ActionType) -> ActionResult;
    fn kind(&self) -> &'static str;
}

pub struct DefaultActionExecutor;

impl ActionExecutor for DefaultActionExecutor {
    fn execute(&self, action: &ActionType) -> ActionResult {
        match action {
            ActionType::Speak { text } => ActionResult::success(format!("speak: {}", text)),
            ActionType::Notify { title, body, .. } => {
                ActionResult::success(format!("notify: {} - {}", title, body))
            }
            ActionType::OpenApp { app_id, .. } => {
                ActionResult::success(format!("open app: {}", app_id))
            }
            ActionType::LaunchActivity {
                package, activity, ..
            } => ActionResult::success(format!("launch {}/{}", package, activity)),
            ActionType::Clipboard { action, .. } => {
                ActionResult::success(format!("clipboard: {:?}", action))
            }
            ActionType::CreateMemory {
                title,
                content: _,
                category,
                ..
            } => ActionResult::success(format!("memory created: '{}' in {}", title, category)),
            ActionType::SearchMemory { query, max_results } => {
                ActionResult::success(format!("search '{}' (max {})", query, max_results))
            }
            ActionType::RunAI { prompt, .. } => {
                ActionResult::success(format!("ai inference: {}", &prompt[..prompt.len().min(50)]))
            }
            ActionType::CaptureVoice { duration_secs } => {
                let d = duration_secs.map_or("unlimited".to_string(), |s| format!("{}s", s));
                ActionResult::success(format!("voice capture: {}", d))
            }
            ActionType::AnalyzeImage {
                image_path,
                analysis_type,
            } => ActionResult::success(format!("analyze {}: {}", analysis_type, image_path)),
            ActionType::DeviceControl { control } => {
                ActionResult::success(format!("device control: {:?}", control))
            }
            ActionType::PluginInvocation {
                plugin_id, method, ..
            } => ActionResult::success(format!("plugin {}.{}", plugin_id, method)),
            ActionType::Wait { duration_ms } => {
                std::thread::sleep(std::time::Duration::from_millis(*duration_ms));
                ActionResult::success(format!("waited {}ms", duration_ms))
            }
            ActionType::SubWorkflow { workflow_id } => {
                ActionResult::success(format!("sub-workflow: {}", workflow_id))
            }
            ActionType::InputInjection(params) => ActionResult::failure(format!(
                "input injection requires InputEngine: {} with {} params",
                params.action_type,
                params.params.len()
            )),
            ActionType::ClickScreenElement { query } => ActionResult::failure(format!(
                "click screen '{}' requires ScreenAwareExecutor",
                query
            )),
            ActionType::TypeIntoScreenElement { query, .. } => ActionResult::failure(format!(
                "type into screen element '{}' requires ScreenAwareExecutor",
                query
            )),
            ActionType::ClickScreenText { text } => ActionResult::failure(format!(
                "click screen text '{}' requires ScreenAwareExecutor",
                text
            )),
            ActionType::DragScreenElements {
                from_query,
                to_query,
            } => ActionResult::failure(format!(
                "drag '{}' -> '{}' requires ScreenAwareExecutor",
                from_query, to_query
            )),
            ActionType::SwipeScreenElements {
                from_query,
                to_query,
            } => ActionResult::failure(format!(
                "swipe '{}' -> '{}' requires ScreenAwareExecutor",
                from_query, to_query
            )),
        }
    }

    fn kind(&self) -> &'static str {
        "default"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_default_executor_all_types() {
        let executor = DefaultActionExecutor;
        let actions = vec![
            ActionType::Speak {
                text: "hello".into(),
            },
            ActionType::Notify {
                title: "Test".into(),
                body: "body".into(),
                priority: NotifyPriority::Normal,
            },
            ActionType::OpenApp {
                app_id: "calc".into(),
                data: None,
            },
            ActionType::LaunchActivity {
                package: "com.test".into(),
                activity: ".Main".into(),
                data: None,
            },
            ActionType::Clipboard {
                action: ClipboardAction::Copy,
                text: Some("data".into()),
            },
            ActionType::CreateMemory {
                title: "t".into(),
                content: "c".into(),
                category: "general".into(),
                tags: vec![],
                importance: 5,
            },
            ActionType::SearchMemory {
                query: "q".into(),
                max_results: 5,
            },
            ActionType::RunAI {
                prompt: "p".into(),
                session_id: None,
            },
            ActionType::CaptureVoice {
                duration_secs: Some(10),
            },
            ActionType::AnalyzeImage {
                image_path: "/img.jpg".into(),
                analysis_type: "general".into(),
            },
            ActionType::DeviceControl {
                control: DeviceControl::SetBrightness(50),
            },
            ActionType::PluginInvocation {
                plugin_id: "p".into(),
                method: "exec".into(),
                parameters: HashMap::new(),
            },
            ActionType::Wait { duration_ms: 1 },
            ActionType::SubWorkflow {
                workflow_id: "sub1".into(),
            },
        ];
        for action in &actions {
            let result = executor.execute(action);
            assert!(result.success, "Action {:?} should succeed", action);
        }
    }

    #[test]
    fn test_action_result_success() {
        let r = ActionResult::success("ok");
        assert!(r.success);
        assert_eq!(r.message, "ok");
    }

    #[test]
    fn test_action_result_failure() {
        let r = ActionResult::failure("fail");
        assert!(!r.success);
        assert_eq!(r.message, "fail");
    }
}
