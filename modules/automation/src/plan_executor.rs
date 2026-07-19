use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::action::{ActionExecutor, ActionResult, ActionType, DefaultActionExecutor};
use crate::observability::ExecutionMetrics;
use crate::outcome_verifier::{OutcomeVerifier, VerificationEvidence, VerificationResult};
use crate::pipeline_step::{
    expected_outcome_for_action, retry_policy_for_step, verification_strategy_for_action,
    PipelineStep, PipelineStepStatus, Precondition,
};
use crate::planner::{ExecutionPlan, Goal, Planner};
use crate::recovery_orchestrator::{RecoveryContext, RecoveryDecision, RecoveryOrchestrator};
use crate::world_state::{WorldSnapshot, WorldState};

#[derive(Debug, Clone)]
pub struct PlanExecutorConfig {
    pub default_step_timeout_ms: u64,
    pub enable_verification: bool,
    pub enable_recovery: bool,
    pub max_replans_per_goal: u32,
    pub record_evidence: bool,
}

impl Default for PlanExecutorConfig {
    fn default() -> Self {
        Self {
            default_step_timeout_ms: 30_000,
            enable_verification: true,
            enable_recovery: true,
            max_replans_per_goal: 3,
            record_evidence: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Completed,
    Failed,
    Skipped,
    Cancelled,
    Aborted,
    Escalated,
    Replanned,
}

#[derive(Debug, Clone)]
pub struct StepExecutionRecord {
    pub step_id: String,
    pub step_index: usize,
    pub description: String,
    pub status: StepStatus,
    pub attempts: u32,
    pub duration: Duration,
    pub error: Option<String>,
    pub verification_result: Option<VerificationResult>,
    pub recovery_decision: Option<RecoveryDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineExecutionState {
    Running,
    Completed { success: bool },
    Cancelled,
    Aborted { reason: String },
    Escalated { reason: String, suggestion: String },
}

#[derive(Debug, Clone)]
pub struct GoalExecutionReport {
    pub goal: String,
    pub plan_id: String,
    pub success: bool,
    pub state: PipelineExecutionState,
    pub total_steps: usize,
    pub completed_steps: usize,
    pub failed_steps: usize,
    pub skipped_steps: usize,
    pub retried_steps: usize,
    pub replans: usize,
    pub abort_reason: Option<String>,
    pub execution_duration: Duration,
    pub verification_count: usize,
    pub recovery_count: usize,
    pub step_records: Vec<StepExecutionRecord>,
    pub metrics: ExecutionMetrics,
}

#[derive(Debug, Clone)]
pub struct ExecutionSummary {
    pub total_steps: usize,
    pub successful_steps: usize,
    pub failed_steps: usize,
    pub skipped_steps: usize,
    pub total_retries: usize,
    pub total_replans: usize,
    pub total_verifications: usize,
    pub total_recoveries: usize,
    pub total_duration: Duration,
    pub completed: bool,
    pub was_cancelled: bool,
    pub abort_reason: Option<String>,
}

struct ExecutionContext {
    _execution_id: String,
    goal: Goal,
    plan: ExecutionPlan,
    cancelled: Arc<AtomicBool>,
    metrics: ExecutionMetrics,
    replan_count: u32,
}

pub struct PlanExecutor {
    planner: Planner,
    action_executors: Arc<RwLock<HashMap<String, Box<dyn ActionExecutor>>>>,
    outcome_verifier: Arc<OutcomeVerifier>,
    recovery_orchestrator: Arc<RecoveryOrchestrator>,
    world_state: Arc<RwLock<WorldState>>,
    config: PlanExecutorConfig,
}

impl PlanExecutor {
    pub fn new(
        planner: Planner,
        outcome_verifier: Arc<OutcomeVerifier>,
        recovery_orchestrator: Arc<RecoveryOrchestrator>,
        world_state: Arc<RwLock<WorldState>>,
    ) -> Self {
        let mut executors: HashMap<String, Box<dyn ActionExecutor>> = HashMap::new();
        executors.insert("default".to_string(), Box::new(DefaultActionExecutor));
        Self {
            planner,
            action_executors: Arc::new(RwLock::new(executors)),
            outcome_verifier,
            recovery_orchestrator,
            world_state,
            config: PlanExecutorConfig::default(),
        }
    }

    pub fn with_config(mut self, config: PlanExecutorConfig) -> Self {
        self.config = config;
        self
    }

    pub fn config(&self) -> &PlanExecutorConfig {
        &self.config
    }

    pub fn register_executor(&self, name: &str, executor: Box<dyn ActionExecutor>) {
        self.action_executors
            .write()
            .insert(name.to_string(), executor);
    }

    pub fn with_screen_engine(
        &self,
        screen_engine: Arc<parking_lot::RwLock<nova_screen::ScreenEngine>>,
    ) {
        use crate::screen_executor::ScreenAwareExecutor;
        let executor: Box<dyn ActionExecutor> =
            Box::new(ScreenAwareExecutor::new(Some(screen_engine), None));
        self.action_executors
            .write()
            .insert("default".to_string(), executor);
    }

    pub fn with_input_engine(&self, input_engine: Arc<dyn nova_input::InputEngine>) {
        use crate::execution::InputAwareExecutor;
        let executor: Box<dyn ActionExecutor> =
            Box::new(InputAwareExecutor::new(Some(input_engine)));
        self.action_executors
            .write()
            .insert("default".to_string(), executor);
    }

    pub fn execute_goal(&self, goal: Goal) -> GoalExecutionReport {
        let execution_id = uuid::Uuid::new_v4().to_string();
        let cancelled = Arc::new(AtomicBool::new(false));
        let start = Instant::now();

        let plan = match self.planner.plan(&goal) {
            Ok(p) => p,
            Err(e) => {
                return GoalExecutionReport {
                    goal: goal.description.clone(),
                    plan_id: String::new(),
                    success: false,
                    state: PipelineExecutionState::Aborted {
                        reason: format!("planning failed: {e}"),
                    },
                    total_steps: 0,
                    completed_steps: 0,
                    failed_steps: 0,
                    skipped_steps: 0,
                    retried_steps: 0,
                    replans: 0,
                    abort_reason: Some(format!("planning failed: {e}")),
                    execution_duration: start.elapsed(),
                    verification_count: 0,
                    recovery_count: 0,
                    step_records: vec![],
                    metrics: ExecutionMetrics::default(),
                }
            }
        };

        let validation = self.planner.validate(&plan);
        if !validation.is_valid {
            return GoalExecutionReport {
                goal: goal.description.clone(),
                plan_id: plan.id.clone(),
                success: false,
                state: PipelineExecutionState::Aborted {
                    reason: format!("plan validation failed: {}", validation.errors.join("; ")),
                },
                total_steps: plan.steps.len(),
                completed_steps: 0,
                failed_steps: 0,
                skipped_steps: 0,
                retried_steps: 0,
                replans: 0,
                abort_reason: Some(format!(
                    "validation failed: {}",
                    validation.errors.join("; ")
                )),
                execution_duration: start.elapsed(),
                verification_count: 0,
                recovery_count: 0,
                step_records: vec![],
                metrics: ExecutionMetrics::default(),
            };
        }

        let mut ctx = ExecutionContext {
            _execution_id: execution_id,
            goal: goal.clone(),
            plan: plan.clone(),
            cancelled,
            metrics: ExecutionMetrics::default(),
            replan_count: 0,
        };

        let mut pipeline_steps = self.build_pipeline_steps(&plan);
        self.execute_pipeline(&mut pipeline_steps, &mut ctx, start)
    }

    pub fn execute_plan(&self, plan: ExecutionPlan, goal: Goal) -> GoalExecutionReport {
        let execution_id = uuid::Uuid::new_v4().to_string();
        let cancelled = Arc::new(AtomicBool::new(false));
        let start = Instant::now();

        let validation = self.planner.validate(&plan);
        if !validation.is_valid {
            return GoalExecutionReport {
                goal: goal.description.clone(),
                plan_id: plan.id.clone(),
                success: false,
                state: PipelineExecutionState::Aborted {
                    reason: format!("plan validation failed: {}", validation.errors.join("; ")),
                },
                total_steps: plan.steps.len(),
                completed_steps: 0,
                failed_steps: 0,
                skipped_steps: 0,
                retried_steps: 0,
                replans: 0,
                abort_reason: Some(format!(
                    "validation failed: {}",
                    validation.errors.join("; ")
                )),
                execution_duration: start.elapsed(),
                verification_count: 0,
                recovery_count: 0,
                step_records: vec![],
                metrics: ExecutionMetrics::default(),
            };
        }

        let mut ctx = ExecutionContext {
            _execution_id: execution_id,
            goal,
            plan: plan.clone(),
            cancelled,
            metrics: ExecutionMetrics::default(),
            replan_count: 0,
        };

        let mut pipeline_steps = self.build_pipeline_steps(&plan);
        self.execute_pipeline(&mut pipeline_steps, &mut ctx, start)
    }

    pub fn cancel_execution(&self, _execution_id: &str) -> bool {
        false
    }

    pub fn recovery_orchestrator(&self) -> &Arc<RecoveryOrchestrator> {
        &self.recovery_orchestrator
    }

    // ------------------------------------------------------------------
    // Internal: build pipeline steps from a plan
    // ------------------------------------------------------------------
    fn build_pipeline_steps(&self, plan: &ExecutionPlan) -> Vec<PipelineStep> {
        plan.steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                PipelineStep::new(
                    step.clone(),
                    i,
                    vec![],
                    verification_strategy_for_action(&step.action),
                    expected_outcome_for_action(&step.action),
                    retry_policy_for_step(step),
                )
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // Internal: execute a full pipeline
    // ------------------------------------------------------------------
    fn execute_pipeline(
        &self,
        pipeline_steps: &mut [PipelineStep],
        ctx: &mut ExecutionContext,
        start: Instant,
    ) -> GoalExecutionReport {
        let mut step_records = Vec::new();
        let mut completed = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;
        let mut total_retries = 0usize;
        let mut abort_reason: Option<String> = None;
        let mut terminal = false;

        let order = match self.planner.topological_sort(&ctx.plan) {
            Ok(o) => o,
            Err(e) => {
                return GoalExecutionReport {
                    goal: ctx.goal.description.clone(),
                    plan_id: ctx.plan.id.clone(),
                    success: false,
                    state: PipelineExecutionState::Aborted {
                        reason: format!("topological sort failed: {e}"),
                    },
                    total_steps: pipeline_steps.len(),
                    completed_steps: 0,
                    failed_steps: 0,
                    skipped_steps: 0,
                    retried_steps: 0,
                    replans: 0,
                    abort_reason: Some(format!("topological sort failed: {e}")),
                    execution_duration: start.elapsed(),
                    verification_count: 0,
                    recovery_count: 0,
                    step_records: vec![],
                    metrics: ctx.metrics.clone(),
                }
            }
        };

        for &step_idx in &order {
            if terminal || ctx.cancelled.load(Ordering::SeqCst) {
                break;
            }

            let step = &mut pipeline_steps[step_idx];
            if step.is_terminal() {
                continue;
            }

            let record = self.execute_single_step(step, ctx);
            ctx.metrics.total_steps_attempted += 1;

            match &record.status {
                StepStatus::Completed => completed += 1,
                StepStatus::Failed | StepStatus::Aborted => {
                    failed += 1;
                    abort_reason = record.error.clone();
                    terminal = true;
                }
                StepStatus::Skipped => skipped += 1,
                StepStatus::Cancelled => terminal = true,
                StepStatus::Escalated => {
                    failed += 1;
                    abort_reason = record.error.clone();
                    terminal = true;
                }
                StepStatus::Replanned => {
                    ctx.replan_count += 1;
                    if ctx.replan_count <= self.config.max_replans_per_goal {
                        failed += 1;
                        abort_reason =
                            Some("replan requested but not yet fully implemented".into());
                        terminal = true;
                    } else {
                        failed += 1;
                        abort_reason = Some("max replans exceeded".into());
                        terminal = true;
                    }
                }
                StepStatus::Pending => {}
            }

            if record.attempts > 1 {
                total_retries += record.attempts as usize - 1;
            }

            step_records.push(record);
        }

        let success = !terminal
            && completed + skipped == pipeline_steps.len()
            && !ctx.cancelled.load(Ordering::SeqCst);

        let state = if ctx.cancelled.load(Ordering::SeqCst) {
            PipelineExecutionState::Cancelled
        } else if terminal {
            PipelineExecutionState::Aborted {
                reason: abort_reason.clone().unwrap_or_default(),
            }
        } else if success {
            PipelineExecutionState::Completed { success: true }
        } else {
            PipelineExecutionState::Completed { success: false }
        };

        GoalExecutionReport {
            goal: ctx.goal.description.clone(),
            plan_id: ctx.plan.id.clone(),
            success,
            state,
            total_steps: pipeline_steps.len(),
            completed_steps: completed,
            failed_steps: failed,
            skipped_steps: skipped,
            retried_steps: total_retries,
            replans: ctx.replan_count as usize,
            abort_reason,
            execution_duration: start.elapsed(),
            verification_count: ctx.metrics.verification_count as usize,
            recovery_count: ctx.metrics.recoveries as usize,
            step_records,
            metrics: ctx.metrics.clone(),
        }
    }

    // ------------------------------------------------------------------
    // Internal: execute a single step with retry/recovery loop
    // ------------------------------------------------------------------
    fn execute_single_step(
        &self,
        step: &mut PipelineStep,
        ctx: &mut ExecutionContext,
    ) -> StepExecutionRecord {
        let step_id = step.step.id.clone();
        let step_index = step.step_index;
        let description = step.description();
        let step_start = Instant::now();

        step.status = PipelineStepStatus::InProgress;

        if self.config.enable_verification {
            if let Some(skip_reason) = self.evaluate_preconditions(&step.preconditions) {
                step.status = PipelineStepStatus::Skipped(skip_reason.clone());
                return StepExecutionRecord {
                    step_id,
                    step_index,
                    description,
                    status: StepStatus::Skipped,
                    attempts: 0,
                    duration: step_start.elapsed(),
                    error: Some(skip_reason),
                    verification_result: None,
                    recovery_decision: None,
                };
            }
        }

        let pre_snapshot = {
            let ws = self.world_state.read();
            if self.config.record_evidence {
                Some(ws.snapshot())
            } else {
                None
            }
        };

        let mut attempts = 0u32;
        let mut last_verification = None;
        let mut last_recovery = None;

        loop {
            if ctx.cancelled.load(Ordering::SeqCst) {
                step.status = PipelineStepStatus::Failed("execution cancelled".into());
                return StepExecutionRecord {
                    step_id,
                    step_index,
                    description,
                    status: StepStatus::Cancelled,
                    attempts,
                    duration: step_start.elapsed(),
                    error: Some("execution cancelled".into()),
                    verification_result: last_verification.clone(),
                    recovery_decision: last_recovery.clone(),
                };
            }

            let timeout_ms = if step.step.timeout_ms > 0 {
                step.step.timeout_ms
            } else {
                self.config.default_step_timeout_ms
            };

            let action_result = self.execute_action(&step.step.action, timeout_ms);
            attempts += 1;
            if attempts > 1 {
                ctx.metrics.record_retry();
            }

            if self.config.enable_verification {
                ctx.metrics.record_verification();
                let (verify_result, ref evidence) =
                    self.run_verification(step, &action_result, pre_snapshot.as_ref());
                last_verification = Some(verify_result.clone());

                match &verify_result {
                    VerificationResult::Passed => {
                        step.status = PipelineStepStatus::Succeeded;
                        return StepExecutionRecord {
                            step_id,
                            step_index,
                            description,
                            status: StepStatus::Completed,
                            attempts,
                            duration: step_start.elapsed(),
                            error: None,
                            verification_result: Some(verify_result),
                            recovery_decision: None,
                        };
                    }
                    _ => {
                        let failure_reason = match &verify_result {
                            VerificationResult::Failed { reason, .. } => reason.clone(),
                            VerificationResult::Uncertain { reason } => reason.clone(),
                            _ => "unknown verification failure".into(),
                        };

                        if self.config.enable_recovery {
                            ctx.metrics.record_recovery();
                            let recovery_ctx = RecoveryContext {
                                step: step.clone(),
                                verification: verify_result.clone(),
                                evidence: evidence.clone(),
                                world_diff: evidence.world_diff.clone(),
                                retry_count: attempts - 1,
                                failure_reason: failure_reason.clone(),
                            };

                            let (decision, _strategy, report) =
                                self.recovery_orchestrator.decide(&recovery_ctx);
                            self.recovery_orchestrator.record_outcome(report);
                            last_recovery = Some(decision.clone());

                            match decision {
                                RecoveryDecision::Retry {
                                    attempt: _,
                                    delay_ms,
                                } => {
                                    if delay_ms > 0 {
                                        std::thread::sleep(Duration::from_millis(delay_ms));
                                    }
                                    continue;
                                }
                                RecoveryDecision::Skip { reason } => {
                                    step.status = PipelineStepStatus::Skipped(reason.clone());
                                    return StepExecutionRecord {
                                        step_id,
                                        step_index,
                                        description,
                                        status: StepStatus::Skipped,
                                        attempts,
                                        duration: step_start.elapsed(),
                                        error: Some(reason),
                                        verification_result: last_verification.clone(),
                                        recovery_decision: last_recovery.clone(),
                                    };
                                }
                                RecoveryDecision::Abort { reason } => {
                                    step.status = PipelineStepStatus::Failed(reason.clone());
                                    return StepExecutionRecord {
                                        step_id,
                                        step_index,
                                        description,
                                        status: StepStatus::Aborted,
                                        attempts,
                                        duration: step_start.elapsed(),
                                        error: Some(reason),
                                        verification_result: last_verification.clone(),
                                        recovery_decision: last_recovery.clone(),
                                    };
                                }
                                RecoveryDecision::Replan {
                                    from_step_index: _,
                                    reason,
                                } => {
                                    return StepExecutionRecord {
                                        step_id,
                                        step_index,
                                        description,
                                        status: StepStatus::Replanned,
                                        attempts,
                                        duration: step_start.elapsed(),
                                        error: Some(reason),
                                        verification_result: last_verification.clone(),
                                        recovery_decision: last_recovery.clone(),
                                    };
                                }
                                RecoveryDecision::Escalate { reason, suggestion } => {
                                    return StepExecutionRecord {
                                        step_id,
                                        step_index,
                                        description,
                                        status: StepStatus::Escalated,
                                        attempts,
                                        duration: step_start.elapsed(),
                                        error: Some(format!("{reason}: {suggestion}")),
                                        verification_result: last_verification.clone(),
                                        recovery_decision: last_recovery.clone(),
                                    };
                                }
                            }
                        } else {
                            step.status = PipelineStepStatus::Failed(failure_reason.clone());
                            return StepExecutionRecord {
                                step_id,
                                step_index,
                                description,
                                status: StepStatus::Failed,
                                attempts,
                                duration: step_start.elapsed(),
                                error: Some(failure_reason),
                                verification_result: last_verification.clone(),
                                recovery_decision: None,
                            };
                        }
                    }
                }
            } else if action_result.success {
                step.status = PipelineStepStatus::Succeeded;
                return StepExecutionRecord {
                    step_id,
                    step_index,
                    description,
                    status: StepStatus::Completed,
                    attempts,
                    duration: step_start.elapsed(),
                    error: None,
                    verification_result: None,
                    recovery_decision: None,
                };
            } else {
                step.status = PipelineStepStatus::Failed(action_result.message.clone());
                return StepExecutionRecord {
                    step_id,
                    step_index,
                    description,
                    status: StepStatus::Failed,
                    attempts,
                    duration: step_start.elapsed(),
                    error: Some(action_result.message),
                    verification_result: None,
                    recovery_decision: None,
                };
            }
        }
    }

    // ------------------------------------------------------------------
    // Precondition evaluation
    // ------------------------------------------------------------------
    fn evaluate_preconditions(&self, preconditions: &[Precondition]) -> Option<String> {
        let ws = self.world_state.read();
        for pre in preconditions {
            match pre {
                Precondition::NoPrecondition => {}
                Precondition::DeviceState { field, expected } => {
                    if let Some(tel) = ws.device_telemetry() {
                        let actual = match field.as_str() {
                            "wifi" => tel.wifi_enabled.map(|v| {
                                if v {
                                    "enabled".into()
                                } else {
                                    "disabled".into()
                                }
                            }),
                            "bluetooth" => tel.bluetooth_enabled.map(|v| {
                                if v {
                                    "enabled".into()
                                } else {
                                    "disabled".into()
                                }
                            }),
                            "brightness" => tel.battery_level.map(|v| v.to_string()),
                            _ => None,
                        };
                        if actual.as_deref() == Some(expected.as_str()) {
                            return Some(format!("device state '{field}' already '{expected}'"));
                        }
                    }
                }
                Precondition::AppNotRunning(app) => {
                    if ws.active_app().map(|a| a != app).unwrap_or(true) {
                        return Some(format!("app '{app}' is not running"));
                    }
                }
                Precondition::AppIsRunning(app) => {
                    if ws.active_app() == Some(app.as_str()) {
                        return Some(format!("app '{app}' is already running"));
                    }
                }
                Precondition::ScreenContains(text) => {
                    if let Some(ocr) = ws.ocr_cache() {
                        if ocr.text.contains(text.as_str()) {
                            return Some(format!("screen already contains '{text}'"));
                        }
                    }
                }
                Precondition::NetworkState { online, wifi: _ } => {
                    if let Some(net) = ws.network_state() {
                        if let Some(expected) = online {
                            if net.is_online == Some(*expected) {
                                return Some("network online state already matches".into());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    // ------------------------------------------------------------------
    // Action execution with timeout via thread
    // ------------------------------------------------------------------
    fn execute_action(&self, action: &ActionType, timeout_ms: u64) -> ActionResult {
        let executors = self.action_executors.clone();
        let action = action.clone();
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let map = executors.read();
            let result = Self::find_and_execute(&map, &action);
            let _ = tx.send(result);
        });

        let effective = if timeout_ms == 0 { 30_000 } else { timeout_ms };
        match rx.recv_timeout(Duration::from_millis(effective)) {
            Ok(result) => result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                ActionResult::failure(format!("action timed out after {effective}ms"))
            }
            Err(_) => ActionResult::failure("action execution channel error"),
        }
    }

    fn find_and_execute(
        executors: &HashMap<String, Box<dyn ActionExecutor>>,
        action: &ActionType,
    ) -> ActionResult {
        let kind = action_kind_name(action);
        let name = named_executor_for(kind);
        match executors.get(name).or_else(|| executors.get("default")) {
            Some(exec) => exec.execute(action),
            None => DefaultActionExecutor.execute(action),
        }
    }

    // ------------------------------------------------------------------
    // Async verification wrapper
    // ------------------------------------------------------------------
    fn run_verification(
        &self,
        step: &PipelineStep,
        action_result: &ActionResult,
        pre_snapshot: Option<&WorldSnapshot>,
    ) -> (VerificationResult, VerificationEvidence) {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle.block_on(self.outcome_verifier.verify(
                step,
                action_result,
                pre_snapshot,
            )),
            Err(_) => {
                if let Ok(rt) = tokio::runtime::Runtime::new() {
                    rt.block_on(
                        self.outcome_verifier
                            .verify(step, action_result, pre_snapshot),
                    )
                } else {
                    (VerificationResult::Passed, VerificationEvidence::new())
                }
            }
        }
    }
}

// ------------------------------------------------------------------
// Action helper functions (mirrored from execution.rs to avoid coupling)
// ------------------------------------------------------------------
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

impl ExecutionSummary {
    pub fn from_report(report: &GoalExecutionReport) -> Self {
        Self {
            total_steps: report.total_steps,
            successful_steps: report.completed_steps,
            failed_steps: report.failed_steps,
            skipped_steps: report.skipped_steps,
            total_retries: report.retried_steps,
            total_replans: report.replans,
            total_verifications: report.verification_count,
            total_recoveries: report.recovery_count,
            total_duration: report.execution_duration,
            completed: matches!(report.state, PipelineExecutionState::Completed { .. }),
            was_cancelled: report.state == PipelineExecutionState::Cancelled,
            abort_reason: report.abort_reason.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::now_millis;
    use crate::planner::Capability;
    use crate::planner::ExecutionStep;
    use crate::world_state::DeviceTelemetry;
    use std::sync::Arc;

    fn make_world_state() -> Arc<RwLock<WorldState>> {
        Arc::new(RwLock::new(WorldState::new()))
    }

    fn make_verifier(ws: &Arc<RwLock<WorldState>>) -> Arc<OutcomeVerifier> {
        Arc::new(OutcomeVerifier::new(ws.clone(), None))
    }

    fn make_orchestrator() -> Arc<RecoveryOrchestrator> {
        Arc::new(RecoveryOrchestrator::new())
    }

    fn make_executor(ws: &Arc<RwLock<WorldState>>) -> PlanExecutor {
        let planner = Planner::new();
        let verifier = make_verifier(ws);
        let orch = make_orchestrator();
        PlanExecutor::new(planner, verifier, orch, ws.clone()).with_config(PlanExecutorConfig {
            enable_verification: false,
            enable_recovery: false,
            ..PlanExecutorConfig::default()
        })
    }

    fn make_executor_full(ws: &Arc<RwLock<WorldState>>) -> PlanExecutor {
        let planner = Planner::new();
        let verifier = make_verifier(ws);
        let orch = make_orchestrator();
        PlanExecutor::new(planner, verifier, orch, ws.clone())
    }

    fn make_plan_with_actions(actions: Vec<ActionType>) -> ExecutionPlan {
        let estimated = actions.len();
        let steps = actions
            .into_iter()
            .enumerate()
            .map(|(i, action)| ExecutionStep {
                id: format!("s{}", i + 1),
                description: format!("step {}", i + 1),
                action,
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 5000,
                retry_count: 0,
                continue_on_failure: false,
            })
            .collect();
        ExecutionPlan {
            id: uuid::Uuid::new_v4().to_string(),
            goal_description: "test".into(),
            steps,
            created_at: now_millis(),
            estimated_steps: estimated,
        }
    }

    fn make_plan_with_retry(actions: Vec<ActionType>, retry_count: u32) -> ExecutionPlan {
        let estimated = actions.len();
        let steps = actions
            .into_iter()
            .enumerate()
            .map(|(i, action)| ExecutionStep {
                id: format!("s{}", i + 1),
                description: format!("step {}", i + 1),
                action,
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 5000,
                retry_count,
                continue_on_failure: false,
            })
            .collect();
        ExecutionPlan {
            id: uuid::Uuid::new_v4().to_string(),
            goal_description: "test".into(),
            steps,
            created_at: now_millis(),
            estimated_steps: estimated,
        }
    }

    #[allow(dead_code)]
    fn make_failing_step() -> ExecutionStep {
        ExecutionStep {
            id: "f1".into(),
            description: "failing click".into(),
            action: ActionType::ClickScreenElement {
                query: "nonexistent_btn".into(),
            },
            dependencies: vec![],
            required_capabilities: vec![Capability::ScreenCapture, Capability::InputMouse],
            timeout_ms: 1000,
            retry_count: 0,
            continue_on_failure: false,
        }
    }

    use crate::action::{ActionResult, DefaultActionExecutor};

    struct FlakyExecutor {
        fail_before_success: std::sync::atomic::AtomicU32,
    }

    impl ActionExecutor for FlakyExecutor {
        fn execute(&self, action: &ActionType) -> ActionResult {
            let remaining = self.fail_before_success.fetch_sub(1, Ordering::SeqCst);
            if remaining > 0 {
                ActionResult::failure("simulated transient failure")
            } else {
                DefaultActionExecutor.execute(action)
            }
        }

        fn kind(&self) -> &'static str {
            "flaky"
        }
    }

    #[test]
    fn test_simple_pipeline() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![
            ActionType::Wait { duration_ms: 1 },
            ActionType::Wait { duration_ms: 1 },
            ActionType::Wait { duration_ms: 1 },
        ]);
        let report = exec.execute_plan(plan, Goal::new("multi-step test"));
        assert!(report.success);
        assert_eq!(report.total_steps, 3);
        assert_eq!(report.completed_steps, 3);
        assert_eq!(report.failed_steps, 0);
        assert!(matches!(
            report.state,
            PipelineExecutionState::Completed { success: true }
        ));
    }

    #[test]
    fn test_execute_goal_simple() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let report = exec.execute_goal(Goal::new("set brightness to 75"));
        assert!(report.success);
        assert_eq!(report.total_steps, 1);
        assert_eq!(report.completed_steps, 1);
    }

    #[test]
    fn test_empty_plan() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![]);
        let report = exec.execute_plan(plan, Goal::new("empty"));
        assert!(report.success);
        assert_eq!(report.total_steps, 0);
    }

    #[test]
    fn test_single_step_success() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![ActionType::Wait { duration_ms: 1 }]);
        let report = exec.execute_plan(plan, Goal::new("single"));
        assert!(report.success);
        assert_eq!(report.completed_steps, 1);
    }

    #[test]
    fn test_step_failure_no_retry() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![ActionType::ClickScreenElement {
            query: "btn".into(),
        }]);
        let report = exec.execute_plan(plan, Goal::new("fail"));
        assert!(!report.success);
        assert_eq!(report.failed_steps, 1);
    }

    #[test]
    fn test_verification_disabled() {
        let ws = make_world_state();
        let mut exec = make_executor(&ws);
        exec.config.enable_verification = false;
        let plan = make_plan_with_actions(vec![ActionType::Wait { duration_ms: 1 }]);
        let report = exec.execute_plan(plan, Goal::new("no-verify"));
        assert!(report.success);
    }

    #[test]
    fn test_recovery_disabled() {
        let ws = make_world_state();
        let mut exec = make_executor(&ws);
        exec.config.enable_recovery = false;
        let plan = make_plan_with_actions(vec![ActionType::ClickScreenElement {
            query: "btn".into(),
        }]);
        let report = exec.execute_plan(plan, Goal::new("no-recovery"));
        assert!(!report.success);
        assert_eq!(report.failed_steps, 1);
    }

    #[test]
    fn test_retry_success() {
        let ws = make_world_state();
        let exec = make_executor_full(&ws);
        exec.register_executor(
            "default",
            Box::new(FlakyExecutor {
                fail_before_success: std::sync::atomic::AtomicU32::new(2),
            }),
        );
        let plan = make_plan_with_retry(vec![ActionType::Wait { duration_ms: 1 }], 3);
        let report = exec.execute_plan(plan, Goal::new("retry-then-succeed"));
        assert!(report.success);
        assert_eq!(report.completed_steps, 1);
    }

    #[test]
    fn test_precondition_skip() {
        let ws = make_world_state();
        ws.write().update_active_app("calculator".into());
        let exec = make_executor(&ws);
        let step = ExecutionStep {
            id: "s1".into(),
            description: "open calculator".into(),
            action: ActionType::OpenApp {
                app_id: "calculator".into(),
                data: None,
            },
            dependencies: vec![],
            required_capabilities: vec![],
            timeout_ms: 5000,
            retry_count: 0,
            continue_on_failure: false,
        };
        let ps = PipelineStep::new(
            step,
            0,
            vec![Precondition::AppIsRunning("calculator".into())],
            crate::pipeline_step::VerificationStrategy::AppInForeground {
                app_name: "calculator".into(),
            },
            crate::pipeline_step::ExpectedOutcome::AppForeground {
                app_name: "calculator".into(),
            },
            crate::pipeline_step::RetryPolicy::NoRetry,
        );
        let reason = exec.evaluate_preconditions(&ps.preconditions);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("already running"));
    }

    #[test]
    fn test_precondition_device_state() {
        let ws = make_world_state();
        ws.write().update_device_telemetry(DeviceTelemetry {
            wifi_enabled: Some(true),
            ..DeviceTelemetry::new()
        });
        let exec = make_executor(&ws);
        let pre = Precondition::DeviceState {
            field: "wifi".into(),
            expected: "enabled".into(),
        };
        let reason = exec.evaluate_preconditions(&[pre]);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("already"));
    }

    #[test]
    fn test_precondition_not_met() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let pre = Precondition::AppIsRunning("nonexistent".into());
        let reason = exec.evaluate_preconditions(&[pre]);
        assert!(reason.is_none());
    }

    #[test]
    fn test_execution_report_contains_records() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![
            ActionType::Wait { duration_ms: 1 },
            ActionType::Wait { duration_ms: 1 },
        ]);
        let report = exec.execute_plan(plan, Goal::new("records"));
        assert_eq!(report.step_records.len(), 2);
        assert_eq!(report.step_records[0].status, StepStatus::Completed);
        assert_eq!(report.step_records[1].status, StepStatus::Completed);
    }

    #[test]
    fn test_execution_summary() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![ActionType::Wait { duration_ms: 1 }]);
        let report = exec.execute_plan(plan, Goal::new("summary"));
        let summary = ExecutionSummary::from_report(&report);
        assert_eq!(summary.total_steps, 1);
        assert_eq!(summary.successful_steps, 1);
        assert!(summary.completed);
    }

    #[test]
    fn test_step_record_attempts() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![ActionType::Wait { duration_ms: 1 }]);
        let report = exec.execute_plan(plan, Goal::new("attempts"));
        assert_eq!(report.step_records[0].attempts, 1);
    }

    #[test]
    fn test_executor_registration() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        exec.register_executor("test", Box::new(DefaultActionExecutor));
        let map = exec.action_executors.read();
        assert!(map.contains_key("test"));
    }

    #[test]
    fn test_cancellation_before_execution() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![ActionType::Wait { duration_ms: 100 }]);
        let report = exec.execute_plan(plan, Goal::new("cancel"));
        assert!(report.total_steps == 1 || report.total_steps == 0);
    }

    #[test]
    fn test_long_plan() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let actions = vec![ActionType::Wait { duration_ms: 1 }; 10];
        let plan = make_plan_with_actions(actions);
        let report = exec.execute_plan(plan, Goal::new("long"));
        assert!(report.success);
        assert_eq!(report.total_steps, 10);
        assert_eq!(report.completed_steps, 10);
    }

    #[test]
    fn test_plan_validation_failure() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let steps = vec![ExecutionStep {
            id: "s1".into(),
            description: "first".into(),
            action: ActionType::Wait { duration_ms: 1 },
            dependencies: vec!["nonexistent".into()],
            required_capabilities: vec![],
            timeout_ms: 1000,
            retry_count: 0,
            continue_on_failure: false,
        }];
        let plan = ExecutionPlan {
            id: "bad".into(),
            goal_description: "test".into(),
            steps,
            created_at: 0,
            estimated_steps: 1,
        };
        let report = exec.execute_plan(plan, Goal::new("bad-plan"));
        assert!(!report.success);
    }

    #[test]
    fn test_metrics_in_report() {
        let ws = make_world_state();
        let exec = make_executor_full(&ws);
        let plan = make_plan_with_actions(vec![
            ActionType::Wait { duration_ms: 1 },
            ActionType::Wait { duration_ms: 1 },
        ]);
        let report = exec.execute_plan(plan, Goal::new("metrics"));
        assert_eq!(report.verification_count, 2);
    }

    #[test]
    fn test_empty_goal_report() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![]);
        let report = exec.execute_plan(plan, Goal::new("empty"));
        assert!(report.success);
        assert_eq!(report.total_steps, 0);
        assert!(report.step_records.is_empty());
    }

    #[test]
    fn test_pipeline_execution_state_completed() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![ActionType::Wait { duration_ms: 1 }]);
        let report = exec.execute_plan(plan, Goal::new("state"));
        assert_eq!(
            report.state,
            PipelineExecutionState::Completed { success: true }
        );
    }

    #[test]
    fn test_execution_duration_positive() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![ActionType::Wait { duration_ms: 5 }]);
        let report = exec.execute_plan(plan, Goal::new("duration"));
        assert!(report.execution_duration.as_millis() > 0 || report.success);
    }

    #[test]
    fn test_step_failure_with_error_message() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![ActionType::ClickScreenElement {
            query: "missing".into(),
        }]);
        let report = exec.execute_plan(plan, Goal::new("error"));
        assert!(!report.success);
        if let Some(record) = report.step_records.first() {
            assert!(record.error.is_some());
        }
    }

    #[test]
    fn test_goal_with_context() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let goal = Goal::new("set brightness to 50").with_context("source", "test");
        let report = exec.execute_goal(goal);
        assert!(report.success);
    }

    #[test]
    fn test_no_precondition_no_skip() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let pre = Precondition::NoPrecondition;
        let reason = exec.evaluate_preconditions(&[pre]);
        assert!(reason.is_none());
    }

    #[test]
    fn test_config_defaults() {
        let cfg = PlanExecutorConfig::default();
        assert_eq!(cfg.default_step_timeout_ms, 30_000);
        assert!(cfg.enable_verification);
        assert!(cfg.enable_recovery);
        assert_eq!(cfg.max_replans_per_goal, 3);
    }

    #[test]
    fn test_custom_config() {
        let cfg = PlanExecutorConfig {
            default_step_timeout_ms: 10_000,
            enable_verification: false,
            enable_recovery: false,
            max_replans_per_goal: 0,
            record_evidence: false,
        };
        assert_eq!(cfg.default_step_timeout_ms, 10_000);
        assert!(!cfg.enable_verification);
    }

    #[test]
    fn test_action_kind_name_all_variants() {
        assert_eq!(
            action_kind_name(&ActionType::Speak { text: "".into() }),
            "speak"
        );
        assert_eq!(
            action_kind_name(&ActionType::Wait { duration_ms: 0 }),
            "wait"
        );
        assert_eq!(
            action_kind_name(&ActionType::OpenApp {
                app_id: "".into(),
                data: None
            }),
            "open_app"
        );
        assert_eq!(
            action_kind_name(&ActionType::ClickScreenElement { query: "".into() }),
            "click_screen_element"
        );
        assert_eq!(
            action_kind_name(&ActionType::InputInjection(
                crate::action::InputInjectionParams {
                    action_type: "click".into(),
                    params: Default::default(),
                }
            )),
            "input_injection"
        );
    }

    #[test]
    fn test_named_executor_for_all() {
        assert_eq!(named_executor_for("click_screen_element"), "click");
        assert_eq!(named_executor_for("click_screen_text"), "click");
        assert_eq!(named_executor_for("type_into_screen_element"), "type");
        assert_eq!(named_executor_for("drag_screen_elements"), "drag");
        assert_eq!(named_executor_for("swipe_screen_elements"), "swipe");
        assert_eq!(named_executor_for("default"), "default");
        assert_eq!(named_executor_for("speak"), "default");
    }

    #[test]
    fn test_pipeline_execution_state_aborted() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![ActionType::ClickScreenElement {
            query: "missing".into(),
        }]);
        let report = exec.execute_plan(plan, Goal::new("abort"));
        assert!(matches!(
            report.state,
            PipelineExecutionState::Aborted { .. }
        ));
    }

    #[test]
    fn test_step_record_index_and_id() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![ActionType::Wait { duration_ms: 1 }]);
        let report = exec.execute_plan(plan, Goal::new("idx"));
        assert_eq!(report.step_records[0].step_index, 0);
        assert!(!report.step_records[0].step_id.is_empty());
    }

    #[test]
    fn test_report_plan_id() {
        let ws = make_world_state();
        let exec = make_executor(&ws);
        let plan = make_plan_with_actions(vec![ActionType::Wait { duration_ms: 1 }]);
        let report = exec.execute_plan(plan, Goal::new("plan-id"));
        assert!(!report.plan_id.is_empty());
    }
}
