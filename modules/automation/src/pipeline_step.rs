use serde::{Deserialize, Serialize};

use crate::action::ActionType;
use crate::planner::ExecutionStep;

/// Status of a single pipeline step during execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineStepStatus {
    /// Step has not been started yet.
    Pending,
    /// Step is currently being executed.
    InProgress,
    /// Step completed successfully.
    Succeeded,
    /// Step failed with an error message.
    Failed(String),
    /// Step was skipped because its preconditions were already satisfied.
    Skipped(String),
}

/// A condition that, if true, allows a step to be skipped because the
/// desired state is already achieved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Precondition {
    /// Device field is already in the expected state.
    DeviceState { field: String, expected: String },
    /// Specified app is not running (skip open-app if already open).
    AppNotRunning(String),
    /// Specified app is running (skip close/activate if already shown).
    AppIsRunning(String),
    /// Expected text is already visible on screen.
    ScreenContains(String),
    /// Network is already in the desired connectivity state.
    NetworkState {
        online: Option<bool>,
        wifi: Option<bool>,
    },
    /// No precondition to check.
    NoPrecondition,
}

/// Strategy used after step execution to verify the step actually achieved
/// its intended outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationStrategy {
    /// Capture a new frame and diff against the pre-execution snapshot.
    /// Use when a screen interaction should change the display.
    CompareSnapshots,
    /// Capture a frame and run OCR, checking for expected text.
    OCRTextPresent { expected_text: String },
    /// Check whether a specific app is now in the foreground.
    AppInForeground { app_name: String },
    /// Query WorldState device telemetry and compare against expected value.
    DeviceTelemetryMatch { field: String, expected: String },
    /// No verification (fire-and-forget steps like Wait, Speak).
    NoVerification,
}

/// Policy controlling how a step is retried on failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryPolicy {
    /// Retry a fixed number of times with no delay between attempts.
    Fixed(u32),
    /// Retry with exponential backoff: delay = base_delay_ms * 2^attempt.
    ExponentialBackoff {
        max_retries: u32,
        base_delay_ms: u64,
    },
    /// Do not retry on failure.
    NoRetry,
}

/// What the system expects to observe after a step executes successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExpectedOutcome {
    /// The screen content is expected to change (navigation, dialog, etc.).
    ScreenChange { description: String },
    /// A device property (brightness, volume, wifi, etc.) changed.
    DeviceStateChange { field: String },
    /// An application moved to the foreground.
    AppForeground { app_name: String },
    /// Text was entered into a screen element.
    TextEntered { target: String, text: String },
    /// No observable change expected (informational steps).
    NoChange,
}

/// A pipeline-ready execution step with enriched metadata for the
/// closed-loop execution pipeline.
///
/// Wraps a planner [`ExecutionStep`] with:
/// - Execution status tracking
/// - Preconditions that may allow skipping the step
/// - Expected outcome for post-execution verification
/// - Retry policy for failure recovery
/// - Pre- and post-execution world snapshots (populated by the executor)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    /// The original planner step.
    pub step: ExecutionStep,
    /// Index within the pipeline (0-based).
    pub step_index: usize,
    /// Current execution status.
    pub status: PipelineStepStatus,
    /// Preconditions checked before execution.
    pub preconditions: Vec<Precondition>,
    /// Strategy for verifying this step succeeded.
    pub verification: VerificationStrategy,
    /// What outcome is expected after successful execution.
    pub expected_outcome: ExpectedOutcome,
    /// How to retry this step on failure.
    pub retry_policy: RetryPolicy,
}

impl PipelineStep {
    /// Create a new pipeline step wrapping an [`ExecutionStep`].
    pub fn new(
        step: ExecutionStep,
        step_index: usize,
        preconditions: Vec<Precondition>,
        verification: VerificationStrategy,
        expected_outcome: ExpectedOutcome,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self {
            step,
            step_index,
            status: PipelineStepStatus::Pending,
            preconditions,
            verification,
            expected_outcome,
            retry_policy,
        }
    }

    /// Short human-readable description combining the step index and action.
    pub fn description(&self) -> String {
        format!(
            "[{}] {}: {}",
            self.step_index, self.step.id, self.step.description
        )
    }

    /// Returns `true` if the step should be attempted (not already succeeded).
    pub fn is_pending(&self) -> bool {
        self.status == PipelineStepStatus::Pending
    }

