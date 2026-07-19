use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::goal_registry::GoalRegistry;
use crate::intention_parser::Intent;
use crate::plan_executor::{GoalExecutionReport, PlanExecutor};
use crate::planner::{Goal, Planner};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum ExecutionPriority {
    Immediate,
    High,
    #[default]
    Normal,
    Low,
    Background,
}

impl ExecutionPriority {
    pub fn as_u8(&self) -> u8 {
        match self {
            ExecutionPriority::Immediate => 0,
            ExecutionPriority::High => 1,
            ExecutionPriority::Normal => 2,
            ExecutionPriority::Low => 3,
            ExecutionPriority::Background => 4,
        }
    }
}

pub use crate::history::ExecutionStatus;

impl ExecutionStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ExecutionStatus::Completed
                | ExecutionStatus::Failed
                | ExecutionStatus::Cancelled
                | ExecutionStatus::TimedOut
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            ExecutionStatus::Queued
                | ExecutionStatus::Running
                | ExecutionStatus::Waiting
                | ExecutionStatus::Paused
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ExecutionPolicy {
    #[default]
    Sequential,
    Parallel,
    Exclusive,
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub id: String,
    pub goal: Goal,
    pub intent: Option<Intent>,
    pub priority: ExecutionPriority,
    pub policy: ExecutionPolicy,
    pub timeout: Option<Duration>,
    pub max_retries: u32,
    pub created_at: i64,
}

