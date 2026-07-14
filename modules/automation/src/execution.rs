use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use crate::action::{ActionExecutor, ActionType, DefaultActionExecutor};
use crate::condition::{ConditionEvaluator, DefaultConditionEvaluator};
use crate::config::AutomationConfig;
use crate::error::AutomationError;
use crate::events::AutomationEventPayload;
use crate::history::{ExecutionRecord, ExecutionStatus, HistoryStore};
use crate::workflow::{Workflow, WorkflowStep};

pub struct ExecutionEngine {
    _config: AutomationConfig,
    action_executors: RwLock<HashMap<String, Box<dyn ActionExecutor>>>,
    condition_evaluator: DefaultConditionEvaluator,
    history: Arc<dyn HistoryStore>,
    active_executions: RwLock<HashMap<String, Arc<ExecutionState>>>,
}

struct ExecutionState {
    cancelled: AtomicBool,
}

impl ExecutionEngine {
    pub fn new(config: AutomationConfig, history: Arc<dyn HistoryStore>) -> Self {
        let mut executors: HashMap<String, Box<dyn ActionExecutor>> = HashMap::new();
        executors.insert("default".to_string(), Box::new(DefaultActionExecutor));
        Self {
            _config: config,
            action_executors: RwLock::new(executors),
            condition_evaluator: DefaultConditionEvaluator,
            history,
            active_executions: RwLock::new(HashMap::new()),
        }
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

        // Execute with retry
        let effective_max = step.retry_count.max(max_retries);
        let mut last_error = String::new();

        for attempt in 0..=effective_max {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(1000));
            }

            let action_kind = action_kind_name(&step.action);

            let executor = DefaultActionExecutor;
            let result = executor.execute(&step.action);

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
        }

        publish(AutomationEventPayload::AutomationError {
            workflow_id: workflow_id.to_string(),
            error: format!(
                "step {} failed after {} retries: {}",
                idx, effective_max, last_error
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
    }
}
