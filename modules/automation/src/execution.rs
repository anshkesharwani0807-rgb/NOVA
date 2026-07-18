use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use crate::action::{
    ActionExecutor, ActionResult, ActionType, DefaultActionExecutor, InputInjectionParams,
};
use crate::condition::{ConditionEvaluator, DefaultConditionEvaluator};
use crate::config::AutomationConfig;
use crate::consent_gate::{ConsentDecision, ConsentGate};
use crate::error::AutomationError;
use crate::events::AutomationEventPayload;
use crate::history::{ExecutionRecord, ExecutionStatus, HistoryStore};
use crate::real_executors::{
    ScreenClickExecutor, ScreenDragExecutor, ScreenSwipeExecutor, ScreenTypeExecutor,
};
use crate::screen_executor::ScreenAwareExecutor;
use crate::workflow::{Workflow, WorkflowStep};

/// Wraps DefaultActionExecutor with InputEngine support for input injection.
pub struct InputAwareExecutor {
    input_engine: Option<Arc<dyn nova_input::InputEngine>>,
}

impl InputAwareExecutor {
    pub fn new(input_engine: Option<Arc<dyn nova_input::InputEngine>>) -> Self {
        Self { input_engine }
    }

    fn execute_input(&self, params: &InputInjectionParams) -> ActionResult {
        let engine = match self.input_engine.as_ref() {
            Some(e) => e.clone(),
            None => return ActionResult::failure("input engine not configured"),
        };

        let input_action = match params.action_type.as_str() {
            "click" => nova_input::InputAction::Mouse(nova_input::MouseAction::Click {
                point: nova_input::Point {
                    x: params.get_i32("x", 0),
                    y: params.get_i32("y", 0),
                },
                button: parse_mouse_button(&params.params.get("button").map(String::as_str)),
                count: params.get_i32("count", 1) as u32,
            }),
            "double_click" => nova_input::InputAction::Mouse(nova_input::MouseAction::Click {
                point: nova_input::Point {
                    x: params.get_i32("x", 0),
                    y: params.get_i32("y", 0),
                },
                button: parse_mouse_button(&params.params.get("button").map(String::as_str)),
                count: 2,
            }),
            "right_click" => nova_input::InputAction::Mouse(nova_input::MouseAction::Click {
                point: nova_input::Point {
                    x: params.get_i32("x", 0),
                    y: params.get_i32("y", 0),
                },
                button: nova_input::MouseButton::Right,
                count: 1,
            }),
            "move" => nova_input::InputAction::Mouse(nova_input::MouseAction::Move {
                point: nova_input::Point {
                    x: params.get_i32("x", 0),
                    y: params.get_i32("y", 0),
                },
                absolute: true,
            }),
            "drag" => nova_input::InputAction::Mouse(nova_input::MouseAction::Drag {
                from: nova_input::Point {
                    x: params.get_i32("from_x", 0),
                    y: params.get_i32("from_y", 0),
                },
                to: nova_input::Point {
                    x: params.get_i32("to_x", 0),
                    y: params.get_i32("to_y", 0),
                },
                button: parse_mouse_button(&params.params.get("button").map(String::as_str)),
            }),
            "scroll" => nova_input::InputAction::Mouse(nova_input::MouseAction::Scroll {
                delta_x: params.get_i32("delta_x", 0),
                delta_y: params.get_i32("delta_y", 0),
            }),
            "type" => nova_input::InputAction::Keyboard(nova_input::KeyboardAction::TypeText {
                text: params.params.get("text").cloned().unwrap_or_default(),
            }),
            "key_press" => {
                nova_input::InputAction::Keyboard(nova_input::KeyboardAction::KeyPress {
                    key: params.params.get("key").cloned().unwrap_or_default(),
                    modifiers: parse_modifiers(&params.params.get("modifiers").map(String::as_str)),
                })
            }
            "key_release" => {
                nova_input::InputAction::Keyboard(nova_input::KeyboardAction::KeyRelease {
                    key: params.params.get("key").cloned().unwrap_or_default(),
                })
            }
            "hotkey" => {
                let keys_str = params.params.get("keys").cloned().unwrap_or_default();
                let keys: Vec<String> = keys_str.split(',').map(|s| s.trim().to_string()).collect();
                nova_input::InputAction::Keyboard(nova_input::KeyboardAction::Hotkey { keys })
            }
            "tap" => nova_input::InputAction::Touch(nova_input::TouchAction::Tap {
                point: nova_input::Point {
                    x: params.get_i32("x", 0),
                    y: params.get_i32("y", 0),
                },
            }),
            "double_tap" => nova_input::InputAction::Touch(nova_input::TouchAction::DoubleTap {
                point: nova_input::Point {
                    x: params.get_i32("x", 0),
                    y: params.get_i32("y", 0),
                },
            }),
            "long_press" => nova_input::InputAction::Touch(nova_input::TouchAction::LongPress {
                point: nova_input::Point {
                    x: params.get_i32("x", 0),
                    y: params.get_i32("y", 0),
                },
                duration_ms: params.get_u64("duration_ms", 500),
            }),
            "swipe" => nova_input::InputAction::Touch(nova_input::TouchAction::Swipe {
                from: nova_input::Point {
                    x: params.get_i32("from_x", 0),
                    y: params.get_i32("from_y", 0),
                },
                to: nova_input::Point {
                    x: params.get_i32("to_x", 0),
                    y: params.get_i32("to_y", 0),
                },
                duration_ms: params.get_u64("duration_ms", 200),
            }),
            "pinch" => nova_input::InputAction::Touch(nova_input::TouchAction::Pinch {
                center: nova_input::Point {
                    x: params.get_i32("x", 0),
                    y: params.get_i32("y", 0),
                },
                scale: params.get_f32("scale", 1.5),
                duration_ms: params.get_u64("duration_ms", 300),
            }),
            "back" | "home" | "recents" => {
                nova_input::InputAction::Keyboard(nova_input::KeyboardAction::KeyPress {
                    key: params.action_type.clone(),
                    modifiers: Vec::new(),
                })
            }
            "wait" => nova_input::InputAction::Wait {
                duration_ms: params.get_u64("duration_ms", 1000),
            },
            other => {
                return ActionResult::failure(format!("unsupported input action: {other}"));
            }
        };

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => match handle.block_on(engine.execute(&input_action)) {
                Ok(result) => ActionResult::success(format!("input: {}", result.detail)),
                Err(e) => ActionResult::failure(format!("input error: {e}")),
            },
            Err(_) => ActionResult::failure("no tokio runtime available for input execution"),
        }
    }
}