    /// Returns `true` if the step completed (success, skip, or terminal failure).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            PipelineStepStatus::Succeeded
                | PipelineStepStatus::Skipped(_)
                | PipelineStepStatus::Failed(_)
        )
    }
}

/// Infer a [`VerificationStrategy`] from an [`ActionType`].
pub fn verification_strategy_for_action(action: &ActionType) -> VerificationStrategy {
    match action {
        ActionType::ClickScreenElement { query } | ActionType::ClickScreenText { text: query } => {
            VerificationStrategy::OCRTextPresent {
                expected_text: query.clone(),
            }
        }
        ActionType::TypeIntoScreenElement { text, .. } => VerificationStrategy::OCRTextPresent {
            expected_text: text.clone(),
        },
        ActionType::DragScreenElements { .. } | ActionType::SwipeScreenElements { .. } => {
            VerificationStrategy::CompareSnapshots
        }
        ActionType::OpenApp { app_id, .. }
        | ActionType::LaunchActivity {
            package: app_id, ..
        } => VerificationStrategy::AppInForeground {
            app_name: app_id.clone(),
        },
        ActionType::DeviceControl { control } => {
            let field = match control {
                crate::action::DeviceControl::SetBrightness(_) => "brightness",
                crate::action::DeviceControl::SetVolume(_) => "volume",
                crate::action::DeviceControl::ToggleWiFi(_) => "wifi",
                crate::action::DeviceControl::ToggleBluetooth(_) => "bluetooth",
                crate::action::DeviceControl::ToggleDND(_) => "dnd",
                crate::action::DeviceControl::LockScreen => "lock_state",
                crate::action::DeviceControl::PowerSave(_) => "power_save",
                crate::action::DeviceControl::SetProfile(_) => "profile",
            };
            VerificationStrategy::DeviceTelemetryMatch {
                field: field.to_string(),
                expected: String::new(),
            }
        }
        _ => VerificationStrategy::NoVerification,
    }
}

/// Infer an [`ExpectedOutcome`] from an [`ActionType`].
pub fn expected_outcome_for_action(action: &ActionType) -> ExpectedOutcome {
    match action {
        ActionType::ClickScreenElement { query } | ActionType::ClickScreenText { text: query } => {
            ExpectedOutcome::ScreenChange {
                description: format!("screen after clicking '{}'", query),
            }
        }
        ActionType::TypeIntoScreenElement { text, .. } => ExpectedOutcome::TextEntered {
            target: String::new(),
            text: text.clone(),
        },
        ActionType::DragScreenElements {
            from_query,
            to_query,
        } => ExpectedOutcome::ScreenChange {
            description: format!("screen after dragging '{}' to '{}'", from_query, to_query),
        },
        ActionType::SwipeScreenElements {
            from_query,
            to_query,
        } => ExpectedOutcome::ScreenChange {
            description: format!("screen after swiping '{}' to '{}'", from_query, to_query),
        },
        ActionType::OpenApp { app_id, .. }
        | ActionType::LaunchActivity {
            package: app_id, ..
        } => ExpectedOutcome::AppForeground {
            app_name: app_id.clone(),
        },
        ActionType::DeviceControl { control } => {
            let field = match control {
                crate::action::DeviceControl::SetBrightness(_) => "brightness",
                crate::action::DeviceControl::SetVolume(_) => "volume",
                crate::action::DeviceControl::ToggleWiFi(_) => "wifi",
                crate::action::DeviceControl::ToggleBluetooth(_) => "bluetooth",
                crate::action::DeviceControl::ToggleDND(_) => "dnd",
                crate::action::DeviceControl::LockScreen => "lock_state",
                crate::action::DeviceControl::PowerSave(_) => "power_save",
                crate::action::DeviceControl::SetProfile(_) => "profile",
            };
            ExpectedOutcome::DeviceStateChange {
                field: field.to_string(),
            }
        }
        _ => ExpectedOutcome::NoChange,
    }
}

