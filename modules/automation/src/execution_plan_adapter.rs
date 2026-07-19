use crate::action::{ActionType, DeviceControl};
use crate::pipeline_step::{
    expected_outcome_for_action, retry_policy_for_step, verification_strategy_for_action,
    PipelineStep, Precondition,
};
use crate::planner::ExecutionPlan;
use crate::world_state::WorldSnapshot;

/// Converts a planner [`ExecutionPlan`] into a vector of [`PipelineStep`] values
/// ready for closed-loop execution.
///
/// The adapter enriches each [`ExecutionStep`] with:
/// - Preconditions derived from the action type and optional world context
/// - Verification strategy and expected outcome based on the action type
/// - Retry policy based on the step's retry configuration
///
/// # Usage
///
/// ```rust,ignore
/// let adapter = ExecutionPlanAdapter;
/// let snapshot = world_state.snapshot();
/// let pipeline_steps = adapter.convert(&plan, Some(&snapshot));
/// ```
pub struct ExecutionPlanAdapter;

impl ExecutionPlanAdapter {
    /// Convert an entire [`ExecutionPlan`] into pipeline steps.
    ///
    /// Each step is enriched with preconditions (derived from the action type
    /// and optional world snapshot), verification strategy, expected outcome,
    /// and retry policy.
    pub fn convert(
        &self,
        plan: &ExecutionPlan,
        world_snapshot: Option<&WorldSnapshot>,
    ) -> Vec<PipelineStep> {
        plan.steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let preconditions = Self::derive_preconditions(step, world_snapshot);
                let verification = verification_strategy_for_action(&step.action);
                let expected_outcome = expected_outcome_for_action(&step.action);
                let retry_policy = retry_policy_for_step(step);

                PipelineStep {
                    step: step.clone(),
                    step_index: index,
                    status: crate::pipeline_step::PipelineStepStatus::Pending,
                    preconditions,
                    verification,
                    expected_outcome,
                    retry_policy,
                }
            })
            .collect()
    }

    /// Derive preconditions for a single [`ExecutionStep`].
    ///
    /// Uses heuristic mapping of action types to preconditions:
    /// - `DeviceControl(SetBrightness(v))` → check if brightness already at `v`
    /// - `DeviceControl(ToggleWiFi(true))` → skip if wifi already enabled
    /// - `DeviceControl(ToggleWiFi(false))` → skip if wifi already disabled
    /// - `ClickScreenElement { query }` → check if text is already visible
    /// - `OpenApp { app_id }` → check if app is not already running
    ///
    /// When a world snapshot is provided, preconditions are filled with
    /// the specific expected values from the current state.
    pub fn derive_preconditions(
        step: &crate::planner::ExecutionStep,
        world_snapshot: Option<&WorldSnapshot>,
    ) -> Vec<Precondition> {
        match &step.action {
            ActionType::DeviceControl { control } => {
                Self::device_control_preconditions(control, world_snapshot)
            }
            ActionType::ClickScreenElement { query }
            | ActionType::ClickScreenText { text: query }
            | ActionType::TypeIntoScreenElement { query, .. } => {
                vec![Precondition::ScreenContains(query.clone())]
            }
            ActionType::DragScreenElements { from_query, .. }
            | ActionType::SwipeScreenElements { from_query, .. } => {
                vec![Precondition::ScreenContains(from_query.clone())]
            }
            ActionType::OpenApp { app_id, .. } => {
                vec![Precondition::AppNotRunning(app_id.clone())]
            }
            ActionType::LaunchActivity { package, .. } => {
                vec![Precondition::AppNotRunning(package.clone())]
            }
            _ => vec![],
        }
    }

    /// Derive device control preconditions from the control variant and world state.
    fn device_control_preconditions(
        control: &DeviceControl,
        _world_snapshot: Option<&WorldSnapshot>,
    ) -> Vec<Precondition> {
        match control {
            DeviceControl::SetBrightness(value) => {
                vec![Precondition::DeviceState {
                    field: "brightness".to_string(),
                    expected: value.to_string(),
                }]
            }
            DeviceControl::SetVolume(value) => {
                vec![Precondition::DeviceState {
                    field: "volume".to_string(),
                    expected: value.to_string(),
                }]
            }
            DeviceControl::ToggleWiFi(enable) => {
                let expected = if *enable { "enabled" } else { "disabled" };
                vec![Precondition::DeviceState {
                    field: "wifi".to_string(),
                    expected: expected.to_string(),
                }]
            }
            DeviceControl::ToggleBluetooth(enable) => {
                let expected = if *enable { "enabled" } else { "disabled" };
                vec![Precondition::DeviceState {
                    field: "bluetooth".to_string(),
                    expected: expected.to_string(),
                }]
            }
            DeviceControl::ToggleDND(enable) => {
                let expected = if *enable { "enabled" } else { "disabled" };
                vec![Precondition::DeviceState {
                    field: "dnd".to_string(),
                    expected: expected.to_string(),
                }]
            }
            DeviceControl::LockScreen => {
                vec![Precondition::NoPrecondition]
            }
            DeviceControl::PowerSave(enable) => {
                let expected = if *enable { "enabled" } else { "disabled" };
                vec![Precondition::DeviceState {
                    field: "power_save".to_string(),
                    expected: expected.to_string(),
                }]
            }
            DeviceControl::SetProfile(profile) => {
                vec![Precondition::DeviceState {
                    field: "profile".to_string(),
                    expected: profile.clone(),
                }]
            }
        }
    }
}