fn parse_mouse_button(button: &Option<&str>) -> nova_input::MouseButton {
    match button.and_then(|b| {
        if b.eq_ignore_ascii_case("right") {
            Some(nova_input::MouseButton::Right)
        } else if b.eq_ignore_ascii_case("middle") {
            Some(nova_input::MouseButton::Middle)
        } else {
            None
        }
    }) {
        Some(b) => b,
        None => nova_input::MouseButton::Left,
    }
}

fn parse_modifiers(modifiers: &Option<&str>) -> Vec<nova_input::Modifier> {
    let raw = match modifiers {
        Some(s) => s,
        None => return Vec::new(),
    };
    raw.split(',')
        .map(|s| match s.trim().to_lowercase().as_str() {
            "ctrl" | "control" => nova_input::Modifier::Ctrl,
            "alt" => nova_input::Modifier::Alt,
            "shift" => nova_input::Modifier::Shift,
            "win" | "windows" | "super" => nova_input::Modifier::Win,
            "meta" | "cmd" | "command" => nova_input::Modifier::Meta,
            _ => nova_input::Modifier::Ctrl,
        })
        .collect()
}

impl ActionExecutor for InputAwareExecutor {
    fn execute(&self, action: &ActionType) -> ActionResult {
        match action {
            ActionType::InputInjection(params) => self.execute_input(params),
            other => {
                let inner = DefaultActionExecutor;
                inner.execute(other)
            }
        }
    }

