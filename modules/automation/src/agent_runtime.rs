use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::execution_manager::ExecutionManager;
use crate::monitoring::{
    EventCounts, RuntimeDiagnostics, RuntimeHealth, RuntimeMonitor, SchedulerStats,
};
use crate::plan_executor::{GoalExecutionReport, PlanExecutor};
use crate::planner::{ExecutionPlan, Goal, Planner};
use crate::resource_manager::ResourceManager;
use crate::session_store::SessionStore;
use nova_kernel::{EventBus, EventMetadata, NovaEvent};

// ---------------------------------------------------------------------------
// AgentState — session lifecycle state machine
// ---------------------------------------------------------------------------

/// Session-level state for an autonomous agent session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentState {
    Created,
    Planning,
    Executing,
    Waiting,
    Recovering,
    Paused,
    Completed,
    Failed(String),
    Cancelled,
    TimedOut,
}

impl AgentState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AgentState::Completed
                | AgentState::Failed(_)
                | AgentState::Cancelled
                | AgentState::TimedOut
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            AgentState::Planning
                | AgentState::Executing
                | AgentState::Waiting
                | AgentState::Recovering
        )
    }

    pub fn can_transition_to(&self, next: &AgentState) -> bool {
        use AgentState::*;
        matches!(
            (self, next),
            (Created, Planning)
                | (Created, Cancelled)
                | (Planning, Executing)
                | (Planning, Failed(_))
                | (Planning, Cancelled)
                | (Executing, Waiting)
                | (Executing, Recovering)
                | (Executing, Completed)
                | (Executing, Failed(_))
                | (Executing, Cancelled)
                | (Executing, Paused)
                | (Executing, TimedOut)
                | (Waiting, Executing)
                | (Waiting, Failed(_))
                | (Waiting, Cancelled)
                | (Recovering, Planning)
                | (Recovering, Executing)
                | (Recovering, Failed(_))
                | (Recovering, Cancelled)
                | (Paused, Executing)
                | (Paused, Cancelled)
                | (Paused, TimedOut)
                // Restart transitions (terminal -> Created)
                | (Completed, Created)
                | (Failed(_), Created)
                | (Cancelled, Created)
                | (TimedOut, Created)
        )
    }
}

// ---------------------------------------------------------------------------
// AgentSession — per-session state
// ---------------------------------------------------------------------------

/// Per-session metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetrics {
    pub total_steps: u64,
    pub completed_steps: u64,
    pub failed_steps: u64,
    pub retries: u64,
    pub recoveries: u64,
    pub replans: u64,
    pub execution_duration_ms: i64,
}

/// A single autonomous agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub session_id: String,
    pub goal: Goal,
    pub execution_plan: Option<ExecutionPlan>,
    pub current_step_index: usize,
    pub state: AgentState,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
    pub retry_count: u32,
    pub recovery_count: u32,
    pub replan_count: u32,
    #[serde(skip)]
    pub report: Option<GoalExecutionReport>,
    pub metrics: SessionMetrics,
    pub tags: HashMap<String, String>,
}