/// Infer a [`RetryPolicy`] from an [`ExecutionStep`].
pub fn retry_policy_for_step(step: &ExecutionStep) -> RetryPolicy {
    if step.retry_count > 0 {
        RetryPolicy::ExponentialBackoff {
            max_retries: step.retry_count,
            base_delay_ms: 1_000,
        }
    } else {
        RetryPolicy::NoRetry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::DeviceControl;

    #[test]
    fn test_pipeline_step_new() {
        let step = ExecutionStep {
            id: "s1".to_string(),
            description: "click submit".to_string(),
            action: ActionType::ClickScreenElement {
                query: "submit".to_string(),
            },
            dependencies: vec![],
            required_capabilities: vec![],
            timeout_ms: 5000,
            retry_count: 2,
            continue_on_failure: false,
        };
        let ps = PipelineStep::new(
            step.clone(),
            0,
            vec![],
            VerificationStrategy::OCRTextPresent {
                expected_text: "submit".to_string(),
            },
            ExpectedOutcome::ScreenChange {
                description: "screen after click".to_string(),
            },
            RetryPolicy::ExponentialBackoff {
                max_retries: 2,
                base_delay_ms: 1000,
            },
        );
        assert_eq!(ps.step.id, "s1");
        assert_eq!(ps.step_index, 0);
        assert_eq!(ps.status, PipelineStepStatus::Pending);
        assert!(ps.is_pending());
        assert!(!ps.is_terminal());
    }

    #[test]
    fn test_pipeline_step_status_transitions() {
        let mut ps = PipelineStep {
            step: ExecutionStep {
                id: "s1".to_string(),
                description: "test".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
            step_index: 0,
            status: PipelineStepStatus::Pending,
            preconditions: vec![],
            verification: VerificationStrategy::NoVerification,
            expected_outcome: ExpectedOutcome::NoChange,
            retry_policy: RetryPolicy::NoRetry,
        };

        assert!(ps.is_pending());
        ps.status = PipelineStepStatus::InProgress;
        assert!(!ps.is_pending());
        assert!(!ps.is_terminal());

        ps.status = PipelineStepStatus::Succeeded;
        assert!(ps.is_terminal());

        ps.status = PipelineStepStatus::Failed("oops".into());
        assert!(ps.is_terminal());

        ps.status = PipelineStepStatus::Skipped("already done".into());
        assert!(ps.is_terminal());
    }

    #[test]
    fn test_verification_strategy_for_screen_actions() {
        let click = ActionType::ClickScreenElement {
            query: "save".to_string(),
        };
        match verification_strategy_for_action(&click) {
            VerificationStrategy::OCRTextPresent { expected_text } => {
                assert_eq!(expected_text, "save");
            }
            _ => panic!("expected OCRTextPresent"),
        }

        let type_action = ActionType::TypeIntoScreenElement {
            query: "field".to_string(),
            text: "hello".to_string(),
        };
        match verification_strategy_for_action(&type_action) {
            VerificationStrategy::OCRTextPresent { expected_text } => {
                assert_eq!(expected_text, "hello");
            }
            _ => panic!("expected OCRTextPresent"),
        }
    }

    #[test]
    fn test_verification_strategy_for_device_control() {
        let brightness = ActionType::DeviceControl {
            control: DeviceControl::SetBrightness(75),
        };
        match verification_strategy_for_action(&brightness) {
            VerificationStrategy::DeviceTelemetryMatch { field, .. } => {
                assert_eq!(field, "brightness");
            }
            _ => panic!("expected DeviceTelemetryMatch"),
        }

        let wifi = ActionType::DeviceControl {
            control: DeviceControl::ToggleWiFi(true),
        };
        match verification_strategy_for_action(&wifi) {
            VerificationStrategy::DeviceTelemetryMatch { field, .. } => {
                assert_eq!(field, "wifi");
            }
            _ => panic!("expected DeviceTelemetryMatch"),
        }
    }

    #[test]
    fn test_verification_strategy_fire_and_forget() {
        let speak = ActionType::Speak {
            text: "hello".to_string(),
        };
        assert!(matches!(
            verification_strategy_for_action(&speak),
            VerificationStrategy::NoVerification
        ));

        let notify = ActionType::Notify {
            title: "t".to_string(),
            body: "b".to_string(),
            priority: crate::action::NotifyPriority::Normal,
        };
        assert!(matches!(
            verification_strategy_for_action(&notify),
            VerificationStrategy::NoVerification
        ));
    }

    #[test]
    fn test_expected_outcome_for_screen_actions() {
        let click = ActionType::ClickScreenElement {
            query: "save".to_string(),
        };
        match expected_outcome_for_action(&click) {
            ExpectedOutcome::ScreenChange { description } => {
                assert!(description.contains("save"));
            }
            _ => panic!("expected ScreenChange"),
        }

        let type_action = ActionType::TypeIntoScreenElement {
            query: "field".to_string(),
            text: "hello".to_string(),
        };
        match expected_outcome_for_action(&type_action) {
            ExpectedOutcome::TextEntered { text, .. } => {
                assert_eq!(text, "hello");
            }
            _ => panic!("expected TextEntered"),
        }
    }

    #[test]
    fn test_expected_outcome_for_open_app() {
        let open = ActionType::OpenApp {
            app_id: "calculator".to_string(),
            data: None,
        };
        match expected_outcome_for_action(&open) {
            ExpectedOutcome::AppForeground { app_name } => {
                assert_eq!(app_name, "calculator");
            }
            _ => panic!("expected AppForeground"),
        }
    }

    #[test]
    fn test_retry_policy_from_retry_count() {
        let step_no_retry = ExecutionStep {
            id: "s1".to_string(),
            description: "no retry".to_string(),
            action: ActionType::Wait { duration_ms: 1 },
            dependencies: vec![],
            required_capabilities: vec![],
            timeout_ms: 1000,
            retry_count: 0,
            continue_on_failure: false,
        };
        assert!(matches!(
            retry_policy_for_step(&step_no_retry),
            RetryPolicy::NoRetry
        ));

        let step_with_retry = ExecutionStep {
            id: "s2".to_string(),
            description: "with retry".to_string(),
            action: ActionType::ClickScreenElement {
                query: "btn".to_string(),
            },
            dependencies: vec![],
            required_capabilities: vec![],
            timeout_ms: 5000,
            retry_count: 3,
            continue_on_failure: false,
        };
        match retry_policy_for_step(&step_with_retry) {
            RetryPolicy::ExponentialBackoff {
                max_retries,
                base_delay_ms,
            } => {
                assert_eq!(max_retries, 3);
                assert_eq!(base_delay_ms, 1000);
            }
            _ => panic!("expected ExponentialBackoff"),
        }
    }

    #[test]
    fn test_pipeline_step_description() {
        let step = ExecutionStep {
            id: "click_save".to_string(),
            description: "click save button".to_string(),
            action: ActionType::ClickScreenElement {
                query: "save".to_string(),
            },
            dependencies: vec![],
            required_capabilities: vec![],
            timeout_ms: 5000,
            retry_count: 2,
            continue_on_failure: false,
        };
        let ps = PipelineStep::new(
            step,
            0,
            vec![],
            VerificationStrategy::NoVerification,
            ExpectedOutcome::NoChange,
            RetryPolicy::NoRetry,
        );
        let desc = ps.description();
        assert!(desc.contains("click_save"));
        assert!(desc.contains("click save button"));
    }

    #[test]
    fn test_verification_strategy_for_drag_swipe() {
        let drag = ActionType::DragScreenElements {
            from_query: "slider".to_string(),
            to_query: "position".to_string(),
        };
        assert!(matches!(
            verification_strategy_for_action(&drag),
            VerificationStrategy::CompareSnapshots
        ));

        let swipe = ActionType::SwipeScreenElements {
            from_query: "left".to_string(),
            to_query: "right".to_string(),
        };
        assert!(matches!(
            verification_strategy_for_action(&swipe),
            VerificationStrategy::CompareSnapshots
        ));
    }

    #[test]
    fn test_pipeline_step_serialize_roundtrip() {
        let step = ExecutionStep {
            id: "s1".to_string(),
            description: "test".to_string(),
            action: ActionType::Wait { duration_ms: 100 },
            dependencies: vec![],
            required_capabilities: vec![],
            timeout_ms: 1000,
            retry_count: 0,
            continue_on_failure: false,
        };
        let ps = PipelineStep::new(
            step,
            0,
            vec![Precondition::NoPrecondition],
            VerificationStrategy::NoVerification,
            ExpectedOutcome::NoChange,
            RetryPolicy::NoRetry,
        );
        let json = serde_json::to_string(&ps).unwrap();
        let deserialized: PipelineStep = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.step_index, ps.step_index);
        assert_eq!(deserialized.status, ps.status);
    }
}