    fn kind(&self) -> &'static str {
        "input-aware"
    }
}

pub struct ExecutionEngine {
    config: AutomationConfig,
    action_executors: RwLock<HashMap<String, Box<dyn ActionExecutor>>>,
    condition_evaluator: DefaultConditionEvaluator,
    history: Arc<dyn HistoryStore>,
    active_executions: RwLock<HashMap<String, Arc<ExecutionState>>>,
    consent_gate: RwLock<Option<Arc<ConsentGate>>>,
    autonomy_level: RwLock<String>,
}

struct ExecutionState {
    cancelled: AtomicBool,
}

impl ExecutionEngine {
    pub fn new(config: AutomationConfig, history: Arc<dyn HistoryStore>) -> Self {
        let mut executors: HashMap<String, Box<dyn ActionExecutor>> = HashMap::new();
        let default_exec: Box<dyn ActionExecutor> = Box::new(DefaultActionExecutor);
        executors.insert("default".to_string(), default_exec);
        Self {
            autonomy_level: RwLock::new("conservative".to_string()),
            config,
            action_executors: RwLock::new(executors),
            condition_evaluator: DefaultConditionEvaluator,
            history,
            active_executions: RwLock::new(HashMap::new()),
            consent_gate: RwLock::new(None),
        }
    }

    pub fn set_consent_gate(&self, gate: Arc<ConsentGate>) {
        *self.consent_gate.write() = Some(gate);
    }

    pub fn set_autonomy_level(&self, level: &str) {
        *self.autonomy_level.write() = level.to_string();
    }

    pub fn with_input_engine(mut self, input_engine: Arc<dyn nova_input::InputEngine>) -> Self {
        let executor: Box<dyn ActionExecutor> =
            Box::new(InputAwareExecutor::new(Some(input_engine)));
        self.action_executors
            .get_mut()
            .insert("default".to_string(), executor);
        self
    }

    pub fn set_input_engine(&self, engine: Arc<dyn nova_input::InputEngine>) {
        let executor: Box<dyn ActionExecutor> = Box::new(InputAwareExecutor::new(Some(engine)));
        self.action_executors
            .write()
            .insert("default".to_string(), executor);
    }

    pub fn with_screen_engine(
        mut self,
        screen_engine: Arc<parking_lot::RwLock<nova_screen::ScreenEngine>>,
    ) -> Self {
        let executor: Box<dyn ActionExecutor> =
            Box::new(ScreenAwareExecutor::new(Some(screen_engine), None));
        self.action_executors
            .get_mut()
            .insert("default".to_string(), executor);
        self
    }

    pub fn set_screen_engine(
        &self,
        screen_engine: Arc<parking_lot::RwLock<nova_screen::ScreenEngine>>,
    ) {
        let executor: Box<dyn ActionExecutor> =
            Box::new(ScreenAwareExecutor::new(Some(screen_engine), None));
        self.action_executors
            .write()
            .insert("default".to_string(), executor);
    }

    pub fn set_screen_and_input(
        &self,
        screen_engine: Arc<parking_lot::RwLock<nova_screen::ScreenEngine>>,
        input_engine: Arc<dyn nova_input::InputEngine>,
    ) {
        let mut executors = self.action_executors.write();
        executors.insert(
            "default".to_string(),
            Box::new(ScreenAwareExecutor::new(
                Some(screen_engine.clone()),
                Some(input_engine.clone()),
            )),
        );
        executors.insert(
            "click".to_string(),
            Box::new(ScreenClickExecutor::new(
                screen_engine.clone(),
                input_engine.clone(),
            )),
        );
        executors.insert(
            "type".to_string(),
            Box::new(ScreenTypeExecutor::new(
                screen_engine.clone(),
                input_engine.clone(),
            )),
        );
        executors.insert(
            "drag".to_string(),
            Box::new(ScreenDragExecutor::new(
                screen_engine.clone(),
                input_engine.clone(),
            )),
        );
        executors.insert(
            "swipe".to_string(),
            Box::new(ScreenSwipeExecutor::new(screen_engine, input_engine)),
        );
    }