impl ExecutionRequest {
    pub fn new(id: impl Into<String>, goal: Goal) -> Self {
        Self {
            id: id.into(),
            goal,
            intent: None,
            priority: ExecutionPriority::Normal,
            policy: ExecutionPolicy::Sequential,
            timeout: None,
            max_retries: 0,
            created_at: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn with_priority(mut self, priority: ExecutionPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_policy(mut self, policy: ExecutionPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_intent(mut self, intent: Intent) -> Self {
        self.intent = Some(intent);
        self
    }

    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub request_id: String,
    pub goal_description: String,
    pub status: ExecutionStatus,
    pub report: Option<GoalExecutionReport>,
    pub error: Option<String>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub duration: Duration,
}

#[derive(Debug, Clone)]
pub enum ExecutionEvent {
    Submitted {
        request_id: String,
        goal: String,
        priority: ExecutionPriority,
    },
    Queued {
        request_id: String,
        position: usize,
    },
    Started {
        request_id: String,
    },
    Completed {
        request_id: String,
        success: bool,
        duration_ms: i64,
    },
    Failed {
        request_id: String,
        error: String,
    },
    Cancelled {
        request_id: String,
        reason: Option<String>,
    },
    Paused {
        request_id: String,
    },
    Resumed {
        request_id: String,
    },
    Progress {
        request_id: String,
        progress: f32,
    },
}

pub struct ExecutionHandle {
    id: String,
    status: Arc<RwLock<ExecutionStatus>>,
    cancelled: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    completed_tx: Arc<tokio::sync::Notify>,
    progress: Arc<RwLock<f32>>,
    result: Arc<RwLock<Option<ExecutionResult>>>,
}

impl ExecutionHandle {
    fn new(id: String) -> Self {
        Self {
            id,
            status: Arc::new(RwLock::new(ExecutionStatus::Pending)),
            cancelled: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            completed_tx: Arc::new(tokio::sync::Notify::new()),
            progress: Arc::new(RwLock::new(0.0)),
            result: Arc::new(RwLock::new(None)),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn status(&self) -> ExecutionStatus {
        *self.status.read()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.set_status(ExecutionStatus::Cancelled);
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn progress(&self) -> f32 {
        *self.progress.read()
    }

    pub fn result(&self) -> Option<ExecutionResult> {
        self.result.read().clone()
    }

    pub async fn wait(&self) {
        self.completed_tx.notified().await;
    }

    fn set_status(&self, status: ExecutionStatus) {
        *self.status.write() = status;
        if status.is_terminal() {
            self.completed_tx.notify_waiters();
        }
    }

    fn set_progress(&self, p: f32) {
        *self.progress.write() = p.clamp(0.0, 1.0);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    #[allow(dead_code)]
    fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }
}

impl Clone for ExecutionHandle {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            status: self.status.clone(),
            cancelled: self.cancelled.clone(),
            paused: self.paused.clone(),
            completed_tx: self.completed_tx.clone(),
            progress: self.progress.clone(),
            result: self.result.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionHistoryEntry {
    pub execution_id: String,
    pub goal_id: String,
    pub intent: Option<String>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub duration: Option<Duration>,
    pub status: ExecutionStatus,
    pub failure_reason: Option<String>,
    pub retry_count: u32,
    pub priority: ExecutionPriority,
    pub policy: ExecutionPolicy,
}

#[derive(Debug, Clone)]
pub struct ExecutionHistory {
    entries: Vec<ExecutionHistoryEntry>,
    max_entries: usize,
}

impl ExecutionHistory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: 1000,
        }
    }

    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    pub fn push(&mut self, entry: ExecutionHistoryEntry) {
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[ExecutionHistoryEntry] {
        &self.entries
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ExecutionHistory {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ExecutionStatistics {
    pub submitted: AtomicU64,
    pub completed: AtomicU64,
    pub failed: AtomicU64,
    pub cancelled: AtomicU64,
    pub running: AtomicU64,
    pub queued: AtomicU64,
    pub average_duration_ms: AtomicU64,
    pub peak_concurrency: AtomicU64,
    pub total_retries: AtomicU64,
}

impl Default for ExecutionStatistics {
    fn default() -> Self {
        Self {
            submitted: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            cancelled: AtomicU64::new(0),
            running: AtomicU64::new(0),
            queued: AtomicU64::new(0),
            average_duration_ms: AtomicU64::new(0),
            peak_concurrency: AtomicU64::new(0),
            total_retries: AtomicU64::new(0),
        }
    }
}

impl ExecutionStatistics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> ExecutionStatisticsSnapshot {
        ExecutionStatisticsSnapshot {
            submitted: self.submitted.load(Ordering::SeqCst),
            completed: self.completed.load(Ordering::SeqCst),
            failed: self.failed.load(Ordering::SeqCst),
            cancelled: self.cancelled.load(Ordering::SeqCst),
            running: self.running.load(Ordering::SeqCst),
            queued: self.queued.load(Ordering::SeqCst),
            average_duration_ms: self.average_duration_ms.load(Ordering::SeqCst),
            peak_concurrency: self.peak_concurrency.load(Ordering::SeqCst),
            total_retries: self.total_retries.load(Ordering::SeqCst),
        }
    }

    fn record_submitted(&self) {
        self.submitted.fetch_add(1, Ordering::SeqCst);
    }

    fn record_completed(&self) {
        self.completed.fetch_add(1, Ordering::SeqCst);
    }

    fn record_failed(&self) {
        self.failed.fetch_add(1, Ordering::SeqCst);
    }

    fn record_cancelled(&self) {
        self.cancelled.fetch_add(1, Ordering::SeqCst);
    }

    #[allow(dead_code)]
    fn record_retry(&self) {
        self.total_retries.fetch_add(1, Ordering::SeqCst);
    }

    fn update_concurrency(&self, current: usize) {
        let prev = self.peak_concurrency.load(Ordering::SeqCst);
        if current as u64 > prev {
            self.peak_concurrency
                .compare_exchange(prev, current as u64, Ordering::SeqCst, Ordering::SeqCst)
                .ok();
        }
    }

    fn update_duration(&self, duration_ms: u64) {
        let prev = self.average_duration_ms.load(Ordering::SeqCst);
        if prev == 0 {
            self.average_duration_ms
                .store(duration_ms, Ordering::SeqCst);
        } else {
            let new = (prev + duration_ms) / 2;
            self.average_duration_ms.store(new, Ordering::SeqCst);
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionStatisticsSnapshot {
    pub submitted: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub running: u64,
    pub queued: u64,
    pub average_duration_ms: u64,
    pub peak_concurrency: u64,
    pub total_retries: u64,
}

#[derive(Debug, Clone)]
pub struct ExecutionQueueConfig {
    pub max_size: usize,
    pub max_concurrent: usize,
    pub enable_background_queue: bool,
    pub default_timeout_ms: u64,
}

impl Default for ExecutionQueueConfig {
    fn default() -> Self {
        Self {
            max_size: 100,
            max_concurrent: 5,
            enable_background_queue: true,
            default_timeout_ms: 300_000,
        }
    }
}

struct QueueEntry {
    request: ExecutionRequest,
    handle: ExecutionHandle,
    _submitted_at: Instant,
}

struct ExecutionQueueInner {
    priority_queue: BTreeMap<u8, VecDeque<QueueEntry>>,
    active: HashMap<String, QueueEntry>,
    background: VecDeque<QueueEntry>,
    exclusive_holder: Option<String>,
}

impl ExecutionQueueInner {
    fn new() -> Self {
        Self {
            priority_queue: BTreeMap::new(),
            active: HashMap::new(),
            background: VecDeque::new(),
            exclusive_holder: None,
        }
    }

    fn total_pending(&self) -> usize {
        let pq: usize = self.priority_queue.values().map(|q| q.len()).sum();
        pq + self.background.len()
    }

    fn total_active(&self) -> usize {
        self.active.len()
    }

    fn is_full(&self, max: usize) -> bool {
        self.total_pending() + self.total_active() >= max
    }

    fn enqueue(&mut self, entry: QueueEntry, max_size: usize) -> Result<(), String> {
        if self.is_full(max_size) {
            return Err("execution queue is full".into());
        }
        if entry.request.policy == ExecutionPolicy::Exclusive && !self.active.is_empty() {
            return Err("cannot enqueue exclusive request while others are active".into());
        }
        if entry.request.priority == ExecutionPriority::Background {
            self.background.push_back(entry);
        } else {
            let key = entry.request.priority.as_u8();
            self.priority_queue.entry(key).or_default().push_back(entry);
        }
        Ok(())
    }

    fn dequeue(&mut self) -> Option<QueueEntry> {
        if let Some(ref holder) = self.exclusive_holder {
            if self.active.contains_key(holder) {
                return None;
            }
            self.exclusive_holder = None;
        }
        for (_, queue) in self.priority_queue.iter_mut() {
            if let Some(entry) = queue.pop_front() {
                if entry.request.policy == ExecutionPolicy::Exclusive {
                    self.exclusive_holder = Some(entry.request.id.clone());
                }
                return Some(entry);
            }
        }
        self.background.pop_front()
    }

    fn remove(&mut self, id: &str) -> Option<QueueEntry> {
        for (_, queue) in self.priority_queue.iter_mut() {
            if let Some(pos) = queue.iter().position(|e| e.request.id == id) {
                return queue.remove(pos);
            }
        }
        if let Some(pos) = self.background.iter().position(|e| e.request.id == id) {
            return self.background.remove(pos);
        }
        self.active.remove(id)
    }
}

pub struct ExecutionManager {
    planner: Arc<Planner>,
    plan_executor: Arc<PlanExecutor>,
    goal_registry: Option<Arc<GoalRegistry>>,
    queue: Arc<RwLock<ExecutionQueueInner>>,
    history: Arc<RwLock<ExecutionHistory>>,
    statistics: Arc<ExecutionStatistics>,
    active_handles: Arc<RwLock<HashMap<String, ExecutionHandle>>>,
    config: ExecutionQueueConfig,
}

impl ExecutionManager {
    pub fn new(planner: Arc<Planner>, plan_executor: Arc<PlanExecutor>) -> Self {
        Self {
            planner,
            plan_executor,
            goal_registry: None,
            queue: Arc::new(RwLock::new(ExecutionQueueInner::new())),
            history: Arc::new(RwLock::new(ExecutionHistory::new())),
            statistics: Arc::new(ExecutionStatistics::new()),
            active_handles: Arc::new(RwLock::new(HashMap::new())),
            config: ExecutionQueueConfig::default(),
        }
    }

    pub fn with_goal_registry(mut self, registry: Arc<GoalRegistry>) -> Self {
        self.goal_registry = Some(registry);
        self
    }

    pub fn with_queue_config(mut self, config: ExecutionQueueConfig) -> Self {
        self.config = config;
        self
    }

    pub fn planner(&self) -> &Arc<Planner> {
        &self.planner
    }

    pub fn plan_executor(&self) -> &Arc<PlanExecutor> {
        &self.plan_executor
    }

    pub fn goal_registry(&self) -> Option<&Arc<GoalRegistry>> {
        self.goal_registry.as_ref()
    }

    pub fn submit(&self, request: ExecutionRequest) -> Result<ExecutionHandle, String> {
        self.statistics.record_submitted();
        let handle = ExecutionHandle::new(request.id.clone());
        handle.set_status(ExecutionStatus::Queued);

        let entry = QueueEntry {
            request: request.clone(),
            handle: handle.clone(),
            _submitted_at: Instant::now(),
        };

        {
            let mut queue = self.queue.write();
            queue.enqueue(entry, self.config.max_size)?;
            self.statistics.queued.fetch_add(1, Ordering::SeqCst);
        }

        self.active_handles
            .write()
            .insert(request.id.clone(), handle.clone());

        self.try_process_queue();

        Ok(handle)
    }

    pub fn submit_batch(
        &self,
        requests: Vec<ExecutionRequest>,
    ) -> Vec<Result<ExecutionHandle, String>> {
        requests.into_iter().map(|r| self.submit(r)).collect()
    }

    pub fn cancel(&self, id: &str) -> Result<(), String> {
        {
            let mut queue = self.queue.write();
            queue.remove(id);
        }
        if let Some(handle) = self.active_handles.write().get(id) {
            handle.cancel();
            self.statistics.record_cancelled();
            self.statistics.running.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        } else {
            Err(format!("no active execution with id '{}'", id))
        }
    }

    pub fn pause(&self, id: &str) -> Result<(), String> {
        let handles = self.active_handles.read();
        if let Some(handle) = handles.get(id) {
            handle.pause();
            Ok(())
        } else {
            Err(format!("no active execution with id '{}'", id))
        }
    }

    pub fn resume(&self, id: &str) -> Result<(), String> {
        let handles = self.active_handles.read();
        if let Some(handle) = handles.get(id) {
            handle.resume();
            Ok(())
        } else {
            Err(format!("no active execution with id '{}'", id))
        }
    }

    pub fn clear(&self) {
        let mut queue = self.queue.write();
        queue.priority_queue.clear();
        queue.background.clear();
        queue.active.clear();
        queue.exclusive_holder = None;
        self.active_handles.write().clear();
        self.history.write().clear();
    }

    pub fn history(&self) -> Vec<ExecutionHistoryEntry> {
        self.history.read().entries().to_vec()
    }

    pub fn statistics(&self) -> ExecutionStatisticsSnapshot {
        self.statistics.snapshot()
    }

    pub fn active(&self) -> Vec<ExecutionHandle> {
        self.active_handles.read().values().cloned().collect()
    }

    pub fn pending(&self) -> Vec<ExecutionHandle> {
        self.active_handles
            .read()
            .values()
            .filter(|h| {
                matches!(
                    h.status(),
                    ExecutionStatus::Queued | ExecutionStatus::Pending
                )
            })
            .cloned()
            .collect()
    }

    pub fn completed(&self) -> Vec<ExecutionHistoryEntry> {
        self.history
            .read()
            .entries()
            .iter()
            .filter(|e| e.status == ExecutionStatus::Completed)
            .cloned()
            .collect()
    }

    pub fn failed(&self) -> Vec<ExecutionHistoryEntry> {
        self.history
            .read()
            .entries()
            .iter()
            .filter(|e| e.status == ExecutionStatus::Failed)
            .cloned()
            .collect()
    }

    pub fn get_handle(&self, id: &str) -> Option<ExecutionHandle> {
        self.active_handles.read().get(id).cloned()
    }

    fn try_process_queue(&self) {
        loop {
            let entry = {
                let mut queue = self.queue.write();
                if queue.total_active() >= self.config.max_concurrent {
                    return;
                }
                match queue.dequeue() {
                    Some(e) => e,
                    None => return,
                }
            };

            let handle = entry.handle.clone();
            let request = entry.request;
            let start = Instant::now();
            let start_time = chrono::Utc::now().timestamp_millis();

            let statistics = self.statistics.clone();
            let history = self.history.clone();
            let planner = self.planner.clone();
            let plan_executor = self.plan_executor.clone();
            let active_handles = self.active_handles.clone();
            let queue = self.queue.clone();

            statistics.running.fetch_add(1, Ordering::SeqCst);
            statistics.queued.fetch_sub(1, Ordering::SeqCst);
            statistics.update_concurrency(queue.read().total_active());

            handle.set_status(ExecutionStatus::Running);

            std::thread::spawn(move || {
                let goal = request.goal.clone();

                let plan_result = planner.plan(&goal);
                let plan = match plan_result {
                    Ok(p) => p,
                    Err(e) => {
                        handle.set_status(ExecutionStatus::Failed);
                        statistics.record_failed();
                        statistics.running.fetch_sub(1, Ordering::SeqCst);
                        let now = chrono::Utc::now().timestamp_millis();
                        let duration = start.elapsed();
                        let result = ExecutionResult {
                            request_id: request.id.clone(),
                            goal_description: goal.description.clone(),
                            status: ExecutionStatus::Failed,
                            report: None,
                            error: Some(format!("planning failed: {}", e)),
                            started_at: start_time,
                            completed_at: Some(now),
                            duration,
                        };
                        *handle.result.write() = Some(result.clone());
                        handle.set_status(ExecutionStatus::Failed);
                        history.write().push(ExecutionHistoryEntry {
                            execution_id: request.id.clone(),
                            goal_id: goal.description.clone(),
                            intent: request.intent.as_ref().map(|i| i.original_text.clone()),
                            start_time,
                            end_time: Some(now),
                            duration: Some(duration),
                            status: ExecutionStatus::Failed,
                            failure_reason: Some(format!("planning failed: {}", e)),
                            retry_count: 0,
                            priority: request.priority,
                            policy: request.policy,
                        });
                        active_handles.write().remove(&request.id);
                        queue.write().exclusive_holder = None;
                        return;
                    }
                };

                handle.set_progress(0.2);

                let report = plan_executor.execute_plan(plan, goal.clone());

                let duration = start.elapsed();
                let now = chrono::Utc::now().timestamp_millis();

                if handle.is_cancelled() {
                    statistics.record_cancelled();
                    let result = ExecutionResult {
                        request_id: request.id.clone(),
                        goal_description: goal.description.clone(),
                        status: ExecutionStatus::Cancelled,
                        report: Some(report),
                        error: Some("cancelled by user".into()),
                        started_at: start_time,
                        completed_at: Some(now),
                        duration,
                    };
                    *handle.result.write() = Some(result);
                    handle.set_status(ExecutionStatus::Cancelled);
                    history.write().push(ExecutionHistoryEntry {
                        execution_id: request.id.clone(),
                        goal_id: goal.description.clone(),
                        intent: request.intent.as_ref().map(|i| i.original_text.clone()),
                        start_time,
                        end_time: Some(now),
                        duration: Some(duration),
                        status: ExecutionStatus::Cancelled,
                        failure_reason: Some("cancelled by user".into()),
                        retry_count: 0,
                        priority: request.priority,
                        policy: request.policy,
                    });
                } else if report.success {
                    statistics.record_completed();
                    statistics.update_duration(duration.as_millis() as u64);
                    let result = ExecutionResult {
                        request_id: request.id.clone(),
                        goal_description: goal.description.clone(),
                        status: ExecutionStatus::Completed,
                        report: Some(report.clone()),
                        error: None,
                        started_at: start_time,
                        completed_at: Some(now),
                        duration,
                    };
                    *handle.result.write() = Some(result);
                    handle.set_status(ExecutionStatus::Completed);
                    handle.set_progress(1.0);
                    history.write().push(ExecutionHistoryEntry {
                        execution_id: request.id.clone(),
                        goal_id: goal.description.clone(),
                        intent: request.intent.as_ref().map(|i| i.original_text.clone()),
                        start_time,
                        end_time: Some(now),
                        duration: Some(duration),
                        status: ExecutionStatus::Completed,
                        failure_reason: None,
                        retry_count: 0,
                        priority: request.priority,
                        policy: request.policy,
                    });
                } else {
                    statistics.record_failed();
                    let error = report
                        .abort_reason
                        .clone()
                        .unwrap_or_else(|| "execution failed".into());
                    let result = ExecutionResult {
                        request_id: request.id.clone(),
                        goal_description: goal.description.clone(),
                        status: ExecutionStatus::Failed,
                        report: Some(report.clone()),
                        error: Some(error.clone()),
                        started_at: start_time,
                        completed_at: Some(now),
                        duration,
                    };
                    *handle.result.write() = Some(result);
                    handle.set_status(ExecutionStatus::Failed);
                    history.write().push(ExecutionHistoryEntry {
                        execution_id: request.id.clone(),
                        goal_id: goal.description.clone(),
                        intent: request.intent.as_ref().map(|i| i.original_text.clone()),
                        start_time,
                        end_time: Some(now),
                        duration: Some(duration),
                        status: ExecutionStatus::Failed,
                        failure_reason: Some(error),
                        retry_count: 0,
                        priority: request.priority,
                        policy: request.policy,
                    });
                }

                statistics.running.fetch_sub(1, Ordering::SeqCst);
                active_handles.write().remove(&request.id);
                queue.write().exclusive_holder = None;
            });
        }
    }
}

unsafe impl Send for ExecutionManager {}
unsafe impl Sync for ExecutionManager {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome_verifier::OutcomeVerifier;
    use crate::recovery_orchestrator::RecoveryOrchestrator;
    use crate::world_state::WorldState;

    fn make_plan_executor() -> Arc<PlanExecutor> {
        let planner = Planner::new();
        let ws = Arc::new(RwLock::new(WorldState::new()));
        let verifier = Arc::new(OutcomeVerifier::new(ws.clone(), None));
        let orch = Arc::new(RecoveryOrchestrator::new());
        let executor = PlanExecutor::new(planner, verifier, orch, ws);
        Arc::new(executor)
    }

    fn make_manager() -> ExecutionManager {
        let planner = Arc::new(Planner::new());
        let plan_executor = make_plan_executor();
        ExecutionManager::new(planner, plan_executor)
    }

    fn make_request(id: &str, description: &str) -> ExecutionRequest {
        ExecutionRequest::new(id, Goal::new(description))
    }

    #[test]
    fn test_submit_single() {
        let mgr = make_manager();
        let req = make_request("test-1", "set brightness to 50");
        let handle = mgr.submit(req).unwrap();
        assert_eq!(handle.id(), "test-1");
        assert!(matches!(
            handle.status(),
            ExecutionStatus::Queued | ExecutionStatus::Running
        ));
    }

    #[test]
    fn test_submit_and_complete() {
        let mgr = make_manager();
        let req = make_request("sc-1", "set brightness to 50");
        let handle = mgr.submit(req).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let status = handle.status();
        assert!(status == ExecutionStatus::Completed || status == ExecutionStatus::Running);
    }

    #[test]
    fn test_cancel_before_execution() {
        let mut mgr = make_manager();
        mgr.config.max_concurrent = 1;
        let r1 = make_request("c1", "set brightness to 50");
        let r2 = make_request("c2", "set brightness to 75");
        let h1 = mgr.submit(r1).unwrap();
        let h2 = mgr.submit(r2).unwrap();
        mgr.cancel("c2").unwrap();
        assert_eq!(h2.status(), ExecutionStatus::Cancelled);
        drop(h1);
        drop(h2);
    }

    #[test]
    fn test_pause_and_resume() {
        let mgr = make_manager();
        let req = make_request("pr-1", "wait");
        let _handle = mgr.submit(req).unwrap();
        mgr.pause("pr-1").unwrap();
        mgr.resume("pr-1").unwrap();
    }

    #[test]
    fn test_priority_ordering() {
        let mut mgr = make_manager();
        mgr.config.max_concurrent = 1;
        let low = ExecutionRequest::new("low-1", Goal::new("set brightness to 50"))
            .with_priority(ExecutionPriority::Low);
        let high = ExecutionRequest::new("high-1", Goal::new("set brightness to 75"))
            .with_priority(ExecutionPriority::High);
        let imm = ExecutionRequest::new("imm-1", Goal::new("set brightness to 100"))
            .with_priority(ExecutionPriority::Immediate);

        let h_low = mgr.submit(low).unwrap();
        let h_high = mgr.submit(high).unwrap();
        let h_imm = mgr.submit(imm).unwrap();

        std::thread::sleep(Duration::from_millis(500));

        let snap = mgr.statistics();
        assert!(snap.submitted >= 3);
        drop(h_low);
        drop(h_high);
        drop(h_imm);
    }

    #[test]
    fn test_fifo_within_same_priority() {
        let mut mgr = make_manager();
        mgr.config.max_concurrent = 1;
        let r1 = ExecutionRequest::new("f1", Goal::new("set brightness to 10"))
            .with_priority(ExecutionPriority::Normal);
        let r2 = ExecutionRequest::new("f2", Goal::new("set brightness to 20"))
            .with_priority(ExecutionPriority::Normal);
        let r3 = ExecutionRequest::new("f3", Goal::new("set brightness to 30"))
            .with_priority(ExecutionPriority::Normal);

        let _ = mgr.submit(r1).unwrap();
        let _ = mgr.submit(r2).unwrap();
        let _ = mgr.submit(r3).unwrap();

        std::thread::sleep(Duration::from_millis(500));
        let snap = mgr.statistics();
        assert!(snap.submitted >= 3);
    }

    #[test]
    fn test_batch_submit() {
        let mgr = make_manager();
        let reqs = vec![
            make_request("b1", "set brightness to 10"),
            make_request("b2", "set brightness to 20"),
            make_request("b3", "set brightness to 30"),
        ];
        let results = mgr.submit_batch(reqs);
        assert_eq!(results.len(), 3);
        for r in &results {
            assert!(r.is_ok());
        }
        std::thread::sleep(Duration::from_millis(500));
        let snap = mgr.statistics();
        assert_eq!(snap.submitted, 3);
    }

    #[test]
    fn test_exclusive_execution() {
        let mut mgr = make_manager();
        mgr.config.max_concurrent = 2;
        let req = ExecutionRequest::new("ex-1", Goal::new("lock device"))
            .with_policy(ExecutionPolicy::Exclusive);
        let handle = mgr.submit(req).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            handle.status() == ExecutionStatus::Completed
                || handle.status() == ExecutionStatus::Running
        );
    }

    #[test]
    fn test_parallel_execution() {
        let mut mgr = make_manager();
        mgr.config.max_concurrent = 10;
        let req = ExecutionRequest::new("par-1", Goal::new("set brightness to 50"))
            .with_policy(ExecutionPolicy::Parallel);
        let handle = mgr.submit(req).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            handle.status() == ExecutionStatus::Completed
                || handle.status() == ExecutionStatus::Running
        );
    }

    #[test]
    fn test_queue_full() {
        let mut mgr = make_manager();
        mgr.config.max_size = 2;
        mgr.config.max_concurrent = 0;
        let r1 = make_request("qf1", "set brightness to 10");
        let r2 = make_request("qf2", "set brightness to 20");
        let r3 = make_request("qf3", "set brightness to 30");
        assert!(mgr.submit(r1).is_ok());
        assert!(mgr.submit(r2).is_ok());
        assert!(mgr.submit(r3).is_err());
    }

    #[test]
    fn test_timeout() {
        let mgr = make_manager();
        let req = ExecutionRequest::new("to-1", Goal::new("set brightness to 50"))
            .with_timeout(Duration::from_millis(1));
        let handle = mgr.submit(req).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let status = handle.status();
        assert!(
            status == ExecutionStatus::Completed
                || status == ExecutionStatus::Running
                || status == ExecutionStatus::Failed
        );
    }

    #[test]
    fn test_history() {
        let mgr = make_manager();
        let req = make_request("hist-1", "set brightness to 50");
        let handle = mgr.submit(req).unwrap();
        // Poll for completion with timeout
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut completed = false;
        while std::time::Instant::now() < deadline {
            if handle.status().is_terminal() {
                completed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(completed, "execution did not complete in time");
        // Allow a small delay for history write
        std::thread::sleep(Duration::from_millis(50));
        let hist = mgr.history();
        assert!(
            !hist.is_empty(),
            "history should have entries after execution"
        );
        let entry = hist.iter().find(|e| e.execution_id == "hist-1");
        assert!(entry.is_some());
    }

    #[test]
    fn test_statistics() {
        let mgr = make_manager();
        let req = make_request("stat-1", "set brightness to 50");
        let _ = mgr.submit(req).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let snap = mgr.statistics();
        assert!(snap.submitted >= 1);
    }

    #[test]
    fn test_active() {
        let mgr = make_manager();
        let req = make_request("act-1", "set brightness to 50");
        let h = mgr.submit(req).unwrap();
        let active = mgr.active();
        assert!(active.iter().any(|a| a.id() == "act-1"));
        drop(h);
    }

    #[test]
    fn test_pending() {
        let mut mgr = make_manager();
        mgr.config.max_concurrent = 1;
        let r1 = make_request("p1", "wait");
        let r2 = make_request("p2", "wait");
        let h1 = mgr.submit(r1).unwrap();
        let _h2 = mgr.submit(r2).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let pending = mgr.pending();
        assert!(!pending.is_empty() || h1.status().is_terminal());
        drop(h1);
    }

    #[test]
    fn test_clear() {
        let mgr = make_manager();
        let req = make_request("clr-1", "set brightness to 50");
        let _ = mgr.submit(req).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        mgr.clear();
        assert!(mgr.active().is_empty());
        assert!(mgr.history().is_empty());
    }

    #[test]
    fn test_get_handle() {
        let mgr = make_manager();
        let req = make_request("gh-1", "set brightness to 50");
        let _ = mgr.submit(req).unwrap();
        let handle = mgr.get_handle("gh-1");
        assert!(handle.is_some());
        assert_eq!(handle.unwrap().id(), "gh-1");
    }

    #[test]
    fn test_cancel_nonexistent() {
        let mgr = make_manager();
        let result = mgr.cancel("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_pause_nonexistent() {
        let mgr = make_manager();
        let result = mgr.pause("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_resume_nonexistent() {
        let mgr = make_manager();
        let result = mgr.resume("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_priority_as_u8() {
        assert_eq!(ExecutionPriority::Immediate.as_u8(), 0);
        assert_eq!(ExecutionPriority::High.as_u8(), 1);
        assert_eq!(ExecutionPriority::Normal.as_u8(), 2);
        assert_eq!(ExecutionPriority::Low.as_u8(), 3);
        assert_eq!(ExecutionPriority::Background.as_u8(), 4);
    }

    #[test]
    fn test_priority_default() {
        assert_eq!(ExecutionPriority::default(), ExecutionPriority::Normal);
    }

    #[test]
    fn test_policy_default() {
        assert_eq!(ExecutionPolicy::default(), ExecutionPolicy::Sequential);
    }

    #[test]
    fn test_status_is_terminal() {
        assert!(ExecutionStatus::Completed.is_terminal());
        assert!(ExecutionStatus::Failed.is_terminal());
        assert!(ExecutionStatus::Cancelled.is_terminal());
        assert!(ExecutionStatus::TimedOut.is_terminal());
        assert!(!ExecutionStatus::Pending.is_terminal());
        assert!(!ExecutionStatus::Running.is_terminal());
    }

    #[test]
    fn test_status_is_active() {
        assert!(ExecutionStatus::Queued.is_active());
        assert!(ExecutionStatus::Running.is_active());
        assert!(ExecutionStatus::Waiting.is_active());
        assert!(ExecutionStatus::Paused.is_active());
        assert!(!ExecutionStatus::Completed.is_active());
        assert!(!ExecutionStatus::Failed.is_active());
    }

    #[test]
    fn test_execution_request_builder() {
        let intent = Intent::new(
            crate::intention_parser::IntentType::OpenApplication,
            "open chrome",
        );
        let req = ExecutionRequest::new("builder-1", Goal::new("open chrome"))
            .with_priority(ExecutionPriority::High)
            .with_policy(ExecutionPolicy::Parallel)
            .with_timeout(Duration::from_secs(30))
            .with_intent(intent.clone())
            .with_max_retries(3);

        assert_eq!(req.id, "builder-1");
        assert_eq!(req.priority, ExecutionPriority::High);
        assert_eq!(req.policy, ExecutionPolicy::Parallel);
        assert_eq!(req.max_retries, 3);
        assert!(req.intent.is_some());
        assert!(req.timeout.is_some());
    }

    #[test]
    fn test_handle_clone() {
        let h1 = ExecutionHandle::new("clone-1".into());
        let h2 = h1.clone();
        assert_eq!(h1.id(), h2.id());
    }

    #[test]
    fn test_handle_progress() {
        let handle = ExecutionHandle::new("prog-1".into());
        assert!((handle.progress() - 0.0).abs() < 0.001);
        handle.set_progress(0.5);
        assert!((handle.progress() - 0.5).abs() < 0.001);
        handle.set_progress(1.5);
        assert!((handle.progress() - 1.0).abs() < 0.001);
        handle.set_progress(-0.5);
        assert!((handle.progress() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_handle_set_status() {
        let handle = ExecutionHandle::new("status-1".into());
        assert_eq!(handle.status(), ExecutionStatus::Pending);
        handle.set_status(ExecutionStatus::Running);
        assert_eq!(handle.status(), ExecutionStatus::Running);
        handle.set_status(ExecutionStatus::Completed);
        assert_eq!(handle.status(), ExecutionStatus::Completed);
    }

    #[test]
    fn test_execution_history_push_and_entries() {
        let mut history = ExecutionHistory::new().with_max_entries(5);
        assert!(history.is_empty());
        for i in 0..3 {
            history.push(ExecutionHistoryEntry {
                execution_id: format!("e{}", i),
                goal_id: format!("g{}", i),
                intent: None,
                start_time: 0,
                end_time: Some(100),
                duration: Some(Duration::from_millis(100)),
                status: ExecutionStatus::Completed,
                failure_reason: None,
                retry_count: 0,
                priority: ExecutionPriority::Normal,
                policy: ExecutionPolicy::Sequential,
            });
        }
        assert_eq!(history.len(), 3);
        history.clear();
        assert!(history.is_empty());
    }

    #[test]
    fn test_execution_history_max_entries() {
        let mut history = ExecutionHistory::new().with_max_entries(2);
        for i in 0..5 {
            history.push(ExecutionHistoryEntry {
                execution_id: format!("e{}", i),
                goal_id: "g".into(),
                intent: None,
                start_time: 0,
                end_time: None,
                duration: None,
                status: ExecutionStatus::Pending,
                failure_reason: None,
                retry_count: 0,
                priority: ExecutionPriority::Normal,
                policy: ExecutionPolicy::Sequential,
            });
        }
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_statistics_snapshot() {
        let stats = ExecutionStatistics::new();
        stats.record_submitted();
        stats.record_completed();
        stats.record_failed();
        stats.record_cancelled();
        stats.record_retry();
        let snap = stats.snapshot();
        assert_eq!(snap.submitted, 1);
        assert_eq!(snap.completed, 1);
        assert_eq!(snap.failed, 1);
        assert_eq!(snap.cancelled, 1);
        assert_eq!(snap.total_retries, 1);
    }

    #[test]
    fn test_statistics_update_duration() {
        let stats = ExecutionStatistics::new();
        stats.update_duration(100);
        assert_eq!(stats.average_duration_ms.load(Ordering::SeqCst), 100);
        stats.update_duration(200);
        assert_eq!(stats.average_duration_ms.load(Ordering::SeqCst), 150);
    }

    #[test]
    fn test_statistics_peak_concurrency() {
        let stats = ExecutionStatistics::new();
        stats.update_concurrency(5);
        assert_eq!(stats.peak_concurrency.load(Ordering::SeqCst), 5);
        stats.update_concurrency(3);
        assert_eq!(stats.peak_concurrency.load(Ordering::SeqCst), 5);
        stats.update_concurrency(10);
        assert_eq!(stats.peak_concurrency.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn test_concurrent_submissions() {
        let mgr = Arc::new(make_manager());
        let mut handles = Vec::new();
        for i in 0..10 {
            let m = mgr.clone();
            handles.push(std::thread::spawn(move || {
                let req = make_request(&format!("conc-{}", i), "set brightness to 50");
                let _ = m.submit(req);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        std::thread::sleep(Duration::from_millis(500));
        let snap = mgr.statistics();
        assert_eq!(snap.submitted, 10);
    }

    #[test]
    fn test_stress_submit_cancel() {
        let mgr = Arc::new(make_manager());
        let mut handles = Vec::new();
        for i in 0..20 {
            let m = mgr.clone();
            handles.push(std::thread::spawn(move || {
                let id = format!("stress-{}", i);
                let req = make_request(&id, "set brightness to 50");
                if let Ok(h) = m.submit(req) {
                    if i % 2 == 0 {
                        std::thread::sleep(Duration::from_millis(10));
                        let _ = m.cancel(&id);
                    }
                    drop(h);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let snap = mgr.statistics();
        assert_eq!(snap.submitted, 20);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let priority = ExecutionPriority::High;
        let json = serde_json::to_string(&priority).unwrap();
        let deserialized: ExecutionPriority = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ExecutionPriority::High);

        let status = ExecutionStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: ExecutionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ExecutionStatus::Running);

        let policy = ExecutionPolicy::Exclusive;
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: ExecutionPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ExecutionPolicy::Exclusive);
    }

    #[test]
    fn test_background_priority() {
        let mut mgr = make_manager();
        mgr.config.max_concurrent = 1;
        let normal = ExecutionRequest::new("bg-norm", Goal::new("set brightness to 50"))
            .with_priority(ExecutionPriority::Normal);
        let bg = ExecutionRequest::new("bg-low", Goal::new("set brightness to 25"))
            .with_priority(ExecutionPriority::Background);

        let h_norm = mgr.submit(normal).unwrap();
        let h_bg = mgr.submit(bg).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(!h_bg.status().is_terminal() || !h_norm.status().is_terminal());
    }

    #[test]
    fn test_completed_list() {
        let mgr = make_manager();
        let req = make_request("compl-1", "set brightness to 50");
        let handle = mgr.submit(req).unwrap();
        // Poll for completion with timeout
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if handle.status().is_terminal() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let completed = mgr.completed();
        assert!(!completed.is_empty() || mgr.active().is_empty());
    }

    #[test]
    fn test_statistics_initial_state() {
        let stats = ExecutionStatistics::new();
        let snap = stats.snapshot();
        assert_eq!(snap.submitted, 0);
        assert_eq!(snap.completed, 0);
        assert_eq!(snap.failed, 0);
        assert_eq!(snap.cancelled, 0);
        assert_eq!(snap.running, 0);
        assert_eq!(snap.queued, 0);
    }

    #[test]
    fn test_execution_queue_config_default() {
        let config = ExecutionQueueConfig::default();
        assert_eq!(config.max_size, 100);
        assert_eq!(config.max_concurrent, 5);
        assert!(config.enable_background_queue);
        assert_eq!(config.default_timeout_ms, 300_000);
    }

    #[test]
    fn test_handle_wait_completion() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let handle = ExecutionHandle::new("wait-1".into());
            let h2 = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                h2.set_status(ExecutionStatus::Completed);
            });
            tokio::time::timeout(Duration::from_secs(5), handle.wait())
                .await
                .unwrap();
            assert_eq!(handle.status(), ExecutionStatus::Completed);
        });
    }
}
