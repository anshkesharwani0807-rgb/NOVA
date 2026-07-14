use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