    pub fn register_executor(&self, name: &str, executor: Box<dyn ActionExecutor>) {
        self.action_executors
            .write()
            .insert(name.to_string(), executor);
    }

    pub fn execute_workflow(
        &self,
        workflow: Arc<Workflow>,
        context: HashMap<String, String>,
        publish: &dyn Fn(AutomationEventPayload),
    ) -> String {
        let execution_id = Uuid::new_v4().to_string();
        let state = Arc::new(ExecutionState {
            cancelled: AtomicBool::new(false),
        });
        self.active_executions
            .write()
            .insert(execution_id.clone(), state.clone());

        publish(AutomationEventPayload::WorkflowStarted {
            workflow_id: workflow.id.clone(),
            execution_id: execution_id.clone(),
        });

        let start = chrono::Utc::now().timestamp_millis();
        let start_record = start;

        let steps = if workflow.parallel {
            self.execute_parallel(&workflow, &context, &execution_id, state.clone(), publish)
        } else {
            self.execute_sequential(&workflow, &context, &execution_id, state.clone(), publish)
        };

        let elapsed = chrono::Utc::now().timestamp_millis() - start_record;

        let succeeded = steps.iter().filter(|s| s.success).count();
        let failed = steps.iter().filter(|s| !s.success).count();

        let status = if failed == 0 {
            ExecutionStatus::Completed
        } else if succeeded == 0 {
            ExecutionStatus::Failed
        } else {
            ExecutionStatus::Partial
        };

        let record = ExecutionRecord {
            execution_id: execution_id.clone(),
            workflow_id: workflow.id.clone(),
            workflow_name: workflow.name.clone(),
            status: status.clone(),
            started_at: start,
            duration_ms: elapsed,
            steps_succeeded: succeeded,
            steps_failed: failed,
            steps_total: steps.len(),
            error: None,
        };
        self.history.store(record);

        match status {
            ExecutionStatus::Completed => {
                publish(AutomationEventPayload::WorkflowCompleted {
                    workflow_id: workflow.id.clone(),
                    execution_id: execution_id.clone(),
                    steps_succeeded: succeeded,
                    steps_failed: failed,
                    duration_ms: elapsed,
                });
            }
            ExecutionStatus::Failed => {
                publish(AutomationEventPayload::WorkflowFailed {
                    workflow_id: workflow.id.clone(),
                    execution_id: execution_id.clone(),
                    error: format!("{} steps failed", failed),
                });
            }
            _ => {
                publish(AutomationEventPayload::WorkflowCompleted {
                    workflow_id: workflow.id.clone(),
                    execution_id: execution_id.clone(),
                    steps_succeeded: succeeded,
                    steps_failed: failed,
                    duration_ms: elapsed,
                });
            }
        }

        self.active_executions.write().remove(&execution_id);
        execution_id
    }

    fn execute_sequential(
        &self,
        workflow: &Workflow,
        context: &HashMap<String, String>,
        execution_id: &str,
        state: Arc<ExecutionState>,
        publish: &dyn Fn(AutomationEventPayload),
    ) -> Vec<StepOutcome> {
        let mut results = Vec::new();

        for (idx, step) in workflow.steps.iter().enumerate() {
            if state.cancelled.load(Ordering::SeqCst) {
                break;
            }

            let outcome = self.execute_step(step, idx, context, workflow, execution_id, publish);
            results.push(outcome.clone());

            if !outcome.success && !step.continue_on_failure {
                break;
            }
        }

        results
    }