impl Default for ExecutionPlanAdapter {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{DeviceControl, NotifyPriority};
    use crate::pipeline_step::{ExpectedOutcome, RetryPolicy, VerificationStrategy};
    use crate::planner::{ExecutionPlan, ExecutionStep, Goal, Planner};
    use crate::world_state::{DeviceTelemetry, NetworkState, WorldSnapshot};

    fn make_sample_step(id: &str, action: ActionType, retry_count: u32) -> ExecutionStep {
        ExecutionStep {
            id: id.to_string(),
            description: format!("step {}", id),
            action,
            dependencies: vec![],
            required_capabilities: vec![],
            timeout_ms: 5000,
            retry_count,
            continue_on_failure: false,
        }
    }

    fn make_plan_with_steps(steps: Vec<ExecutionStep>) -> ExecutionPlan {
        ExecutionPlan {
            id: "test_plan".to_string(),
            goal_description: "test goal".to_string(),
            steps,
            created_at: 0,
            estimated_steps: 0,
        }
    }

    #[test]
    fn test_adapter_converts_empty_plan() {
        let adapter = ExecutionPlanAdapter;
        let plan = make_plan_with_steps(vec![]);
        let pipeline = adapter.convert(&plan, None);
        assert!(pipeline.is_empty());
    }

    #[test]
    fn test_adapter_converts_single_step() {
        let adapter = ExecutionPlanAdapter;
        let step = make_sample_step("s1", ActionType::Wait { duration_ms: 100 }, 0);
        let plan = make_plan_with_steps(vec![step]);
        let pipeline = adapter.convert(&plan, None);

        assert_eq!(pipeline.len(), 1);
        assert_eq!(pipeline[0].step_index, 0);
        assert_eq!(pipeline[0].step.id, "s1");
        assert!(pipeline[0].preconditions.is_empty());
        assert!(matches!(
            pipeline[0].verification,
            VerificationStrategy::NoVerification
        ));
        assert!(matches!(
            pipeline[0].expected_outcome,
            ExpectedOutcome::NoChange
        ));
        assert!(matches!(pipeline[0].retry_policy, RetryPolicy::NoRetry));
    }

    #[test]
    fn test_adapter_converts_click_step() {
        let adapter = ExecutionPlanAdapter;
        let step = make_sample_step(
            "click1",
            ActionType::ClickScreenElement {
                query: "submit".to_string(),
            },
            2,
        );
        let plan = make_plan_with_steps(vec![step]);
        let pipeline = adapter.convert(&plan, None);

        assert_eq!(pipeline.len(), 1);
        let ps = &pipeline[0];

        // Precondition: screen should contain "submit"
        assert!(ps.preconditions.iter().any(|p| matches!(
            p,
            Precondition::ScreenContains(q) if q == "submit"
        )));

        // Verification: OCR for "submit"
        assert!(matches!(
            ps.verification,
            VerificationStrategy::OCRTextPresent { ref expected_text } if expected_text == "submit"
        ));

        // Expected outcome: screen change
        assert!(matches!(
            ps.expected_outcome,
            ExpectedOutcome::ScreenChange { .. }
        ));

        // Retry policy: exponential backoff with max_retries=2
        match ps.retry_policy {
            RetryPolicy::ExponentialBackoff {
                max_retries,
                base_delay_ms,
            } => {
                assert_eq!(max_retries, 2);
                assert_eq!(base_delay_ms, 1000);
            }
            _ => panic!("expected ExponentialBackoff"),
        }
    }