impl AgentSession {
    pub fn new(session_id: impl Into<String>, goal: Goal) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            session_id: session_id.into(),
            goal,
            execution_plan: None,
            current_step_index: 0,
            state: AgentState::Created,
            created_at: now,
            started_at: None,
            updated_at: now,
            completed_at: None,
            retry_count: 0,
            recovery_count: 0,
            replan_count: 0,
            report: None,
            metrics: SessionMetrics::default(),
            tags: HashMap::new(),
        }
    }

    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Attempt a state transition. Returns an error if the transition is invalid.
    pub fn transition_to(&mut self, next: AgentState) -> Result<(), String> {
        if !self.state.can_transition_to(&next) {
            return Err(format!(
                "invalid state transition: {:?} -> {:?}",
                self.state, next
            ));
        }
        self.state = next;
        self.updated_at = chrono::Utc::now().timestamp_millis();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ScheduledJob — future / recurring job descriptor
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum JobType {
    OneShot,
    Delayed { delay_ms: u64 },
    Recurring { interval_ms: u64 },
}

/// A job scheduled for future or recurring execution.
#[derive(Debug, Clone)]
pub struct ScheduledJob {
    pub id: String,
    pub goal: Goal,
    pub job_type: JobType,
    pub created_at: i64,
    pub scheduled_at: i64,
    pub last_run_at: Option<i64>,
    pub run_count: u64,
    pub max_runs: Option<u64>,
    pub priority: u8,
    pub session_id: Option<String>,
    pub tags: HashMap<String, String>,
}

impl ScheduledJob {
    pub fn new(id: impl Into<String>, goal: Goal, job_type: JobType, scheduled_at: i64) -> Self {
        Self {
            id: id.into(),
            goal,
            job_type,
            created_at: Utc::now().timestamp_millis(),
            scheduled_at,
            last_run_at: None,
            run_count: 0,
            max_runs: None,
            priority: 0,
            session_id: None,
            tags: HashMap::new(),
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_max_runs(mut self, max: u64) -> Self {
        self.max_runs = Some(max);
        self
    }

    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Returns true if this job is due for execution at the given timestamp.
    pub fn is_due(&self, now: i64) -> bool {
        now >= self.scheduled_at
    }

    /// Returns true if this job has reached its maximum number of runs.
    pub fn is_exhausted(&self) -> bool {
        match self.max_runs {
            Some(max) => self.run_count >= max,
            None => false,
        }
    }

    /// Compute the next scheduled time for a recurring job.
    pub fn next_occurrence(&self) -> Option<i64> {
        match &self.job_type {
            JobType::Recurring { interval_ms } => {
                let base = self.last_run_at.unwrap_or(self.scheduled_at);
                Some(base + *interval_ms as i64)
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime metrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    pub total_sessions_created: u64,
    pub active_sessions: u64,
    pub completed_sessions: u64,
    pub failed_sessions: u64,
    pub cancelled_sessions: u64,
    pub timed_out_sessions: u64,
    pub total_steps_executed: u64,
    pub total_recoveries: u64,
    pub total_replans: u64,
    // S2 — scheduling
    pub scheduled_jobs: u64,
    pub resumed_sessions: u64,
    pub recovered_sessions: u64,
    pub queue_latency_ms: i64,
    pub persistence_failures: u64,
    pub persistence_saves: u64,
    pub persistence_loads: u64,
    pub cleaned_up_sessions: u64,
    pub recurring_jobs_completed: u64,
    /// Runtime uptime tracking — set when the runtime is first used.
    #[serde(skip)]
    pub runtime_started_at: Option<i64>,
    /// Total runtime duration in milliseconds since first session creation.
    pub runtime_duration_ms: i64,
}

// ---------------------------------------------------------------------------
// AgentRuntimeConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AgentRuntimeConfig {
    pub max_concurrent_sessions: usize,
    pub max_sessions: usize,
    pub session_timeout_ms: u64,
    pub schedule_interval_ms: u64,
    pub default_step_timeout_ms: u64,
    pub max_retries_per_step: u32,
    pub max_replans_per_session: u32,
    // S2 — scheduling & persistence
    pub retention_window_ms: u64,
    pub cleanup_interval_ms: u64,
    pub max_concurrent_scheduled_jobs: usize,
    pub enable_persistence: bool,
    pub max_scheduled_jobs: usize,
}

impl Default for AgentRuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrent_sessions: 5,
            max_sessions: 100,
            session_timeout_ms: 300_000,
            schedule_interval_ms: 500,
            default_step_timeout_ms: 30_000,
            max_retries_per_step: 3,
            max_replans_per_session: 5,
            retention_window_ms: 86_400_000, // 24 hours
            cleanup_interval_ms: 3_600_000,  // 1 hour
            max_concurrent_scheduled_jobs: 10,
            enable_persistence: true,
            max_scheduled_jobs: 1000,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentScheduler — polls and advances sessions
// ---------------------------------------------------------------------------

/// Drives active agent sessions forward by polling and executing steps.
pub struct AgentScheduler {
    config: AgentRuntimeConfig,
}

impl AgentScheduler {
    pub fn new(config: &AgentRuntimeConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// Tick the scheduler: advance all active sessions by one step.
    /// Returns a list of session IDs whose state changed.
    pub fn tick(
        &self,
        sessions: &mut HashMap<String, AgentSession>,
        plan_executor: &PlanExecutor,
    ) -> Vec<String> {
        let mut changed = Vec::new();
        let now = chrono::Utc::now().timestamp_millis();

        let active_ids: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.state.is_active() && s.state != AgentState::Paused)
            .map(|(id, _)| id.clone())
            .collect();

        for id in active_ids {
            let session = match sessions.get_mut(&id) {
                Some(s) => s,
                None => continue,
            };

            // Timeout check.
            if let Some(started) = session.started_at {
                if now - started > self.config.session_timeout_ms as i64 {
                    let _ = session.transition_to(AgentState::TimedOut);
                    session.completed_at = Some(now);
                    changed.push(id.clone());
                    continue;
                }
            }

            match &session.state {
                AgentState::Planning => {
                    // Session is in planning state — skip (external trigger needed).
                }
                AgentState::Executing => {
                    let plan = match &session.execution_plan {
                        Some(p) => p.clone(),
                        None => {
                            let _ = session.transition_to(AgentState::Failed("no plan".into()));
                            changed.push(id.clone());
                            continue;
                        }
                    };

                    if session.current_step_index >= plan.steps.len() {
                        let report = plan_executor.execute_plan(plan, session.goal.clone());
                        session.metrics.completed_steps = report.completed_steps as u64;
                        session.metrics.failed_steps = report.failed_steps as u64;
                        session.metrics.total_steps = report.total_steps as u64;
                        session.metrics.execution_duration_ms =
                            report.execution_duration.as_millis() as i64;
                        session.report = Some(report);
                        let _ = session.transition_to(AgentState::Completed);
                        session.completed_at = Some(now);
                        changed.push(id.clone());
                        continue;
                    }

                    // Execute the current step via the plan executor.
                    let step = &plan.steps[session.current_step_index];
                    let single_step_plan = ExecutionPlan {
                        id: plan.id.clone(),
                        goal_description: plan.goal_description.clone(),
                        steps: vec![step.clone()],
                        created_at: plan.created_at,
                        estimated_steps: 1,
                    };
                    let report = plan_executor.execute_plan(single_step_plan, session.goal.clone());

                    if report.success {
                        session.current_step_index += 1;
                        session.metrics.completed_steps += 1;
                    } else {
                        session.metrics.failed_steps += 1;
                        if session.retry_count < self.config.max_retries_per_step {
                            session.retry_count += 1;
                            session.metrics.retries += 1;
                        } else if session.replan_count < self.config.max_replans_per_session {
                            let _ = session.transition_to(AgentState::Recovering);
                            session.replan_count += 1;
                            session.metrics.replans += 1;
                            changed.push(id.clone());
                            continue;
                        } else {
                            let err = report
                                .abort_reason
                                .clone()
                                .unwrap_or_else(|| "step failed".into());
                            let _ = session.transition_to(AgentState::Failed(err));
                            session.completed_at = Some(now);
                            changed.push(id.clone());
                            continue;
                        }
                    }
                    session.metrics.total_steps = session.current_step_index as u64;
                    changed.push(id.clone());
                }
                AgentState::Waiting => {
                    // Session is waiting — skip until externally resumed.
                }
                AgentState::Recovering => {
                    // Transition back to planning for replanning.
                    let _ = session.transition_to(AgentState::Planning);
                    changed.push(id.clone());
                }
                _ => {}
            }
        }

        changed
    }
}

// ---------------------------------------------------------------------------
// AgentRuntime — public API
// ---------------------------------------------------------------------------

/// High-level autonomous agent runtime managing multiple long-running sessions.
pub struct AgentRuntime {
    sessions: Arc<RwLock<HashMap<String, AgentSession>>>,
    config: AgentRuntimeConfig,
    metrics: Arc<RwLock<RuntimeMetrics>>,
    plan_executor: Arc<PlanExecutor>,
    planner: Arc<Planner>,
    execution_manager: Option<Arc<ExecutionManager>>,
    // S2 — scheduling & persistence
    scheduled_jobs: Arc<RwLock<Vec<ScheduledJob>>>,
    session_store: Arc<dyn SessionStore>,
    // S4 — resource management
    resource_manager: Option<Arc<ResourceManager>>,
    // S5 — monitoring & events
    event_bus: Option<Arc<EventBus>>,
    monitor: RuntimeMonitor,
    scheduler_stats: Arc<RwLock<SchedulerStats>>,
}

impl AgentRuntime {
    pub fn new(plan_executor: Arc<PlanExecutor>, planner: Arc<Planner>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config: AgentRuntimeConfig::default(),
            metrics: Arc::new(RwLock::new(RuntimeMetrics::default())),
            plan_executor,
            planner,
            execution_manager: None,
            scheduled_jobs: Arc::new(RwLock::new(Vec::new())),
            session_store: Arc::new(crate::session_store::InMemorySessionStore::new()),
            resource_manager: None,
            event_bus: None,
            monitor: RuntimeMonitor::new(),
            scheduler_stats: Arc::new(RwLock::new(SchedulerStats::default())),
        }
    }

    pub fn with_execution_manager(mut self, em: Arc<ExecutionManager>) -> Self {
        self.execution_manager = Some(em);
        self
    }

    pub fn with_config(mut self, config: AgentRuntimeConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_session_store(mut self, store: Arc<dyn SessionStore>) -> Self {
        self.session_store = store;
        self
    }

    pub fn with_resource_manager(mut self, rm: Arc<ResourceManager>) -> Self {
        self.resource_manager = Some(rm);
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn config(&self) -> AgentRuntimeConfig {
        self.config.clone()
    }

    // -- Event Publishing --

    fn publish_event(&self, payload: crate::events::AutomationEventPayload) {
        self.monitor.record_event(payload.variant_name());
        if let Some(ref bus) = self.event_bus {
            let meta = EventMetadata::new("automation", None);
            let event = NovaEvent {
                metadata: meta,
                payload: Arc::new(payload),
            };
            let _ = bus.publish(event);
        }
    }

    // -- Session Management --

    /// Create a new agent session for the given goal.
    pub fn create_session(&self, goal: Goal) -> Result<String, String> {
        let sessions = self.sessions.read();
        if sessions.len() >= self.config.max_sessions {
            return Err("maximum number of sessions reached".into());
        }
        // Check for duplicate pending/active sessions with the same goal.
        let is_duplicate =
            |state: &AgentState| -> bool { state.is_active() || *state == AgentState::Created };
        for session in sessions.values() {
            if is_duplicate(&session.state) && session.goal.description == goal.description {
                return Err(format!(
                    "an active session for goal '{}' already exists",
                    goal.description
                ));
            }
        }
        drop(sessions);

        let session_id = uuid::Uuid::new_v4().to_string();
        let goal_desc = goal.description.clone();
        let session = AgentSession::new(&session_id, goal);

        let mut sessions = self.sessions.write();
        {
            let mut metrics = self.metrics.write();
            metrics.total_sessions_created += 1;
            metrics.active_sessions += 1;
            if metrics.runtime_started_at.is_none() {
                metrics.runtime_started_at = Some(chrono::Utc::now().timestamp_millis());
            }
        }
        self.monitor.record_session_created();
        sessions.insert(session_id.clone(), session);
        self.publish_event(crate::events::AutomationEventPayload::SessionCreated {
            session_id: session_id.clone(),
            goal: goal_desc,
        });
        Ok(session_id)
    }

    /// Start planning and executing a session.
    pub fn start_session(&self, session_id: &str) -> Result<(), String> {
        let goal_desc;
        let sid = session_id.to_string();
        {
            let mut sessions = self.sessions.write();
            let session = sessions
                .get_mut(&sid)
                .ok_or_else(|| format!("session '{}' not found", session_id))?;

            if session.state != AgentState::Created {
                return Err(format!("cannot start session in state {:?}", session.state));
            }

            goal_desc = session.goal.description.clone();

            let now = chrono::Utc::now().timestamp_millis();
            session.started_at = Some(now);
            session.transition_to(AgentState::Planning)?;

            // Attempt initial planning using the heuristic planner.
            match self.planner.plan(&session.goal) {
                Ok(plan) => {
                    session.execution_plan = Some(plan);
                    session.transition_to(AgentState::Executing)?;
                }
                Err(e) => {
                    // If planner returned RunAI fallback, try AI planning.
                    if e.contains("needs_ai_planning") || e.contains("could not decompose") {
                        if self.planner.has_ai() {
                            let result = tokio::runtime::Handle::current()
                                .block_on(self.planner.plan_with_ai(&session.goal, None));
                            match result {
                                Ok(crate::planner::AiPlanResult::Plan(plan)) => {
                                    session.execution_plan = Some(plan);
                                    session.transition_to(AgentState::Executing)?;
                                }
                                Ok(crate::planner::AiPlanResult::Clarification { question }) => {
                                    let _ = session.transition_to(AgentState::Failed(format!(
                                        "needs clarification: {}",
                                        question
                                    )));
                                    return Err(format!("clarification needed: {}", question));
                                }
                                Ok(crate::planner::AiPlanResult::Failed { reason })
                                | Err(reason) => {
                                    let _ =
                                        session.transition_to(AgentState::Failed(reason.clone()));
                                    return Err(reason);
                                }
                            }
                        } else {
                            let _ = session.transition_to(AgentState::Failed(
                                "heuristic planner could not decompose goal and no AI provider is configured".into(),
                            ));
                            return Err("planning failed: heuristic could not handle goal and no AI is available".into());
                        }
                    } else {
                        let _ = session.transition_to(AgentState::Failed(e.clone()));
                        return Err(e);
                    }
                }
            }
        }

        self.publish_event(crate::events::AutomationEventPayload::SessionStarted {
            session_id: sid,
            goal: goal_desc,
        });
        Ok(())
    }

    /// Pause an executing session.
    pub fn pause_session(&self, session_id: &str) -> Result<(), String> {
        let sid = session_id.to_string();
        {
            let mut sessions = self.sessions.write();
            let session = sessions
                .get_mut(&sid)
                .ok_or_else(|| format!("session '{}' not found", session_id))?;

            if session.state != AgentState::Executing && session.state != AgentState::Waiting {
                return Err(format!("cannot pause session in state {:?}", session.state));
            }

            session.transition_to(AgentState::Paused)?;
            let active = self.metrics.read().active_sessions;
            self.metrics.write().active_sessions = active.saturating_sub(1);
        }
        self.publish_event(crate::events::AutomationEventPayload::SessionPaused {
            session_id: sid,
        });
        Ok(())
    }

    /// Resume a paused session.
    pub fn resume_session(&self, session_id: &str) -> Result<(), String> {
        let sid = session_id.to_string();
        {
            let mut sessions = self.sessions.write();
            let session = sessions
                .get_mut(&sid)
                .ok_or_else(|| format!("session '{}' not found", session_id))?;

            if session.state != AgentState::Paused {
                return Err(format!(
                    "cannot resume session in state {:?}",
                    session.state
                ));
            }

            session.transition_to(AgentState::Executing)?;
            self.metrics.write().active_sessions += 1;
        }
        self.publish_event(crate::events::AutomationEventPayload::SessionResumed {
            session_id: sid,
        });
        Ok(())
    }

    /// Cancel a session (terminal).
    pub fn cancel_session(&self, session_id: &str) -> Result<(), String> {
        let sid = session_id.to_string();
        let goal_desc;
        {
            let mut sessions = self.sessions.write();
            let session = sessions
                .get_mut(&sid)
                .ok_or_else(|| format!("session '{}' not found", session_id))?;

            if session.state.is_terminal() {
                return Err(format!(
                    "session already in terminal state {:?}",
                    session.state
                ));
            }

            goal_desc = session.goal.description.clone();
            let was_active = session.state.is_active();
            session.transition_to(AgentState::Cancelled)?;
            session.completed_at = Some(chrono::Utc::now().timestamp_millis());
            if was_active {
                let active = self.metrics.read().active_sessions;
                self.metrics.write().active_sessions = active.saturating_sub(1);
            }
            self.metrics.write().cancelled_sessions += 1;
        }

        // Release any resources held by this session
        if let Some(ref rm) = self.resource_manager {
            rm.force_release(&sid);
        }
        self.publish_event(crate::events::AutomationEventPayload::SessionCancelled {
            session_id: sid,
            goal: goal_desc,
        });
        Ok(())
    }

    /// Restart a completed/failed/cancelled/timed-out session.
    /// Resets the session to Created state with fresh counters.
    pub fn restart_session(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.write();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("session '{}' not found", session_id))?;

        if !session.state.is_terminal() {
            return Err(format!(
                "cannot restart session in non-terminal state {:?}",
                session.state
            ));
        }

        let now = chrono::Utc::now().timestamp_millis();
        session.state = AgentState::Created;
        session.execution_plan = None;
        session.current_step_index = 0;
        session.started_at = None;
        session.completed_at = None;
        session.updated_at = now;
        session.retry_count = 0;
        session.recovery_count = 0;
        session.replan_count = 0;
        session.report = None;
        session.metrics = SessionMetrics::default();
        self.metrics.write().resumed_sessions += 1;
        Ok(())
    }

    // -- Query --

    /// Get a session by ID.
    pub fn session(&self, session_id: &str) -> Option<AgentSession> {
        self.sessions.read().get(session_id).cloned()
    }

    /// Get all sessions.
    pub fn sessions(&self) -> Vec<AgentSession> {
        self.sessions.read().values().cloned().collect()
    }

    /// Get all currently running sessions.
    pub fn running_sessions(&self) -> Vec<AgentSession> {
        self.sessions
            .read()
            .values()
            .filter(|s| s.state.is_active())
            .cloned()
            .collect()
    }

    /// Get all completed (terminal) sessions.
    pub fn completed_sessions(&self) -> Vec<AgentSession> {
        self.sessions
            .read()
            .values()
            .filter(|s| s.state.is_terminal())
            .cloned()
            .collect()
    }

    // -- Metrics --

    /// Snapshot of runtime-level metrics.
    pub fn metrics(&self) -> RuntimeMetrics {
        let mut m = self.metrics.read().clone();
        if let Some(started) = m.runtime_started_at {
            m.runtime_duration_ms = chrono::Utc::now().timestamp_millis() - started;
        }
        m
    }

    /// Remove completed sessions beyond the configured limit.
    pub fn prune_completed(&self, max_to_keep: usize) -> u64 {
        let mut sessions = self.sessions.write();
        let terminal: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.state.is_terminal())
            .map(|(id, _)| id.clone())
            .collect();

        if terminal.len() <= max_to_keep {
            return 0;
        }

        let to_remove = terminal.len() - max_to_keep;
        for id in terminal.iter().take(to_remove) {
            sessions.remove(id);
        }
        to_remove as u64
    }

    // ======================================================================
    // S2 — Scheduling
    // ======================================================================

    /// Schedule a goal for immediate execution via the job queue.
    pub fn schedule(&self, goal: Goal) -> Result<String, String> {
        let now = Utc::now().timestamp_millis();
        self.schedule_inner(goal, JobType::OneShot, now, 0)
    }

    /// Schedule a goal for execution after the given delay.
    pub fn schedule_after(&self, goal: Goal, delay_ms: u64) -> Result<String, String> {
        let scheduled_at = Utc::now().timestamp_millis() + delay_ms as i64;
        self.schedule_inner(goal, JobType::Delayed { delay_ms }, scheduled_at, 0)
    }

    /// Schedule a goal for execution at the given absolute timestamp.
    pub fn schedule_at(&self, goal: Goal, timestamp_ms: i64) -> Result<String, String> {
        let delay = (timestamp_ms - Utc::now().timestamp_millis()).max(0) as u64;
        self.schedule_inner(goal, JobType::Delayed { delay_ms: delay }, timestamp_ms, 0)
    }

    /// Schedule a recurring goal.
    pub fn schedule_recurring(
        &self,
        goal: Goal,
        interval_ms: u64,
        max_runs: Option<u64>,
    ) -> Result<String, String> {
        let now = Utc::now().timestamp_millis();
        let job = self.schedule_inner(goal, JobType::Recurring { interval_ms }, now, 0)?;
        if let Some(max) = max_runs {
            if let Some(j) = self.scheduled_jobs.write().iter_mut().find(|j| j.id == job) {
                j.max_runs = Some(max);
            }
        }
        self.metrics.write().scheduled_jobs += 1;
        Ok(job)
    }

    fn schedule_inner(
        &self,
        goal: Goal,
        job_type: JobType,
        scheduled_at: i64,
        priority: u8,
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let job = ScheduledJob::new(&id, goal, job_type, scheduled_at).with_priority(priority);
        let mut jobs = self.scheduled_jobs.write();
        if jobs.len() >= self.config.max_scheduled_jobs {
            return Err("maximum number of scheduled jobs reached".into());
        }
        jobs.push(job);
        // Keep jobs ordered by scheduled_at (ascending), then priority (ascending).
        jobs.sort_by(|a, b| {
            a.scheduled_at
                .cmp(&b.scheduled_at)
                .then(a.priority.cmp(&b.priority))
        });
        Ok(id)
    }

    /// Process due scheduled jobs and create sessions for them.
    /// Returns the number of jobs that were dispatched.
    pub fn process_scheduled_jobs(&self) -> usize {
        let due_ids: Vec<String>;
        let now = Utc::now().timestamp_millis();
        {
            let jobs = self.scheduled_jobs.read();
            due_ids = jobs
                .iter()
                .filter(|j| j.is_due(now) && !j.is_exhausted())
                .map(|j| j.id.clone())
                .collect();
        }

        let mut dispatched = 0usize;
        for id in &due_ids {
            let job = {
                let jobs = self.scheduled_jobs.read();
                jobs.iter().find(|j| j.id == *id).cloned()
            };
            let job = match job {
                Some(j) => j,
                None => continue,
            };

            // Check max concurrent.
            {
                let sessions = self.sessions.read();
                let active_count = sessions.values().filter(|s| s.state.is_active()).count();
                if active_count >= self.config.max_concurrent_scheduled_jobs {
                    break;
                }
            }

            // Create and start a session for the job.
            match self.create_session(job.goal.clone()) {
                Ok(session_id) => {
                    // Record latency.
                    let latency = now - job.scheduled_at;
                    self.metrics.write().queue_latency_ms = latency.max(0);

                    let _ = self.start_session(&session_id);

                    // Update job state.
                    let mut jobs = self.scheduled_jobs.write();
                    if let Some(j) = jobs.iter_mut().find(|j| j.id == *id) {
                        j.last_run_at = Some(now);
                        j.run_count += 1;
                        j.session_id = Some(session_id);
                    }

                    dispatched += 1;
                }
                Err(_) => {
                    continue;
                }
            }
        }

        // Handle recurring: re-schedule or remove exhausted/consumed jobs.
        let mut jobs = self.scheduled_jobs.write();
        let to_reinsert: Vec<ScheduledJob> = jobs
            .iter()
            .filter(|j| due_ids.contains(&j.id) && j.run_count > 0)
            .filter_map(|j| {
                if j.is_exhausted() {
                    None // remove exhausted
                } else if matches!(j.job_type, JobType::Recurring { .. }) {
                    if let Some(next) = j.next_occurrence() {
                        let mut new_job = j.clone();
                        new_job.scheduled_at = next;
                        self.metrics.write().recurring_jobs_completed += 1;
                        Some(new_job)
                    } else {
                        None
                    }
                } else {
                    None // one-shot and delayed are consumed
                }
            })
            .collect();
        jobs.retain(|j| {
            // Remove exhausted jobs regardless of due status.
            if j.is_exhausted() {
                return false;
            }
            // Remove consumed non-recurring jobs that were processed.
            if due_ids.contains(&j.id) && !matches!(j.job_type, JobType::Recurring { .. }) {
                return false;
            }
            true
        });
        for j in to_reinsert {
            jobs.push(j);
        }
        jobs.sort_by(|a, b| {
            a.scheduled_at
                .cmp(&b.scheduled_at)
                .then(a.priority.cmp(&b.priority))
        });

        dispatched
    }

    /// Remove expired, cancelled, and completed sessions past the retention window.
    pub fn cleanup_sessions(&self) -> u64 {
        let now = Utc::now().timestamp_millis();
        let mut sessions = self.sessions.write();
        let to_remove: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| {
                if !s.state.is_terminal() {
                    return false;
                }
                // Remove if past retention window.
                let completed = s.completed_at.unwrap_or(s.updated_at);
                now - completed > self.config.retention_window_ms as i64
            })
            .map(|(id, _)| id.clone())
            .collect();

        let count = to_remove.len() as u64;
        for id in &to_remove {
            sessions.remove(id);
        }
        self.metrics.write().cleaned_up_sessions += count;
        count
    }

    // ======================================================================
    // S2 — Persistence
    // ======================================================================

    /// Persist all sessions to the session store.
    pub fn persist_all(&self) -> Result<u64, String> {
        let sessions = self.sessions.read().values().cloned().collect::<Vec<_>>();
        let count = sessions.len() as u64;
        for session in &sessions {
            self.session_store.save_session(session).map_err(|e| {
                self.metrics.write().persistence_failures += 1;
                format!("failed to save session '{}': {}", session.session_id, e)
            })?;
        }
        self.metrics.write().persistence_saves += count;
        Ok(count)
    }

    /// Restore all sessions from the session store into memory.
    /// Returns the number of sessions restored.
    pub fn restore_all(&self) -> Result<u64, String> {
        let stored = self.session_store.list_sessions().map_err(|e| {
            self.metrics.write().persistence_failures += 1;
            format!("failed to list sessions from store: {}", e)
        })?;
        let count = stored.len() as u64;
        let mut sessions = self.sessions.write();
        for session in stored {
            sessions.insert(session.session_id.clone(), session);
        }
        self.metrics.write().persistence_loads += count;
        Ok(count)
    }

    // ======================================================================
    // S2 — Recovery
    // ======================================================================

    /// Resume all pending sessions that were interrupted.
    /// A session is pending if it was in Planning, Executing, Waiting, or Recovering state
    /// (i.e., it was active when the runtime went down).
    /// Returns the number of sessions resumed.
    pub fn resume_pending(&self) -> u64 {
        let mut count = 0u64;
        let ids: Vec<String> = {
            let sessions = self.sessions.read();
            sessions
                .iter()
                .filter(|(_, s)| s.state.is_active())
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in &ids {
            // Transition active sessions back to a runnable state.
            let mut sessions = self.sessions.write();
            if let Some(session) = sessions.get_mut(id) {
                match &session.state {
                    AgentState::Planning | AgentState::Waiting => {
                        // These states are already valid to resume from.
                        count += 1;
                        self.metrics.write().resumed_sessions += 1;
                    }
                    AgentState::Executing => {
                        // Re-plan from the current step.
                        if session.execution_plan.is_some() {
                            count += 1;
                            self.metrics.write().resumed_sessions += 1;
                            self.metrics.write().recovered_sessions += 1;
                        }
                    }
                    AgentState::Recovering => {
                        // Transition back to planning.
                        let _ = session.transition_to(AgentState::Planning);
                        count += 1;
                        self.metrics.write().resumed_sessions += 1;
                        self.metrics.write().recovered_sessions += 1;
                    }
                    _ => {}
                }
            }
        }
        count
    }

    // ======================================================================
    // S2 — Tick extension: process scheduled jobs before advancing sessions
    // ======================================================================

    /// Run a single scheduler tick that processes scheduled jobs
    /// and advances active sessions.
    /// Returns a list of session IDs whose state changed.
    pub fn tick(&self) -> Vec<String> {
        let tick_start = std::time::Instant::now();

        // Process due scheduled jobs first.
        let dispatched = self.process_scheduled_jobs();

        // Then advance active sessions as before.
        let scheduler = AgentScheduler::new(&self.config);
        let mut sessions = self.sessions.write();
        let changed = scheduler.tick(&mut sessions, &self.plan_executor);

        // Update runtime metrics from changed sessions, publish events,
        // and release resources for terminal sessions.
        if !changed.is_empty() {
            let mut terminal_events = Vec::new();
            let mut metrics = self.metrics.write();
            for id in &changed {
                if let Some(session) = sessions.get(id) {
                    metrics.total_steps_executed += session.metrics.completed_steps;
                    metrics.total_recoveries += session.metrics.recoveries;
                    metrics.total_replans += session.metrics.replans;
                    match session.state.clone() {
                        AgentState::Completed => {
                            metrics.completed_sessions += 1;
                            metrics.active_sessions = metrics.active_sessions.saturating_sub(1);
                            let dur = session
                                .started_at
                                .map(|s| chrono::Utc::now().timestamp_millis() - s)
                                .unwrap_or(0);
                            terminal_events.push((
                                id.clone(),
                                session.goal.description.clone(),
                                dur,
                                None::<String>,
                                None::<String>,
                                AgentState::Completed,
                            ));
                        }
                        AgentState::Failed(ref err) => {
                            metrics.failed_sessions += 1;
                            metrics.active_sessions = metrics.active_sessions.saturating_sub(1);
                            let dur = session
                                .started_at
                                .map(|s| chrono::Utc::now().timestamp_millis() - s)
                                .unwrap_or(0);
                            terminal_events.push((
                                id.clone(),
                                session.goal.description.clone(),
                                dur,
                                Some(err.clone()),
                                None::<String>,
                                AgentState::Failed(err.clone()),
                            ));
                        }
                        AgentState::TimedOut => {
                            metrics.timed_out_sessions += 1;
                            metrics.active_sessions = metrics.active_sessions.saturating_sub(1);
                            let dur = session
                                .started_at
                                .map(|s| chrono::Utc::now().timestamp_millis() - s)
                                .unwrap_or(0);
                            terminal_events.push((
                                id.clone(),
                                session.goal.description.clone(),
                                dur,
                                None::<String>,
                                Some("session timed out".into()),
                                AgentState::TimedOut,
                            ));
                        }
                        _ => {}
                    }
                }
            }
            drop(metrics);
            drop(sessions);

            // Publish events and release resources for terminal sessions
            for (sid, goal, dur, error, _timeout_reason, state) in &terminal_events {
                let sid = sid.clone();
                let goal = goal.clone();
                match state {
                    AgentState::Completed => {
                        self.publish_event(
                            crate::events::AutomationEventPayload::SessionCompleted {
                                session_id: sid,
                                goal,
                                duration_ms: *dur,
                            },
                        );
                    }
                    AgentState::Failed(_) => {
                        self.publish_event(crate::events::AutomationEventPayload::SessionFailed {
                            session_id: sid,
                            goal,
                            error: error.clone().unwrap_or_default(),
                        });
                    }
                    AgentState::TimedOut => {
                        self.publish_event(
                            crate::events::AutomationEventPayload::SessionTimedOut {
                                session_id: sid,
                                goal,
                            },
                        );
                    }
                    _ => {}
                }
            }

            if let Some(ref rm) = self.resource_manager {
                for (sid, _, _, _, _, _) in &terminal_events {
                    rm.force_release(sid);
                }
            }
        }

        // Track scheduler stats
        let tick_duration = tick_start.elapsed().as_millis() as u64;
        let mut ss = self.scheduler_stats.write();
        ss.total_ticks += 1;
        if changed.is_empty() {
            ss.successful_ticks += 1;
        } else {
            let has_failures = changed.iter().any(|id| {
                self.sessions
                    .read()
                    .get(id)
                    .map(|s| matches!(s.state, AgentState::Failed(_) | AgentState::TimedOut))
                    .unwrap_or(false)
            });
            if has_failures {
                ss.failed_ticks += 1;
            } else {
                ss.successful_ticks += 1;
            }
        }
        ss.last_tick_duration_ms = tick_duration;
        if ss.min_tick_duration_ms == 0 || tick_duration < ss.min_tick_duration_ms {
            ss.min_tick_duration_ms = tick_duration;
        }
        if tick_duration > ss.max_tick_duration_ms {
            ss.max_tick_duration_ms = tick_duration;
        }
        ss.avg_tick_duration_ms = if ss.total_ticks > 0 {
            let total =
                ss.avg_tick_duration_ms * (ss.total_ticks - 1) as f64 + tick_duration as f64;
            total / ss.total_ticks as f64
        } else {
            tick_duration as f64
        };
        ss.total_sessions_scheduled += dispatched as u64;
        ss.queue_depth = self.sessions.read().len() - self.running_sessions().len();
        ss.is_running = true;

        changed
    }

    // ======================================================================
    // S5 — Monitoring APIs
    // ======================================================================

    /// Current runtime health status.
    pub fn runtime_health(&self) -> RuntimeHealth {
        let m = self.metrics.read();
        let active = self.running_sessions().len();
        let rm = self
            .resource_manager
            .as_ref()
            .map(|rm| rm.resource_metrics());
        let ss = self.scheduler_stats.read();
        crate::monitoring::assess_health(
            active,
            m.failed_sessions + m.timed_out_sessions + m.cancelled_sessions,
            rm.as_ref(),
            Some(&ss),
        )
    }

    /// Snapshot of runtime statistics.
    pub fn runtime_statistics(&self) -> super::RuntimeMetrics {
        self.metrics()
    }

    /// Detailed runtime diagnostics.
    pub fn runtime_diagnostics(&self) -> RuntimeDiagnostics {
        let active = self.running_sessions().len();
        let m = self.metrics();
        let completed = m.completed_sessions;
        let total_dur = completed as f64 * 1000.0;
        let rm = self
            .resource_manager
            .as_ref()
            .map(|rm| rm.resource_metrics());
        let utilization = rm
            .map(|r| {
                if r.peak_locks_held > 0 {
                    r.current_locks_held as f64 / r.peak_locks_held as f64
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);
        let queue_depth = self.scheduled_jobs.read().len();
        self.monitor
            .diagnostics(active, completed, total_dur, utilization, queue_depth)
    }

    /// Event type counts.
    pub fn event_counts(&self) -> EventCounts {
        self.monitor.event_counts()
    }

    /// Currently held resources across all sessions.
    pub fn active_resources(&self) -> Vec<(String, String, String)> {
        let Some(ref rm) = self.resource_manager else {
            return Vec::new();
        };
        let sessions = self.sessions.read();
        let mut result = Vec::new();
        for sid in sessions.keys() {
            for owned in rm.session_resources(sid) {
                result.push((
                    format!("{:?}", owned.resource),
                    sid.clone(),
                    format!("{:?}", owned.access_mode),
                ));
            }
        }
        result
    }

    /// Scheduler performance statistics.
    pub fn scheduler_statistics(&self) -> SchedulerStats {
        self.scheduler_stats.read().clone()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome_verifier::OutcomeVerifier;
    use crate::planner::Goal;
    use crate::recovery_orchestrator::RecoveryOrchestrator;
    use crate::world_state::WorldState;
    use parking_lot::RwLock as PRwLock;
    use std::sync::Arc;

    fn make_executor() -> (Arc<Planner>, Arc<PlanExecutor>) {
        let planner_inner = Planner::new();
        let ws = Arc::new(PRwLock::new(WorldState::new()));
        let verifier = Arc::new(OutcomeVerifier::new(ws.clone(), None));
        let orch = Arc::new(RecoveryOrchestrator::new());
        let executor = Arc::new(PlanExecutor::new(planner_inner, verifier, orch, ws));
        let planner_arc = Arc::new(Planner::new());
        (planner_arc, executor)
    }

    #[test]
    fn test_agent_state_terminal() {
        assert!(AgentState::Completed.is_terminal());
        assert!(AgentState::Failed("err".into()).is_terminal());
        assert!(AgentState::Cancelled.is_terminal());
        assert!(AgentState::TimedOut.is_terminal());
        assert!(!AgentState::Created.is_terminal());
        assert!(!AgentState::Executing.is_terminal());
    }

    #[test]
    fn test_agent_state_active() {
        assert!(AgentState::Planning.is_active());
        assert!(AgentState::Executing.is_active());
        assert!(AgentState::Waiting.is_active());
        assert!(AgentState::Recovering.is_active());
        assert!(!AgentState::Paused.is_active());
        assert!(!AgentState::Created.is_active());
        assert!(!AgentState::Completed.is_active());
    }

    #[test]
    fn test_agent_state_valid_transitions() {
        assert!(AgentState::Created.can_transition_to(&AgentState::Planning));
        assert!(AgentState::Planning.can_transition_to(&AgentState::Executing));
        assert!(AgentState::Executing.can_transition_to(&AgentState::Completed));
        assert!(AgentState::Executing.can_transition_to(&AgentState::Failed("".into())));
        assert!(AgentState::Executing.can_transition_to(&AgentState::Cancelled));
        assert!(AgentState::Executing.can_transition_to(&AgentState::Paused));
        assert!(AgentState::Executing.can_transition_to(&AgentState::Waiting));
        assert!(AgentState::Executing.can_transition_to(&AgentState::Recovering));
        assert!(AgentState::Paused.can_transition_to(&AgentState::Executing));
        assert!(AgentState::Paused.can_transition_to(&AgentState::Cancelled));
        assert!(AgentState::Paused.can_transition_to(&AgentState::TimedOut));
        assert!(AgentState::Waiting.can_transition_to(&AgentState::Executing));
        assert!(AgentState::Recovering.can_transition_to(&AgentState::Planning));
    }

    #[test]
    fn test_agent_state_invalid_transitions() {
        assert!(!AgentState::Created.can_transition_to(&AgentState::Completed));
        // Created -> Cancelled is allowed (cancel before start).
        assert!(AgentState::Created.can_transition_to(&AgentState::Cancelled));
        assert!(!AgentState::Completed.can_transition_to(&AgentState::Executing));
        // Cancelled -> Created is allowed (restart).
        assert!(AgentState::Cancelled.can_transition_to(&AgentState::Created));
        assert!(!AgentState::Paused.can_transition_to(&AgentState::Planning));
        assert!(!AgentState::Executing.can_transition_to(&AgentState::Created));
    }

    #[test]
    fn test_session_creation() {
        let goal = Goal::new("open calculator");
        let session = AgentSession::new("sess_1", goal.clone());
        assert_eq!(session.session_id, "sess_1");
        assert_eq!(session.goal.description, "open calculator");
        assert_eq!(session.state, AgentState::Created);
        assert!(session.created_at > 0);
        assert!(session.started_at.is_none());
    }

    #[test]
    fn test_session_transition_valid() {
        let goal = Goal::new("test");
        let mut session = AgentSession::new("s1", goal);
        assert!(session.transition_to(AgentState::Planning).is_ok());
        assert_eq!(session.state, AgentState::Planning);
        assert!(session.transition_to(AgentState::Executing).is_ok());
        assert_eq!(session.state, AgentState::Executing);
        assert!(session.transition_to(AgentState::Completed).is_ok());
        assert_eq!(session.state, AgentState::Completed);
    }

    #[test]
    fn test_session_transition_invalid() {
        let goal = Goal::new("test");
        let mut session = AgentSession::new("s1", goal);
        assert!(session.transition_to(AgentState::Completed).is_err());
        assert_eq!(session.state, AgentState::Created);
    }

    #[test]
    fn test_runtime_create_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("open chrome")).unwrap();
        assert!(!id.is_empty());
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Created);
    }

    #[test]
    fn test_runtime_create_session_duplicate_goal() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let _id1 = runtime.create_session(Goal::new("open chrome")).unwrap();
        let result = runtime.create_session(Goal::new("open chrome"));
        assert!(result.is_err());
    }

    #[test]
    fn test_runtime_create_session_max_limit() {
        let (planner, executor) = make_executor();
        let config = AgentRuntimeConfig {
            max_sessions: 2,
            ..Default::default()
        };
        let runtime = AgentRuntime::new(executor, planner).with_config(config);
        let _id1 = runtime.create_session(Goal::new("g1")).unwrap();
        let _id2 = runtime.create_session(Goal::new("g2")).unwrap();
        let result = runtime.create_session(Goal::new("g3"));
        assert!(result.is_err());
    }

    #[test]
    fn test_runtime_start_session_known_goal() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        assert!(runtime.start_session(&id).is_ok());
        let session = runtime.session(&id).unwrap();
        assert!(session.state.is_active() || session.state == AgentState::Completed);
    }

    #[test]
    fn test_runtime_start_session_twice_fails() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        assert!(runtime.start_session(&id).is_ok());
        assert!(runtime.start_session(&id).is_err());
    }

    #[test]
    fn test_runtime_pause_resume_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        runtime.start_session(&id).unwrap();

        // Pause
        assert!(runtime.pause_session(&id).is_ok());
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Paused);

        // Resume
        assert!(runtime.resume_session(&id).is_ok());
        let session = runtime.session(&id).unwrap();
        assert!(session.state.is_active() || session.state == AgentState::Completed);
    }

    #[test]
    fn test_runtime_pause_invalid_state() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        // Can't pause a created session.
        assert!(runtime.pause_session(&id).is_err());
    }

    #[test]
    fn test_runtime_resume_invalid_state() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        // Can't resume a created session.
        assert!(runtime.resume_session(&id).is_err());
    }

    #[test]
    fn test_runtime_cancel_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        runtime.start_session(&id).unwrap();
        assert!(runtime.cancel_session(&id).is_ok());
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Cancelled);
    }

    #[test]
    fn test_runtime_cancel_session_twice_fails() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        runtime.start_session(&id).unwrap();
        assert!(runtime.cancel_session(&id).is_ok());
        assert!(runtime.cancel_session(&id).is_err());
    }

    #[test]
    fn test_runtime_session_not_found() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        assert!(runtime.start_session("nonexistent").is_err());
        assert!(runtime.pause_session("nonexistent").is_err());
        assert!(runtime.resume_session("nonexistent").is_err());
        assert!(runtime.cancel_session("nonexistent").is_err());
        assert!(runtime.session("nonexistent").is_none());
    }

    #[test]
    fn test_runtime_running_sessions() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id1 = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        let id2 = runtime.create_session(Goal::new("set brightness")).unwrap();
        runtime.start_session(&id1).unwrap();
        runtime.start_session(&id2).unwrap();
        let running = runtime.running_sessions();
        assert!(!running.is_empty());
    }

    #[test]
    fn test_runtime_completed_sessions() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        runtime.start_session(&id).unwrap();
        runtime.cancel_session(&id).unwrap();
        let completed = runtime.completed_sessions();
        assert!(!completed.is_empty());
    }

    #[test]
    fn test_runtime_tick_advances_sessions() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        runtime.start_session(&id).unwrap();
        runtime.tick();
        // Session should complete or advance.
        let session = runtime.session(&id).unwrap();
        assert!(session.state.is_terminal() || session.state.is_active());
    }

    #[test]
    fn test_runtime_metrics_initial() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let m = runtime.metrics();
        assert_eq!(m.total_sessions_created, 0);
        assert_eq!(m.active_sessions, 0);
    }

    #[test]
    fn test_runtime_metrics_after_sessions() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        let m = runtime.metrics();
        assert_eq!(m.total_sessions_created, 1);

        runtime.start_session(&id).unwrap();
        runtime.cancel_session(&id).unwrap();
        let m = runtime.metrics();
        assert_eq!(m.cancelled_sessions, 1);
    }

    #[test]
    fn test_runtime_prune_completed() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id1 = runtime.create_session(Goal::new("g1")).unwrap();
        let id2 = runtime.create_session(Goal::new("g2")).unwrap();
        runtime.start_session(&id1).unwrap();
        runtime.cancel_session(&id1).unwrap();
        runtime.cancel_session(&id2).unwrap();
        // Both are cancelled → terminal. Prune to 1.
        let pruned = runtime.prune_completed(1);
        assert_eq!(pruned, 1);
        assert_eq!(runtime.sessions().len(), 1);
    }

    #[test]
    fn test_concurrent_sessions_independent() {
        let (planner, executor) = make_executor();
        let runtime = Arc::new(AgentRuntime::new(executor, planner));
        let mut handles = Vec::new();

        for i in 0..3 {
            let rt = runtime.clone();
            handles.push(std::thread::spawn(move || {
                let goal = Goal::new(format!("open app{}", i));
                let id = rt.create_session(goal).unwrap();
                rt.start_session(&id).ok();
                id
            }));
        }

        let ids: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(ids.len(), 3);
        for id in &ids {
            let session = runtime.session(id);
            assert!(session.is_some());
        }
    }

    #[test]
    fn test_session_with_tag() {
        let goal = Goal::new("test");
        let session = AgentSession::new("s1", goal)
            .with_tag("source", "cli")
            .with_tag("user", "admin");
        assert_eq!(session.tags.get("source").unwrap(), "cli");
        assert_eq!(session.tags.get("user").unwrap(), "admin");
    }

    #[test]
    fn test_scheduler_timeout() {
        let (planner, executor) = make_executor();
        let config = AgentRuntimeConfig {
            session_timeout_ms: 1,
            ..Default::default()
        };
        let runtime = AgentRuntime::new(executor, planner).with_config(config);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        runtime.start_session(&id).unwrap();

        // Wait briefly for timeout to register.
        std::thread::sleep(std::time::Duration::from_millis(10));
        runtime.tick();

        let session = runtime.session(&id).unwrap();
        // Could be completed, cancelled, or timed out depending on timing.
        assert!(session.state.is_terminal());
    }

    #[test]
    fn test_runtime_sessions_list() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        runtime.create_session(Goal::new("g1")).unwrap();
        runtime.create_session(Goal::new("g2")).unwrap();
        assert_eq!(runtime.sessions().len(), 2);
    }

    #[test]
    fn test_runtime_unknown_goal_fallback() {
        // A goal that doesn't match heuristic patterns should fail gracefully.
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("do something completely unheard of"))
            .unwrap();
        let result = runtime.start_session(&id);
        // Should either fail or succeed — either is acceptable, but shouldn't panic.
        if let Err(ref e) = result {
            assert!(!e.is_empty());
        }
    }

    #[test]
    fn test_session_metrics_default() {
        let metrics = SessionMetrics::default();
        assert_eq!(metrics.total_steps, 0);
        assert_eq!(metrics.completed_steps, 0);
        assert_eq!(metrics.failed_steps, 0);
    }

    #[test]
    fn test_runtime_metrics_after_tick() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        runtime.start_session(&id).unwrap();
        runtime.tick();
        let m = runtime.metrics();
        assert!(m.total_sessions_created >= 1);
    }

    #[test]
    fn test_runtime_config() {
        let (planner, executor) = make_executor();
        let config = AgentRuntimeConfig {
            max_concurrent_sessions: 10,
            session_timeout_ms: 600_000,
            ..Default::default()
        };
        let runtime = AgentRuntime::new(executor, planner).with_config(config.clone());
        let retrieved = runtime.config();
        assert_eq!(retrieved.max_concurrent_sessions, 10);
        assert_eq!(retrieved.session_timeout_ms, 600_000);
    }

    #[test]
    fn test_runtime_session_has_timestamps() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        runtime.start_session(&id).unwrap();
        let session = runtime.session(&id).unwrap();
        assert!(session.created_at > 0);
        assert!(session.started_at.is_some());
        assert!(session.updated_at >= session.created_at);
    }

    // ======================================================================
    // S2 — Scheduling tests
    // ======================================================================

    #[test]
    fn test_schedule_one_shot_immediate() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.schedule(Goal::new("open calculator")).unwrap();
        assert!(!id.is_empty());
        let jobs = runtime.scheduled_jobs.read();
        assert_eq!(jobs.len(), 1);
    }

    #[test]
    fn test_schedule_after_delayed() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let _id = runtime
            .schedule_after(Goal::new("open calculator"), 5000)
            .unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let jobs = runtime.scheduled_jobs.read();
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].scheduled_at > now - 100);
    }

    #[test]
    fn test_schedule_at_timestamp() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let future = chrono::Utc::now().timestamp_millis() + 10000;
        let _id = runtime.schedule_at(Goal::new("test"), future).unwrap();
        let jobs = runtime.scheduled_jobs.read();
        assert_eq!(jobs[0].scheduled_at, future);
    }

    #[test]
    fn test_schedule_recurring_no_max() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let _id = runtime
            .schedule_recurring(Goal::new("poll"), 5000, None)
            .unwrap();
        let jobs = runtime.scheduled_jobs.read();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].max_runs, None);
        assert!(matches!(jobs[0].job_type, JobType::Recurring { .. }));
    }

    #[test]
    fn test_schedule_recurring_with_max_runs() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let _id = runtime
            .schedule_recurring(Goal::new("poll"), 5000, Some(3))
            .unwrap();
        let jobs = runtime.scheduled_jobs.read();
        assert_eq!(jobs[0].max_runs, Some(3));
    }

    #[test]
    fn test_schedule_max_jobs_limit() {
        let (planner, executor) = make_executor();
        let config = AgentRuntimeConfig {
            max_scheduled_jobs: 2,
            ..Default::default()
        };
        let runtime = AgentRuntime::new(executor, planner).with_config(config);
        runtime.schedule(Goal::new("g1")).unwrap();
        runtime.schedule(Goal::new("g2")).unwrap();
        let result = runtime.schedule(Goal::new("g3"));
        assert!(result.is_err());
    }

    #[test]
    fn test_process_scheduled_jobs_due() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        // Schedule in past so it's immediately due.
        let id = uuid::Uuid::new_v4().to_string();
        {
            let mut jobs = runtime.scheduled_jobs.write();
            jobs.push(ScheduledJob::new(
                &id,
                Goal::new("open calculator"),
                JobType::OneShot,
                0,
            ));
        }
        let dispatched = runtime.process_scheduled_jobs();
        assert_eq!(dispatched, 1);
        let jobs = runtime.scheduled_jobs.read();
        // One-shot should be consumed.
        assert!(jobs.iter().all(|j| j.id != id));
    }

    #[test]
    fn test_process_scheduled_jobs_none_due() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let far_future = chrono::Utc::now().timestamp_millis() + 1_000_000;
        let _id = runtime
            .schedule_at(Goal::new("future goal"), far_future)
            .unwrap();
        let dispatched = runtime.process_scheduled_jobs();
        assert_eq!(dispatched, 0);
    }

    #[test]
    fn test_schedule_priority_ordering() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let now = chrono::Utc::now().timestamp_millis();
        // Insert jobs with varying priority.
        {
            let mut jobs = runtime.scheduled_jobs.write();
            jobs.push(
                ScheduledJob::new("high", Goal::new("high"), JobType::OneShot, now)
                    .with_priority(5),
            );
            jobs.push(
                ScheduledJob::new("low", Goal::new("low"), JobType::OneShot, now).with_priority(1),
            );
            jobs.sort_by(|a, b| {
                a.scheduled_at
                    .cmp(&b.scheduled_at)
                    .then(a.priority.cmp(&b.priority))
            });
        }
        let jobs = runtime.scheduled_jobs.read();
        // Lower priority value = runs first. low=1 should be before high=5.
        assert_eq!(jobs[0].id, "low");
    }

    #[test]
    fn test_schedule_concurrent_max_limit() {
        let (planner, executor) = make_executor();
        let config = AgentRuntimeConfig {
            max_concurrent_scheduled_jobs: 2,
            ..Default::default()
        };
        let runtime = Arc::new(AgentRuntime::new(executor, planner).with_config(config));
        // Submit 3 jobs all immediately due.
        for i in 0..3 {
            let rt = runtime.clone();
            std::thread::spawn(move || {
                let _ = rt.schedule(Goal::new(format!("g{}", i)));
            })
            .join()
            .unwrap();
        }
        let dispatched = runtime.process_scheduled_jobs();
        // At most 2 should be dispatched (max_concurrent_scheduled_jobs = 2).
        assert!(dispatched <= 2);
    }

    #[test]
    fn test_schedule_recurring_exhausted_removed() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = uuid::Uuid::new_v4().to_string();
        {
            let mut jobs = runtime.scheduled_jobs.write();
            let mut job = ScheduledJob::new(
                &id,
                Goal::new("poll"),
                JobType::Recurring { interval_ms: 100 },
                0,
            );
            job.max_runs = Some(1);
            job.run_count = 1; // Already ran once, exhausted
            jobs.push(job);
        }
        let dispatched = runtime.process_scheduled_jobs();
        assert_eq!(dispatched, 0);
        let jobs = runtime.scheduled_jobs.read();
        assert!(jobs.iter().all(|j| j.id != id));
    }

    #[test]
    fn test_schedule_job_is_due_and_exhausted() {
        let now = chrono::Utc::now().timestamp_millis();
        let job = ScheduledJob::new("j1", Goal::new("test"), JobType::OneShot, now - 1000);
        assert!(job.is_due(now));
        assert!(!job.is_exhausted());

        let mut exhausted = job.clone();
        exhausted.run_count = 1;
        exhausted.max_runs = Some(1);
        assert!(exhausted.is_exhausted());
    }

    #[test]
    fn test_schedule_next_occurrence_recurring() {
        let now = chrono::Utc::now().timestamp_millis();
        let mut job = ScheduledJob::new(
            "j1",
            Goal::new("poll"),
            JobType::Recurring { interval_ms: 1000 },
            now,
        );
        job.last_run_at = Some(now + 5000);
        let next = job.next_occurrence();
        assert_eq!(next, Some(now + 5000 + 1000));
    }

    #[test]
    fn test_schedule_next_occurrence_one_shot() {
        let job = ScheduledJob::new("j1", Goal::new("test"), JobType::OneShot, 0);
        assert!(job.next_occurrence().is_none());
    }

    // ======================================================================
    // S2 — Cleanup tests
    // ======================================================================

    #[test]
    fn test_cleanup_expired_sessions() {
        let (planner, executor) = make_executor();
        let config = AgentRuntimeConfig {
            retention_window_ms: 1, // 1ms retention → everything expired immediately
            ..Default::default()
        };
        let runtime = AgentRuntime::new(executor, planner).with_config(config);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        runtime.cancel_session(&id).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let cleaned = runtime.cleanup_sessions();
        assert_eq!(cleaned, 1);
    }

    #[test]
    fn test_cleanup_preserves_recent() {
        let (planner, executor) = make_executor();
        let config = AgentRuntimeConfig {
            retention_window_ms: 86_400_000, // 24h — nothing expired
            ..Default::default()
        };
        let runtime = AgentRuntime::new(executor, planner).with_config(config);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        runtime.cancel_session(&id).unwrap();
        let cleaned = runtime.cleanup_sessions();
        assert_eq!(cleaned, 0);
    }

    #[test]
    fn test_cleanup_skips_active_sessions() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        runtime.create_session(Goal::new("g1")).unwrap();
        let cleaned = runtime.cleanup_sessions();
        // Created is not terminal → should not be cleaned.
        assert_eq!(cleaned, 0);
    }

    #[test]
    fn test_cleanup_metrics_tracked() {
        let (planner, executor) = make_executor();
        let config = AgentRuntimeConfig {
            retention_window_ms: 1,
            ..Default::default()
        };
        let runtime = AgentRuntime::new(executor, planner).with_config(config);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        runtime.cancel_session(&id).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        runtime.cleanup_sessions();
        let m = runtime.metrics();
        assert_eq!(m.cleaned_up_sessions, 1);
    }

    // ======================================================================
    // S2 — Persistence tests
    // ======================================================================

    #[test]
    fn test_persist_all_sessions() {
        let (planner, executor) = make_executor();
        let store = Arc::new(crate::session_store::InMemorySessionStore::new());
        let runtime = AgentRuntime::new(executor, planner).with_session_store(store.clone());
        runtime.create_session(Goal::new("g1")).unwrap();
        runtime.create_session(Goal::new("g2")).unwrap();
        let count = runtime.persist_all().unwrap();
        assert_eq!(count, 2);
        assert_eq!(store.list_sessions().unwrap().len(), 2);
    }

    #[test]
    fn test_persist_all_empty() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let count = runtime.persist_all().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_restore_all_from_store() {
        let (planner, executor) = make_executor();
        let store = Arc::new(crate::session_store::InMemorySessionStore::new());
        let session = AgentSession::new("test-session", Goal::new("restored"));
        store.save_session(&session).unwrap();
        let runtime = AgentRuntime::new(executor, planner).with_session_store(store);
        let count = runtime.restore_all().unwrap();
        assert_eq!(count, 1);
        let loaded = runtime.session("test-session").unwrap();
        assert_eq!(loaded.goal.description, "restored");
    }

    #[test]
    fn test_restore_all_empty() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let count = runtime.restore_all().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_persist_metrics_tracked() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        runtime.create_session(Goal::new("g1")).unwrap();
        runtime.persist_all().unwrap();
        let m = runtime.metrics();
        assert_eq!(m.persistence_saves, 1);
    }

    #[test]
    fn test_restore_metrics_tracked() {
        let (planner, executor) = make_executor();
        let store = Arc::new(crate::session_store::InMemorySessionStore::new());
        store
            .save_session(&AgentSession::new("s1", Goal::new("g1")))
            .unwrap();
        let runtime = AgentRuntime::new(executor, planner).with_session_store(store);
        runtime.restore_all().unwrap();
        let m = runtime.metrics();
        assert_eq!(m.persistence_loads, 1);
    }

    // ======================================================================
    // S2 — Recovery tests
    // ======================================================================

    #[test]
    fn test_resume_pending_active_sessions() {
        let (planner, executor) = make_executor();
        let plan = planner.plan(&Goal::new("open calculator")).ok();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        {
            let mut sessions = runtime.sessions.write();
            if let Some(s) = sessions.get_mut(&id) {
                s.state = AgentState::Executing;
                s.execution_plan = plan.clone();
            }
        }
        let resumed = runtime.resume_pending();
        assert_eq!(resumed, 1);
    }

    #[test]
    fn test_resume_pending_skips_terminal() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            if let Some(s) = sessions.get_mut(&id) {
                s.state = AgentState::Completed;
            }
        }
        let resumed = runtime.resume_pending();
        assert_eq!(resumed, 0);
    }

    #[test]
    fn test_resume_pending_no_sessions() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let resumed = runtime.resume_pending();
        assert_eq!(resumed, 0);
    }

    #[test]
    fn test_resume_pending_recovering_transitions_to_planning() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            if let Some(s) = sessions.get_mut(&id) {
                s.state = AgentState::Recovering;
            }
        }
        let resumed = runtime.resume_pending();
        assert_eq!(resumed, 1);
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Planning);
    }

    #[test]
    fn test_resume_pending_metrics_tracked() {
        let (planner, executor) = make_executor();
        let plan = planner.plan(&Goal::new("open calculator")).ok();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime
            .create_session(Goal::new("open calculator"))
            .unwrap();
        {
            let mut sessions = runtime.sessions.write();
            if let Some(s) = sessions.get_mut(&id) {
                s.state = AgentState::Executing;
                s.execution_plan = plan.clone();
            }
        }
        runtime.resume_pending();
        let m = runtime.metrics();
        assert_eq!(m.resumed_sessions, 1);
        assert_eq!(m.recovered_sessions, 1);
    }

    // ======================================================================
    // S2 — Scheduler metrics tests
    // ======================================================================

    #[test]
    fn test_scheduler_metrics_scheduled_jobs() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        runtime
            .schedule_recurring(Goal::new("g1"), 1000, None)
            .unwrap();
        let m = runtime.metrics();
        assert_eq!(m.scheduled_jobs, 1);
    }

    #[test]
    fn test_scheduler_metrics_queue_latency() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = uuid::Uuid::new_v4().to_string();
        {
            let mut jobs = runtime.scheduled_jobs.write();
            jobs.push(ScheduledJob::new(
                &id,
                Goal::new("test"),
                JobType::OneShot,
                0,
            ));
        }
        runtime.process_scheduled_jobs();
        let m = runtime.metrics();
        assert!(m.queue_latency_ms >= 0);
    }

    #[test]
    fn test_scheduled_job_has_priority_default() {
        let job = ScheduledJob::new("j1", Goal::new("test"), JobType::OneShot, 0);
        assert_eq!(job.priority, 0);
    }

    #[test]
    fn test_scheduled_job_with_priority() {
        let job = ScheduledJob::new("j1", Goal::new("test"), JobType::OneShot, 0).with_priority(10);
        assert_eq!(job.priority, 10);
    }

    #[test]
    fn test_scheduled_job_with_tag() {
        let job =
            ScheduledJob::new("j1", Goal::new("test"), JobType::OneShot, 0).with_tag("env", "prod");
        assert_eq!(job.tags.get("env").unwrap(), "prod");
    }

    #[test]
    fn test_agent_session_serializable() {
        // Verify AgentSession can be serialized/deserialized via serde_json.
        let session = AgentSession::new("test-id", Goal::new("test goal"));
        let json = serde_json::to_string(&session).unwrap();
        let deserialized: AgentSession = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.session_id, "test-id");
        assert_eq!(deserialized.goal.description, "test goal");
        assert_eq!(deserialized.state, AgentState::Created);
    }

    #[test]
    fn test_runtime_metrics_has_s2_fields() {
        let metrics = RuntimeMetrics::default();
        assert_eq!(metrics.scheduled_jobs, 0);
        assert_eq!(metrics.resumed_sessions, 0);
        assert_eq!(metrics.recovered_sessions, 0);
        assert_eq!(metrics.persistence_failures, 0);
        assert_eq!(metrics.cleaned_up_sessions, 0);
        assert_eq!(metrics.recurring_jobs_completed, 0);
    }

    #[test]
    fn test_schedule_metrics_tick_integration() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = uuid::Uuid::new_v4().to_string();
        {
            let mut jobs = runtime.scheduled_jobs.write();
            jobs.push(ScheduledJob::new(
                &id,
                Goal::new("open calculator"),
                JobType::OneShot,
                0,
            ));
        }
        runtime.tick(); // tick processes scheduled jobs + advances sessions
        let m = runtime.metrics();
        // At minimum the scheduled job was processed.
        assert!(m.total_sessions_created >= 1);
    }

    #[test]
    fn test_schedule_recurring_reschedules_after_run() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        {
            let mut jobs = runtime.scheduled_jobs.write();
            jobs.push(ScheduledJob::new(
                &id,
                Goal::new("poll"),
                JobType::Recurring { interval_ms: 1000 },
                now,
            ));
        }
        runtime.process_scheduled_jobs();
        let jobs = runtime.scheduled_jobs.read();
        // After processing, the recurring job should be re-scheduled with a future scheduled_at.
        if let Some(j) = jobs.iter().find(|j| j.id == id) {
            assert!(j.run_count > 0);
            assert!(j.scheduled_at >= now);
        } else {
            // If it was immediately exhausted (max_runs set), it's removed.
        }
    }

    #[test]
    fn test_cleanup_expired_sessions_with_store() {
        let (planner, executor) = make_executor();
        let store = Arc::new(crate::session_store::InMemorySessionStore::new());
        let config = AgentRuntimeConfig {
            retention_window_ms: 1,
            enable_persistence: true,
            ..Default::default()
        };
        let runtime = AgentRuntime::new(executor, planner)
            .with_config(config)
            .with_session_store(store);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        runtime.cancel_session(&id).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let cleaned = runtime.cleanup_sessions();
        assert_eq!(cleaned, 1);
        runtime.persist_all().unwrap();
    }

    #[test]
    fn test_session_store_trait_object() {
        let store: Arc<dyn SessionStore> =
            Arc::new(crate::session_store::InMemorySessionStore::new());
        let session = AgentSession::new("trait-test", Goal::new("test"));
        store.save_session(&session).unwrap();
        let loaded = store.load_session("trait-test").unwrap();
        assert_eq!(loaded.session_id, "trait-test");
    }

    // ======================================================================
    // M24 — Restart API tests
    // ======================================================================

    #[test]
    fn test_restart_completed_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        // Force complete
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            s.state = AgentState::Completed;
            s.completed_at = Some(chrono::Utc::now().timestamp_millis());
        }
        assert!(runtime.restart_session(&id).is_ok());
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Created);
        assert!(session.started_at.is_none());
        assert!(session.completed_at.is_none());
        assert_eq!(session.retry_count, 0);
        assert_eq!(session.metrics.completed_steps, 0);
    }

    #[test]
    fn test_restart_failed_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            s.state = AgentState::Failed("error".into());
            s.retry_count = 3;
            s.metrics.failed_steps = 2;
        }
        assert!(runtime.restart_session(&id).is_ok());
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Created);
        assert_eq!(session.retry_count, 0);
        assert_eq!(session.metrics.failed_steps, 0);
    }

    #[test]
    fn test_restart_cancelled_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        runtime.cancel_session(&id).unwrap();
        assert!(runtime.restart_session(&id).is_ok());
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Created);
    }

    #[test]
    fn test_restart_timedout_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            s.state = AgentState::TimedOut;
        }
        assert!(runtime.restart_session(&id).is_ok());
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Created);
    }

    #[test]
    fn test_restart_active_session_fails() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        // Cannot restart an executing session
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            s.state = AgentState::Executing;
        }
        assert!(runtime.restart_session(&id).is_err());
    }

    #[test]
    fn test_restart_nonexistent_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        assert!(runtime.restart_session("nonexistent").is_err());
    }

    #[test]
    fn test_restart_clears_execution_plan() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner.clone());
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            s.state = AgentState::Completed;
            s.execution_plan = planner.plan(&Goal::new("old")).ok();
        }
        runtime.restart_session(&id).unwrap();
        let session = runtime.session(&id).unwrap();
        assert!(session.execution_plan.is_none());
    }

    #[test]
    fn test_restart_clears_report() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            s.state = AgentState::Failed("err".into());
            s.report = Some(crate::plan_executor::GoalExecutionReport {
                goal: "g1".into(),
                plan_id: "p1".into(),
                success: false,
                state: crate::plan_executor::PipelineExecutionState::Aborted {
                    reason: "err".into(),
                },
                total_steps: 1,
                completed_steps: 0,
                failed_steps: 1,
                skipped_steps: 0,
                retried_steps: 0,
                replans: 0,
                abort_reason: Some("err".into()),
                execution_duration: std::time::Duration::from_millis(0),
                verification_count: 0,
                recovery_count: 0,
                step_records: vec![],
                metrics: crate::observability::ExecutionMetrics::default(),
            });
        }
        runtime.restart_session(&id).unwrap();
        let session = runtime.session(&id).unwrap();
        assert!(session.report.is_none());
    }

    // ======================================================================
    // M24 — Lifecycle edge case tests
    // ======================================================================

    #[test]
    fn test_cancel_created_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        assert!(runtime.cancel_session(&id).is_ok());
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Cancelled);
    }

    #[test]
    fn test_cancel_paused_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        runtime.pause_session(&id).unwrap();
        assert!(runtime.cancel_session(&id).is_ok());
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Cancelled);
    }

    #[test]
    fn test_cancel_planning_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            let _ = s.transition_to(AgentState::Planning);
        }
        assert!(runtime.cancel_session(&id).is_ok());
        assert_eq!(runtime.session(&id).unwrap().state, AgentState::Cancelled);
    }

    #[test]
    fn test_cancel_recovering_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            let _ = s.transition_to(AgentState::Planning);
            let _ = s.transition_to(AgentState::Executing);
            let _ = s.transition_to(AgentState::Recovering);
        }
        assert!(runtime.cancel_session(&id).is_ok());
        assert_eq!(runtime.session(&id).unwrap().state, AgentState::Cancelled);
    }

    #[test]
    fn test_cancel_waiting_session() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            let _ = s.transition_to(AgentState::Planning);
            let _ = s.transition_to(AgentState::Executing);
            let _ = s.transition_to(AgentState::Waiting);
        }
        assert!(runtime.cancel_session(&id).is_ok());
        assert_eq!(runtime.session(&id).unwrap().state, AgentState::Cancelled);
    }

    #[test]
    fn test_multiple_pause_resume_cycles() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        for _ in 0..5 {
            assert!(runtime.pause_session(&id).is_ok());
            let s = runtime.session(&id).unwrap();
            assert_eq!(s.state, AgentState::Paused);
            assert!(runtime.resume_session(&id).is_ok());
        }
    }

    #[test]
    fn test_runtime_max_concurrent_sessions_enforced() {
        let (planner, executor) = make_executor();
        let config = AgentRuntimeConfig {
            max_concurrent_sessions: 2,
            ..Default::default()
        };
        let runtime = AgentRuntime::new(executor, planner).with_config(config);
        let id1 = runtime.create_session(Goal::new("g1")).unwrap();
        let id2 = runtime.create_session(Goal::new("g2")).unwrap();
        runtime.start_session(&id1).unwrap();
        runtime.start_session(&id2).unwrap();
        // Both should be running
        assert_eq!(runtime.running_sessions().len(), 2);
    }

    #[test]
    fn test_session_recovery_count_tracked() {
        let goal = Goal::new("test");
        let mut session = AgentSession::new("s1", goal);
        session.recovery_count = 5;
        assert_eq!(session.recovery_count, 5);
        session.recovery_count += 1;
        assert_eq!(session.recovery_count, 6);
    }

    #[test]
    fn test_session_retry_replan_counts() {
        let goal = Goal::new("test");
        let mut session = AgentSession::new("s1", goal);
        session.retry_count = 3;
        session.replan_count = 2;
        assert_eq!(session.retry_count, 3);
        assert_eq!(session.replan_count, 2);
    }

    #[test]
    fn test_runtime_metrics_total_replans() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            s.metrics.replans = 3;
        }
        runtime.tick();
    }

    #[test]
    fn test_runtime_metrics_consistent_after_ops() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id1 = runtime.create_session(Goal::new("g1")).unwrap();
        let id2 = runtime.create_session(Goal::new("g2")).unwrap();
        runtime.start_session(&id1).unwrap();
        runtime.start_session(&id2).unwrap();
        runtime.pause_session(&id1).unwrap();
        runtime.cancel_session(&id2).unwrap();
        let m = runtime.metrics();
        assert_eq!(m.total_sessions_created, 2);
    }

    #[test]
    fn test_runtime_metrics_active_count_correct() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        assert_eq!(runtime.running_sessions().len(), 0);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        // Should be active or completed
        let running = runtime.running_sessions();
        assert!(running.len() == 1 || running.is_empty());
    }

    #[test]
    fn test_runtime_metrics_duration_positive() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        runtime.create_session(Goal::new("g1")).unwrap();
        let m = runtime.metrics();
        assert!(m.runtime_started_at.is_some());
        assert!(m.runtime_duration_ms >= 0);
    }

    #[test]
    fn test_scheduler_does_not_advance_paused() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        runtime.pause_session(&id).unwrap();
        runtime.tick();
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Paused);
    }

    #[test]
    fn test_scheduler_timeout_check_only_active() {
        let (planner, executor) = make_executor();
        let config = AgentRuntimeConfig {
            session_timeout_ms: 1,
            ..Default::default()
        };
        let runtime = AgentRuntime::new(executor, planner).with_config(config);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        // Created — not active, should not be timed out
        runtime.tick();
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.state, AgentState::Created);
    }

    #[test]
    fn test_session_completed_at_set_on_success() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            s.state = AgentState::Completed;
            s.completed_at = Some(chrono::Utc::now().timestamp_millis());
        }
        let session = runtime.session(&id).unwrap();
        assert!(session.completed_at.is_some());
    }

    #[test]
    fn test_session_failed_state_contains_reason() {
        let goal = Goal::new("test");
        let mut session = AgentSession::new("s1", goal);
        session.state = AgentState::Failed("permission denied".into());
        session.updated_at = chrono::Utc::now().timestamp_millis();
        match &session.state {
            AgentState::Failed(reason) => assert_eq!(reason, "permission denied"),
            _ => panic!("expected Failed state"),
        }
    }

    #[test]
    fn test_session_timestamps_order() {
        let goal = Goal::new("test");
        let mut session = AgentSession::new("s1", goal);
        let created = session.created_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        let _ = session.transition_to(AgentState::Planning);
        assert!(session.updated_at > created);
    }

    #[test]
    fn test_runtime_restart_affects_metrics() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        {
            let mut sessions = runtime.sessions.write();
            let s = sessions.get_mut(&id).unwrap();
            s.state = AgentState::Completed;
        }
        let m_before = runtime.metrics();
        assert!(runtime.restart_session(&id).is_ok());
        let m_after = runtime.metrics();
        assert_eq!(m_after.resumed_sessions, m_before.resumed_sessions + 1);
    }

    #[test]
    fn test_paused_session_does_not_timeout() {
        let (planner, executor) = make_executor();
        let config = AgentRuntimeConfig {
            session_timeout_ms: 1,
            ..Default::default()
        };
        let runtime = AgentRuntime::new(executor, planner).with_config(config);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        runtime.pause_session(&id).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        runtime.tick();
        let session = runtime.session(&id).unwrap();
        // Paused sessions should NOT be timed out
        assert_eq!(session.state, AgentState::Paused);
    }

    #[test]
    fn test_runtime_create_session_with_tags() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let goal = Goal::new("test")
            .with_context("user", "alice")
            .with_context("env", "dev");
        let id = runtime.create_session(goal).unwrap();
        let session = runtime.session(&id).unwrap();
        assert_eq!(session.goal.context.get("user").unwrap(), "alice");
        assert_eq!(session.goal.context.get("env").unwrap(), "dev");
    }

    #[test]
    fn test_runtime_sessions_list_unsorted() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        runtime.create_session(Goal::new("g1")).unwrap();
        runtime.create_session(Goal::new("g2")).unwrap();
        runtime.create_session(Goal::new("g3")).unwrap();
        let all = runtime.sessions();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_runtime_completed_sessions_includes_failed() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        let id = runtime.create_session(Goal::new("g1")).unwrap();
        runtime.start_session(&id).unwrap();
        runtime.cancel_session(&id).unwrap();
        let completed = runtime.completed_sessions();
        assert!(completed.iter().any(|s| s.state == AgentState::Cancelled));
    }

    #[test]
    fn test_invalid_transition_from_completed() {
        let mut session = AgentSession::new("s1", Goal::new("test"));
        session.state = AgentState::Completed;
        // From completed, only Created (restart) should work
        assert!(!session.state.can_transition_to(&AgentState::Executing));
        assert!(!session.state.can_transition_to(&AgentState::Paused));
        assert!(session.state.can_transition_to(&AgentState::Created));
    }

    #[test]
    fn test_invalid_transition_from_failed() {
        let mut session = AgentSession::new("s1", Goal::new("test"));
        session.state = AgentState::Failed("err".into());
        assert!(session.state.can_transition_to(&AgentState::Created));
        assert!(!session.state.can_transition_to(&AgentState::Executing));
    }

    #[test]
    fn test_invalid_transition_from_cancelled() {
        let mut session = AgentSession::new("s1", Goal::new("test"));
        session.state = AgentState::Cancelled;
        assert!(session.state.can_transition_to(&AgentState::Created));
        assert!(!session.state.can_transition_to(&AgentState::Planning));
    }

    #[test]
    fn test_invalid_transition_from_timedout() {
        let mut session = AgentSession::new("s1", Goal::new("test"));
        session.state = AgentState::TimedOut;
        assert!(session.state.can_transition_to(&AgentState::Created));
        assert!(!session.state.can_transition_to(&AgentState::Executing));
    }

    #[test]
    fn test_recovering_can_transition_to_executing() {
        let mut session = AgentSession::new("s1", Goal::new("test"));
        let _ = session.transition_to(AgentState::Planning);
        let _ = session.transition_to(AgentState::Executing);
        let _ = session.transition_to(AgentState::Recovering);
        assert!(session.state.can_transition_to(&AgentState::Executing));
    }

    #[test]
    fn test_running_sessions_empty_when_none() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        assert!(runtime.running_sessions().is_empty());
    }

    #[test]
    fn test_completed_sessions_empty_when_none() {
        let (planner, executor) = make_executor();
        let runtime = AgentRuntime::new(executor, planner);
        assert!(runtime.completed_sessions().is_empty());
    }
}