    fn execute_parallel(
        &self,
        workflow: &Workflow,
        context: &HashMap<String, String>,
        execution_id: &str,
        state: Arc<ExecutionState>,
        _publish: &dyn Fn(AutomationEventPayload),
    ) -> Vec<StepOutcome> {
        let mut handles = Vec::new();

        for (idx, step) in workflow.steps.iter().enumerate() {
            let step = step.clone();
            let ctx = context.clone();
            let wf_id = workflow.id.clone();
            let exec_id = execution_id.to_string();
            let s = state.clone();

            let handle = std::thread::spawn(move || {
                if s.cancelled.load(Ordering::SeqCst) {
                    return StepOutcome {
                        step_id: step.id,
                        step_index: idx,
                        success: false,
                        message: "cancelled".to_string(),
                        data: None,
                    };
                }
                let engine = ExecutionEngine::new(
                    AutomationConfig::default(),
                    Arc::new(crate::history::InMemoryHistory::new()),
                );
                engine.execute_step_direct(&step, idx, &ctx, &wf_id, &exec_id, 0, &|_| {})
            });
            handles.push(handle);
        }

        handles
            .into_iter()
            .map(|h| {
                h.join().unwrap_or_else(|_| StepOutcome {
                    step_id: "unknown".to_string(),
                    step_index: 0,
                    success: false,
                    message: "thread panic".to_string(),
                    data: None,
                })
            })
            .collect()
    }