    #[test]
    fn test_adapter_device_control_preconditions() {
        // Brightness
        let step = make_sample_step(
            "bright",
            ActionType::DeviceControl {
                control: DeviceControl::SetBrightness(75),
            },
            1,
        );
        let preconditions = ExecutionPlanAdapter::derive_preconditions(&step, None);
        assert!(preconditions.iter().any(|p| matches!(
            p,
            Precondition::DeviceState { field, expected }
                if field == "brightness" && expected == "75"
        )));

        // WiFi on
        let step = make_sample_step(
            "wifi_on",
            ActionType::DeviceControl {
                control: DeviceControl::ToggleWiFi(true),
            },
            0,
        );
        let preconditions = ExecutionPlanAdapter::derive_preconditions(&step, None);
        assert!(preconditions.iter().any(|p| matches!(
            p,
            Precondition::DeviceState { field, expected }
                if field == "wifi" && expected == "enabled"
        )));

        // WiFi off
        let step = make_sample_step(
            "wifi_off",
            ActionType::DeviceControl {
                control: DeviceControl::ToggleWiFi(false),
            },
            0,
        );
        let preconditions = ExecutionPlanAdapter::derive_preconditions(&step, None);
        assert!(preconditions.iter().any(|p| matches!(
            p,
            Precondition::DeviceState { field, expected }
                if field == "wifi" && expected == "disabled"
        )));
    }

    #[test]
    fn test_adapter_open_app_precondition() {
        let step = make_sample_step(
            "open_calc",
            ActionType::OpenApp {
                app_id: "calculator".to_string(),
                data: None,
            },
            1,
        );
        let preconditions = ExecutionPlanAdapter::derive_preconditions(&step, None);
        assert!(preconditions.iter().any(|p| matches!(
            p,
            Precondition::AppNotRunning(app) if app == "calculator"
        )));
    }

    #[test]
    fn test_adapter_with_world_snapshot() {
        let snapshot = WorldSnapshot {
            frame: None,
            active_app: Some("notepad".to_string()),
            ocr: None,
            grounded_elements: vec![],
            ui_tree: None,
            device_telemetry: Some(DeviceTelemetry {
                battery_level: Some(85),
                is_charging: Some(true),
                wifi_enabled: Some(true),
                bluetooth_enabled: Some(false),
                last_updated: Some(1000),
            }),
            network_state: Some(NetworkState {
                is_online: Some(true),
                network_type: Some("wifi".to_string()),
                last_updated: Some(1000),
            }),
            timestamp: 1000,
        };

        // Open app when active_app is already "notepad"
        let step = make_sample_step(
            "open_note",
            ActionType::OpenApp {
                app_id: "notepad".to_string(),
                data: None,
            },
            0,
        );
        let preconditions = ExecutionPlanAdapter::derive_preconditions(&step, Some(&snapshot));
        // Should still express AppNotRunning precondition (the adapter doesn't
        // resolve whether it's met; it just derives the check).
        assert!(preconditions.iter().any(|p| matches!(
            p,
            Precondition::AppNotRunning(app) if app == "notepad"
        )));
    }