    fn execute_step(
        &self,
        step: &WorkflowStep,
        idx: usize,
        context: &HashMap<String, String>,
        workflow: &Workflow,
        execution_id: &str,
        publish: &dyn Fn(AutomationEventPayload),
    ) -> StepOutcome {
        self.execute_step_direct(
            step,
            idx,
            context,
            &workflow.id,
            execution_id,
            workflow.max_retries,
            publish,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_step_direct(
        &self,
        step: &WorkflowStep,
        idx: usize,
        context: &HashMap<String, String>,
        workflow_id: &str,
        execution_id: &str,
        max_retries: u32,
        publish: &dyn Fn(AutomationEventPayload),
    ) -> StepOutcome {
        // Check condition
        if let Some(ref cond) = step.condition {
            let result = self.condition_evaluator.evaluate(cond, context);
            publish(AutomationEventPayload::ConditionMatched {
                workflow_id: workflow_id.to_string(),
                execution_id: execution_id.to_string(),
                condition: format!("{:?}", cond),
                matched: result.matched,
            });
            if !result.matched {
                return StepOutcome {
                    step_id: step.id.clone(),
                    step_index: idx,
                    success: true,
                    message: format!("condition not met: {}", result.reason),
                    data: None,
                };
            }
        }

        // Check consent gate
        let consent_decision = self.consent_gate.read().as_ref().map(|gate| {
            let level = self.autonomy_level.read().clone();
            gate.check_action(&step.action, &level)
        });
        if let Some(decision) = consent_decision {
            match decision {
                ConsentDecision::Allowed => {}
                ConsentDecision::Blocked { reason } => {
                    publish(AutomationEventPayload::AutomationError {
                        workflow_id: workflow_id.to_string(),
                        error: format!("step {idx} blocked by consent: {reason}"),
                    });
                    return StepOutcome {
                        step_id: step.id.clone(),
                        step_index: idx,
                        success: false,
                        message: format!("blocked by consent: {reason}"),
                        data: None,
                    };
                }
                ConsentDecision::RequiresPrompt {
                    stakes: _,
                    description,
                } => {
                    publish(AutomationEventPayload::AutomationError {
                        workflow_id: workflow_id.to_string(),
                        error: format!("step {idx} requires user consent: {description}"),
                    });
                    return StepOutcome {
                        step_id: step.id.clone(),
                        step_index: idx,
                        success: false,
                        message: format!("requires user consent: {description}"),
                        data: None,
                    };
                }
            }
        }

        // Determine timeout per step
        let step_timeout = if step.timeout_ms > 0 {
            step.timeout_ms
        } else {
            self.config.step_timeout_ms
        };

        // Execute with retry
        let effective_max = step.retry_count.max(max_retries);
        let mut last_error = String::new();

        for attempt in 0..=effective_max {
            if attempt > 0 {
                let delay = self.config.retry_delay_ms * (1u64 << attempt.min(5));
                std::thread::sleep(std::time::Duration::from_millis(delay.min(10_000)));
            }

            let action_kind = action_kind_name(&step.action);

            let executors = self.action_executors.read();

            let executor_name = named_executor_for(action_kind);
            let executor = executors
                .get(executor_name)
                .or_else(|| executors.get("default"));

            let result = match executor {
                Some(exec) => exec.execute(&step.action),
                None => {
                    let fallback = DefaultActionExecutor;
                    fallback.execute(&step.action)
                }
            };
            drop(executors);

            publish(AutomationEventPayload::ActionExecuted {
                workflow_id: workflow_id.to_string(),
                execution_id: execution_id.to_string(),
                step: idx,
                action_type: action_kind.to_string(),
                success: result.success,
            });

            if result.success {
                let msg = if attempt > 0 {
                    format!("{} (retry {})", result.message, attempt)
                } else {
                    result.message
                };
                return StepOutcome {
                    step_id: step.id.clone(),
                    step_index: idx,
                    success: true,
                    message: msg,
                    data: result.data,
                };
            }

            last_error = result.message;

            if last_error.contains("timed out") || last_error.contains("timeout") {
                break;
            }
        }

        publish(AutomationEventPayload::AutomationError {
            workflow_id: workflow_id.to_string(),
            error: format!(
                "step {} failed after {} retries within {}ms: {}",
                idx, effective_max, step_timeout, last_error
            ),
        });

        StepOutcome {
            step_id: step.id.clone(),
            step_index: idx,
            success: false,
            message: format!("failed after {} retries: {}", effective_max, last_error),
            data: None,
        }
    }

    pub fn cancel_execution(&self, execution_id: &str) -> Result<(), AutomationError> {
        let map = self.active_executions.read();
        match map.get(execution_id) {
            Some(state) => {
                state.cancelled.store(true, Ordering::SeqCst);
                Ok(())
            }
            None => Err(AutomationError::WorkflowNotFound(execution_id.to_string())),
        }
    }

    pub fn active_count(&self) -> usize {
        self.active_executions.read().len()
    }
}

#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub step_id: String,
    pub step_index: usize,
    pub success: bool,
    pub message: String,
    pub data: Option<String>,
}

fn action_kind_name(action: &ActionType) -> &'static str {
    match action {
        ActionType::Speak { .. } => "speak",
        ActionType::Notify { .. } => "notify",
        ActionType::OpenApp { .. } => "open_app",
        ActionType::LaunchActivity { .. } => "launch_activity",
        ActionType::Clipboard { .. } => "clipboard",
        ActionType::CreateMemory { .. } => "create_memory",
        ActionType::SearchMemory { .. } => "search_memory",
        ActionType::RunAI { .. } => "run_ai",
        ActionType::CaptureVoice { .. } => "capture_voice",
        ActionType::AnalyzeImage { .. } => "analyze_image",
        ActionType::DeviceControl { .. } => "device_control",
        ActionType::PluginInvocation { .. } => "plugin",
        ActionType::Wait { .. } => "wait",
        ActionType::SubWorkflow { .. } => "sub_workflow",
        ActionType::InputInjection(..) => "input_injection",
        ActionType::ClickScreenElement { .. } => "click_screen_element",
        ActionType::TypeIntoScreenElement { .. } => "type_into_screen_element",
        ActionType::ClickScreenText { .. } => "click_screen_text",
        ActionType::DragScreenElements { .. } => "drag_screen_elements",
        ActionType::SwipeScreenElements { .. } => "swipe_screen_elements",
    }
}

fn named_executor_for(action_kind: &str) -> &str {
    match action_kind {
        "click_screen_element" | "click_screen_text" => "click",
        "type_into_screen_element" => "type",
        "drag_screen_elements" => "drag",
        "swipe_screen_elements" => "swipe",
        _ => "default",
    }
}