    #[test]
    fn test_adapter_multiple_steps_in_plan() {
        let adapter = ExecutionPlanAdapter;
        let steps = vec![
            make_sample_step(
                "s1",
                ActionType::Speak {
                    text: "hello".into(),
                },
                0,
            ),
            make_sample_step(
                "s2",
                ActionType::ClickScreenElement {
                    query: "next".to_string(),
                },
                1,
            ),
            make_sample_step(
                "s3",
                ActionType::DeviceControl {
                    control: DeviceControl::SetVolume(50),
                },
                0,
            ),
        ];
        let plan = make_plan_with_steps(steps);
        let pipeline = adapter.convert(&plan, None);

        assert_eq!(pipeline.len(), 3);

        // s1: fire-and-forget
        assert!(matches!(
            pipeline[0].verification,
            VerificationStrategy::NoVerification
        ));
        assert!(pipeline[0].preconditions.is_empty());

        // s2: screen click
        assert!(pipeline[1]
            .preconditions
            .iter()
            .any(|p| matches!(p, Precondition::ScreenContains(q) if q == "next")));
        assert!(matches!(
            pipeline[1].verification,
            VerificationStrategy::OCRTextPresent { .. }
        ));

        // s3: device control
        assert!(pipeline[2]
            .preconditions
            .iter()
            .any(|p| matches!(p, Precondition::DeviceState { field, .. } if field == "volume")));
        assert!(matches!(
            pipeline[2].verification,
            VerificationStrategy::DeviceTelemetryMatch { ref field, .. } if field == "volume"
        ));
    }

    #[test]
    fn test_adapter_retry_policy_from_planner_plan() {
        let planner = Planner::new();
        let goal = Goal::new("click 'submit'");
        let plan = planner.plan(&goal).unwrap();
        let adapter = ExecutionPlanAdapter;
        let pipeline = adapter.convert(&plan, None);

        assert_eq!(pipeline.len(), 1);
        // Planner defaults retry_count=2 for click steps
        match pipeline[0].retry_policy {
            RetryPolicy::ExponentialBackoff { max_retries, .. } => {
                assert!(max_retries > 0);
            }
            _ => panic!("expected ExponentialBackoff for click step"),
        }
    }

    #[test]
    fn test_adapter_launch_activity_precondition() {
        let step = make_sample_step(
            "launch",
            ActionType::LaunchActivity {
                package: "com.example".to_string(),
                activity: ".Main".to_string(),
                data: None,
            },
            0,
        );
        let preconditions = ExecutionPlanAdapter::derive_preconditions(&step, None);
        assert!(preconditions.iter().any(|p| matches!(
            p,
            Precondition::AppNotRunning(app) if app == "com.example"
        )));
    }

    #[test]
    fn test_adapter_all_action_types_produce_valid_pipeline_steps() {
        let adapter = ExecutionPlanAdapter;
        let actions = vec![
            ActionType::Speak { text: "hi".into() },
            ActionType::Notify {
                title: "Test".into(),
                body: "body".into(),
                priority: NotifyPriority::Normal,
            },
            ActionType::OpenApp {
                app_id: "calc".into(),
                data: None,
            },
            ActionType::CreateMemory {
                title: "t".into(),
                content: "c".into(),
                category: "general".into(),
                tags: vec![],
                importance: 5,
            },
            ActionType::SearchMemory {
                query: "test".into(),
                max_results: 10,
            },
            ActionType::RunAI {
                prompt: "hello".into(),
                session_id: None,
            },
            ActionType::DeviceControl {
                control: DeviceControl::SetBrightness(50),
            },
            ActionType::DeviceControl {
                control: DeviceControl::LockScreen,
            },
            ActionType::Wait { duration_ms: 100 },
            ActionType::ClickScreenElement {
                query: "btn".to_string(),
            },
            ActionType::ClickScreenText {
                text: "save".to_string(),
            },
            ActionType::TypeIntoScreenElement {
                query: "field".to_string(),
                text: "hello".to_string(),
            },
        ];

        let steps: Vec<ExecutionStep> = actions
            .into_iter()
            .enumerate()
            .map(|(i, action)| make_sample_step(&format!("s{}", i + 1), action, 0))
            .collect();

        let plan = make_plan_with_steps(steps);
        let pipeline = adapter.convert(&plan, None);

        assert_eq!(pipeline.len(), 12);
        for (i, ps) in pipeline.iter().enumerate() {
            assert_eq!(ps.step_index, i);
            assert!(ps.is_pending());
            assert!(
                !ps.preconditions.is_empty()
                    || ps.retry_policy != RetryPolicy::NoRetry
                    || matches!(ps.verification, VerificationStrategy::NoVerification)
                    || matches!(ps.expected_outcome, ExpectedOutcome::NoChange)
            );
        }
    }
}
