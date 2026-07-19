use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;

use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::agent_runtime::AgentRuntime;
use crate::planner::Goal;

// ---------------------------------------------------------------------------
// GoalPriority
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum GoalPriority {
    Critical = 0,
    High = 1,
    Normal = 2,
    Low = 3,
    Background = 4,
}

impl GoalPriority {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Compute an adjusted priority score (lower = more urgent).
    /// Factors: age in queue, retry count, deadline proximity.
    pub fn adjust(self, age_ms: i64, retries: u32, deadline: Option<i64>) -> AdjustedPriority {
        let mut score = (self.as_u8() as i64) * 1000;

        let age_hours = age_ms as f64 / 3_600_000.0;
        if age_hours > 0.5 {
            score = (score as f64 - (age_hours * 100.0).min(500.0)) as i64;
        }

        score = (score as f64 - (retries as f64 * 200.0).min(600.0)) as i64;

        if let Some(deadline_ms) = deadline {
            let remaining = deadline_ms - Utc::now().timestamp_millis();
            if remaining > 0 && remaining < 300_000 {
                score = -1000;
            } else if remaining > 0 && remaining < 3_600_000 {
                score = score.min(-100) - 500;
            }
        }

        AdjustedPriority {
            score: score.max(-2000),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AdjustedPriority {
    score: i64,
}

// ---------------------------------------------------------------------------
// Resource
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Resource {
    Screen,
    Keyboard,
    Mouse,
    Clipboard,
    Audio,
}

// ---------------------------------------------------------------------------
// OrchestratedGoalState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrchestratedGoalState {
    Queued,
    Delayed { until: i64 },
    WaitingForDependencies,
    Running,
    Paused,
    Completed,
    Failed(String),
    Cancelled,
}

impl OrchestratedGoalState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrchestratedGoalState::Completed
                | OrchestratedGoalState::Failed(_)
                | OrchestratedGoalState::Cancelled
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            OrchestratedGoalState::Running | OrchestratedGoalState::Paused
        )
    }

    pub fn is_queued(&self) -> bool {
        matches!(
            self,
            OrchestratedGoalState::Queued
                | OrchestratedGoalState::Delayed { .. }
                | OrchestratedGoalState::WaitingForDependencies
        )
    }
}

// ---------------------------------------------------------------------------
// OrchestratedGoal
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OrchestratedGoal {
    pub id: String,
    pub goal: Goal,
    pub priority: GoalPriority,
    pub dependencies: Vec<String>,
    pub state: OrchestratedGoalState,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub deadline: Option<i64>,
    pub retry_count: u32,
    pub recovery_count: u32,
    pub required_resources: Vec<Resource>,
    pub exclusive: bool,
    pub session_id: Option<String>,
    pub tags: HashMap<String, String>,
}

impl OrchestratedGoal {
    pub fn new(id: impl Into<String>, goal: Goal, priority: GoalPriority) -> Self {
        Self {
            id: id.into(),
            goal,
            priority,
            dependencies: Vec::new(),
            state: OrchestratedGoalState::Queued,
            created_at: Utc::now().timestamp_millis(),
            started_at: None,
            completed_at: None,
            deadline: None,
            retry_count: 0,
            recovery_count: 0,
            required_resources: Vec::new(),
            exclusive: false,
            session_id: None,
            tags: HashMap::new(),
        }
    }

    pub fn with_dependency(mut self, dep_id: impl Into<String>) -> Self {
        self.dependencies.push(dep_id.into());
        self
    }

    pub fn with_deadline(mut self, deadline_ms: i64) -> Self {
        self.deadline = Some(deadline_ms);
        self
    }

    pub fn with_resource(mut self, resource: Resource) -> Self {
        self.required_resources.push(resource);
        self
    }

    pub fn with_exclusive(mut self) -> Self {
        self.exclusive = true;
        self
    }

    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Internal queue entry types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Eq, PartialEq)]
struct PriorityEntry {
    adjusted_score: i64,
    created_at: i64,
    goal_id: String,
}

impl Ord for PriorityEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.adjusted_score
            .cmp(&other.adjusted_score)
            .then(self.created_at.cmp(&other.created_at))
            .then(self.goal_id.cmp(&other.goal_id))
    }
}

impl PartialOrd for PriorityEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct DelayedEntry {
    scheduled_at: i64,
    goal_id: String,
}

impl Ord for DelayedEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.scheduled_at
            .cmp(&other.scheduled_at)
            .then(self.goal_id.cmp(&other.goal_id))
    }
}

impl PartialOrd for DelayedEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// ---------------------------------------------------------------------------
// ResourceLock
// ---------------------------------------------------------------------------

pub struct ResourceLock {
    locks: RwLock<HashSet<Resource>>,
}

impl ResourceLock {
    pub fn new() -> Self {
        Self {
            locks: RwLock::new(HashSet::new()),
        }
    }

    /// Acquire the given resources. Returns an error if any is already held.
    pub fn acquire(&self, resources: &[Resource]) -> Result<(), String> {
        let mut locks = self.locks.write();
        for r in resources {
            if locks.contains(r) {
                return Err(format!("resource '{:?}' is already locked", r));
            }
        }
        for r in resources {
            locks.insert(*r);
        }
        Ok(())
    }

    /// Release the given resources.
    pub fn release(&self, resources: &[Resource]) {
        let mut locks = self.locks.write();
        for r in resources {
            locks.remove(r);
        }
    }

    /// Check if a resource is currently held.
    pub fn is_held(&self, resource: &Resource) -> bool {
        self.locks.read().contains(resource)
    }

    /// Get all currently held resources.
    pub fn held_resources(&self) -> Vec<Resource> {
        self.locks.read().iter().copied().collect()
    }

    /// Check if any of the given resources conflict with currently held ones.
    pub fn has_conflict(&self, resources: &[Resource]) -> bool {
        let locks = self.locks.read();
        resources.iter().any(|r| locks.contains(r))
    }
}

impl Default for ResourceLock {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// GoalQueue
// ---------------------------------------------------------------------------

pub struct GoalQueue {
    fifo: VecDeque<String>,
    priority_queue: BTreeSet<PriorityEntry>,
    delayed: BTreeSet<DelayedEntry>,
}

impl GoalQueue {
    pub fn new() -> Self {
        Self {
            fifo: VecDeque::new(),
            priority_queue: BTreeSet::new(),
            delayed: BTreeSet::new(),
        }
    }

    /// Enqueue a goal. If delay_ms > 0, it goes to the delayed queue.
    pub fn enqueue(
        &mut self,
        goal_id: &str,
        priority: GoalPriority,
        created_at: i64,
        delay_ms: u64,
    ) {
        if delay_ms > 0 {
            let scheduled_at = Utc::now().timestamp_millis() + delay_ms as i64;
            self.delayed.insert(DelayedEntry {
                scheduled_at,
                goal_id: goal_id.to_string(),
            });
        } else {
            let score = priority.adjust(0, 0, None);
            self.priority_queue.insert(PriorityEntry {
                adjusted_score: score.score,
                created_at,
                goal_id: goal_id.to_string(),
            });
            self.fifo.push_back(goal_id.to_string());
        }
    }

    /// Remove a goal from all internal structures.
    pub fn remove(&mut self, goal_id: &str) {
        self.fifo.retain(|id| id != goal_id);
        self.priority_queue.retain(|e| e.goal_id != goal_id);
        self.delayed.retain(|e| e.goal_id != goal_id);
    }

    /// Peek at the next ready goal (highest priority).
    pub fn peek(&self) -> Option<String> {
        self.priority_queue.iter().next().map(|e| e.goal_id.clone())
    }

    /// Dequeue the highest-priority goal. Returns the goal ID.
    pub fn dequeue(&mut self) -> Option<String> {
        let entry = self.priority_queue.iter().next().cloned()?;
        self.priority_queue.remove(&entry);
        self.fifo.retain(|id| id != &entry.goal_id);
        Some(entry.goal_id)
    }

    /// Get all delayed goals that are now due. Returns their goal IDs.
    pub fn pop_due_delayed(&mut self) -> Vec<String> {
        let now = Utc::now().timestamp_millis();
        let due: Vec<DelayedEntry> = self
            .delayed
            .iter()
            .take_while(|e| e.scheduled_at <= now)
            .cloned()
            .collect();
        let mut result = Vec::new();
        for entry in due {
            self.delayed.remove(&entry);
            result.push(entry.goal_id);
        }
        result
    }

    /// Returns the number of queued items (excluding delayed).
    pub fn len(&self) -> usize {
        self.priority_queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.priority_queue.is_empty()
    }

    /// Get all goal IDs currently in the queue (for debugging).
    pub fn all_ids(&self) -> Vec<String> {
        self.priority_queue
            .iter()
            .map(|e| e.goal_id.clone())
            .collect()
    }

    /// Get all delayed goal IDs.
    pub fn delayed_ids(&self) -> Vec<String> {
        self.delayed.iter().map(|e| e.goal_id.clone()).collect()
    }
}

impl Default for GoalQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// OrchestratorMetrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrchestratorMetrics {
    pub total_submitted: u64,
    pub total_completed: u64,
    pub total_failed: u64,
    pub total_cancelled: u64,
    pub total_retries: u64,
    pub total_recoveries: u64,
    pub cumulative_queue_wait_ms: i64,
    pub cumulative_execution_ms: i64,
    pub peak_concurrent: usize,
}

// ---------------------------------------------------------------------------
// GoalOrchestratorConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GoalOrchestratorConfig {
    pub max_concurrent_goals: usize,
    pub max_background_goals: usize,
    pub max_queued_goals: usize,
    pub starvation_check_interval_ms: u64,
    pub enable_dynamic_priority: bool,
}

impl Default for GoalOrchestratorConfig {
    fn default() -> Self {
        Self {
            max_concurrent_goals: 5,
            max_background_goals: 2,
            max_queued_goals: 100,
            starvation_check_interval_ms: 30_000,
            enable_dynamic_priority: true,
        }
    }
}

// ---------------------------------------------------------------------------
// GoalOrchestrator
// ---------------------------------------------------------------------------

pub struct GoalOrchestrator {
    goals: RwLock<HashMap<String, OrchestratedGoal>>,
    queue: RwLock<GoalQueue>,
    resource_lock: Arc<ResourceLock>,
    metrics: RwLock<OrchestratorMetrics>,
    config: GoalOrchestratorConfig,
    runtime: RwLock<Option<Arc<AgentRuntime>>>,
    running_ids: RwLock<HashSet<String>>,
}

impl GoalOrchestrator {
    pub fn new(config: GoalOrchestratorConfig) -> Self {
        Self {
            goals: RwLock::new(HashMap::new()),
            queue: RwLock::new(GoalQueue::new()),
            resource_lock: Arc::new(ResourceLock::new()),
            metrics: RwLock::new(OrchestratorMetrics::default()),
            config,
            runtime: RwLock::new(None),
            running_ids: RwLock::new(HashSet::new()),
        }
    }

    pub fn with_runtime(self, runtime: Arc<AgentRuntime>) -> Self {
        *self.runtime.write() = Some(runtime);
        self
    }

    pub fn set_runtime(&self, runtime: Arc<AgentRuntime>) {
        *self.runtime.write() = Some(runtime);
    }

    pub fn resource_lock(&self) -> &ResourceLock {
        &self.resource_lock
    }

    pub fn config(&self) -> GoalOrchestratorConfig {
        self.config.clone()
    }

    pub fn metrics(&self) -> OrchestratorMetrics {
        self.metrics.read().clone()
    }

    // -- DAG helpers --

    fn has_cycle_internal(
        goal_id: &str,
        all_goals: &HashMap<String, OrchestratedGoal>,
        visited: &mut HashSet<String>,
        in_progress: &mut HashSet<String>,
    ) -> bool {
        if in_progress.contains(goal_id) {
            return true;
        }
        if visited.contains(goal_id) {
            return false;
        }
        visited.insert(goal_id.to_string());
        in_progress.insert(goal_id.to_string());
        if let Some(goal) = all_goals.get(goal_id) {
            for dep in &goal.dependencies {
                if Self::has_cycle_internal(dep, all_goals, visited, in_progress) {
                    in_progress.remove(goal_id);
                    return true;
                }
            }
        }
        in_progress.remove(goal_id);
        false
    }

    fn detect_cycle_for(goal_id: &str, all_goals: &HashMap<String, OrchestratedGoal>) -> bool {
        let mut visited = HashSet::new();
        let mut in_progress = HashSet::new();
        Self::has_cycle_internal(goal_id, all_goals, &mut visited, &mut in_progress)
    }

    fn validate_dependencies(
        dependencies: &[String],
        all_goals: &HashMap<String, OrchestratedGoal>,
        exclude_id: Option<&str>,
    ) -> Result<(), String> {
        for dep_id in dependencies {
            if dep_id.is_empty() {
                return Err("dependency ID cannot be empty".into());
            }
            if let Some(exclude) = exclude_id {
                if dep_id == exclude {
                    return Err(format!("goal cannot depend on itself: '{}'", dep_id));
                }
            }
            if !all_goals.contains_key(dep_id) {
                return Err(format!("dependency '{}' does not exist", dep_id));
            }
        }
        Ok(())
    }

    // ======================================================================
    // Public APIs
    // ======================================================================

    /// Submit a goal for orchestration.
    pub fn submit_goal(
        &self,
        goal: Goal,
        priority: GoalPriority,
        delay_ms: u64,
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let og = OrchestratedGoal::new(&id, goal, priority);
        {
            let mut goals = self.goals.write();
            if goals.len() >= self.config.max_queued_goals {
                return Err("maximum queued goals reached".into());
            }
            goals.insert(id.clone(), og);
        }
        {
            let mut q = self.queue.write();
            let now = Utc::now().timestamp_millis();
            q.enqueue(&id, priority, now, delay_ms);
        }
        self.metrics.write().total_submitted += 1;
        Ok(id)
    }

    /// Cancel a goal by ID. Only possible if not in a terminal state.
    pub fn cancel_goal(&self, goal_id: &str) -> Result<(), String> {
        let mut goals = self.goals.write();
        let goal = goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal '{}' not found", goal_id))?;
        if goal.state.is_terminal() {
            return Err(format!(
                "goal '{}' already in terminal state {:?}",
                goal_id, goal.state
            ));
        }
        let resources = goal.required_resources.clone();
        goal.state = OrchestratedGoalState::Cancelled;
        goal.completed_at = Some(Utc::now().timestamp_millis());
        drop(goals);
        self.release_resources_for(&resources);
        self.queue.write().remove(goal_id);
        self.running_ids.write().remove(goal_id);
        self.metrics.write().total_cancelled += 1;
        Ok(())
    }

    /// Pause a running goal.
    pub fn pause_goal(&self, goal_id: &str) -> Result<(), String> {
        let mut goals = self.goals.write();
        let goal = goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal '{}' not found", goal_id))?;
        if goal.state != OrchestratedGoalState::Running {
            return Err(format!("cannot pause goal in state {:?}", goal.state));
        }
        goal.state = OrchestratedGoalState::Paused;
        self.running_ids.write().remove(goal_id);
        Ok(())
    }

    /// Resume a paused goal.
    pub fn resume_goal(&self, goal_id: &str) -> Result<(), String> {
        let mut goals = self.goals.write();
        let goal = goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal '{}' not found", goal_id))?;
        if goal.state != OrchestratedGoalState::Paused {
            return Err(format!("cannot resume goal in state {:?}", goal.state));
        }
        goal.state = OrchestratedGoalState::Queued;
        let now = Utc::now().timestamp_millis();
        self.queue.write().enqueue(goal_id, goal.priority, now, 0);
        Ok(())
    }

    /// Get the current status of a goal.
    pub fn goal_status(&self, goal_id: &str) -> Option<OrchestratedGoalState> {
        self.goals.read().get(goal_id).map(|g| g.state.clone())
    }

    /// Get all queued (non-active, non-terminal) goals.
    pub fn queued_goals(&self) -> Vec<OrchestratedGoal> {
        self.goals
            .read()
            .values()
            .filter(|g| g.state.is_queued())
            .cloned()
            .collect()
    }

    /// Get all currently running goals.
    pub fn running_goals(&self) -> Vec<OrchestratedGoal> {
        self.goals
            .read()
            .values()
            .filter(|g| g.state.is_active())
            .cloned()
            .collect()
    }

    /// Get all completed goals.
    pub fn completed_goals(&self) -> Vec<OrchestratedGoal> {
        self.goals
            .read()
            .values()
            .filter(|g| matches!(g.state, OrchestratedGoalState::Completed))
            .cloned()
            .collect()
    }

    /// Get all failed goals.
    pub fn failed_goals(&self) -> Vec<OrchestratedGoal> {
        self.goals
            .read()
            .values()
            .filter(|g| matches!(g.state, OrchestratedGoalState::Failed(_)))
            .cloned()
            .collect()
    }

    /// Get the dependency list for a goal.
    pub fn dependencies(&self, goal_id: &str) -> Option<Vec<String>> {
        self.goals
            .read()
            .get(goal_id)
            .map(|g| g.dependencies.clone())
    }

    /// Get the priority of a goal.
    pub fn priority(&self, goal_id: &str) -> Option<GoalPriority> {
        self.goals.read().get(goal_id).map(|g| g.priority)
    }

    /// Submit a goal with dependencies.
    pub fn submit_goal_with_deps(
        &self,
        goal: Goal,
        priority: GoalPriority,
        dependencies: Vec<String>,
        delay_ms: u64,
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        // Validate dependencies before inserting
        {
            let goals = self.goals.read();
            Self::validate_dependencies(&dependencies, &goals, Some(&id))?;
            // Check for cycles
            let mut test_goals = goals.clone();
            let mut og = OrchestratedGoal::new(&id, goal.clone(), priority);
            og.dependencies = dependencies.clone();
            test_goals.insert(id.clone(), og);
            if Self::detect_cycle_for(&id, &test_goals) {
                return Err(
                    "submitting this goal would create a cycle in the dependency graph".into(),
                );
            }
        }

        let state = if dependencies.is_empty() {
            OrchestratedGoalState::Queued
        } else {
            OrchestratedGoalState::WaitingForDependencies
        };

        let has_deps = !dependencies.is_empty();
        let mut og = OrchestratedGoal::new(&id, goal, priority);
        og.dependencies = dependencies;
        og.state = state;

        {
            let mut goals = self.goals.write();
            if goals.len() >= self.config.max_queued_goals {
                return Err("maximum queued goals reached".into());
            }
            goals.insert(id.clone(), og);
        }

        if !has_deps {
            let mut q = self.queue.write();
            let now = Utc::now().timestamp_millis();
            q.enqueue(&id, priority, now, delay_ms);
        }

        self.metrics.write().total_submitted += 1;
        Ok(id)
    }

    // ======================================================================
    // Tick — process queue, start goals, update state
    // ======================================================================

    /// Run one scheduler tick. Returns IDs of goals whose state changed.
    pub fn tick(&self) -> Vec<String> {
        let mut changed = Vec::new();

        // 1. Process delayed goals → enqueue
        {
            let mut q = self.queue.write();
            let due = q.pop_due_delayed();
            for id in &due {
                if let Some(goal) = self.goals.read().get(id) {
                    q.enqueue(id, goal.priority, goal.created_at, 0);
                }
                changed.push(id.clone());
            }
        }

        // 2. Resolve dependencies
        {
            let goals = self.goals.read();
            let ready: Vec<String> = goals
                .values()
                .filter(|g| g.state == OrchestratedGoalState::WaitingForDependencies)
                .filter(|g| {
                    g.dependencies.iter().all(|dep_id| {
                        goals.get(dep_id).is_some_and(|dep| {
                            dep.state.is_terminal()
                                && matches!(dep.state, OrchestratedGoalState::Completed)
                        })
                    })
                })
                .map(|g| g.id.clone())
                .collect();
            drop(goals);

            if !ready.is_empty() {
                let mut goals = self.goals.write();
                let mut q = self.queue.write();
                for id in &ready {
                    if let Some(g) = goals.get_mut(id) {
                        g.state = OrchestratedGoalState::Queued;
                        q.enqueue(id, g.priority, g.created_at, 0);
                    }
                    changed.push(id.clone());
                }
            }
        }

        // 3. Dequeue and start goals (up to max concurrent)
        {
            let running = self.running_ids.read().len();
            let max_new = self.config.max_concurrent_goals.saturating_sub(running);

            if max_new > 0 {
                let mut to_start: Vec<String> = Vec::new();
                let mut q = self.queue.write();

                // Check if any running goal is exclusive
                let has_exclusive_running = {
                    let running = self.running_ids.read();
                    let goals = self.goals.read();
                    running
                        .iter()
                        .any(|rid| goals.get(rid).is_some_and(|rg| rg.exclusive))
                };

                if !has_exclusive_running {
                    for _ in 0..max_new {
                        // Stop if a goal already picked for start is exclusive
                        if to_start
                            .iter()
                            .any(|id| self.goals.read().get(id).is_some_and(|g| g.exclusive))
                        {
                            break;
                        }

                        let next = q.peek();
                        match next {
                            Some(id) => {
                                let ready = {
                                    let goals = self.goals.read();
                                    goals.get(&id).is_some_and(|g| {
                                        g.state == OrchestratedGoalState::Queued
                                            && !self
                                                .resource_lock
                                                .has_conflict(&g.required_resources)
                                            && self.can_start_background(g, to_start.len())
                                    })
                                };
                                if ready {
                                    if let Some(id) = q.dequeue() {
                                        to_start.push(id);
                                    }
                                } else {
                                    // Check exclusive or resource conflict at queue head
                                    if let Some(id) = &q.peek() {
                                        let goals = self.goals.read();
                                        if let Some(g) = goals.get(id) {
                                            if g.exclusive {
                                                break;
                                            }
                                            if self
                                                .resource_lock
                                                .has_conflict(&g.required_resources)
                                            {
                                                break;
                                            }
                                        }
                                    }
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }

                for id in &to_start {
                    let result = self.start_goal_inner(id);
                    if result.is_ok() {
                        changed.push(id.clone());
                    }
                }
            }
        }

        // 4. Update dynamic priorities for aging goals
        if self.config.enable_dynamic_priority {
            self.reprioritize_aging_goals();
        }

        changed
    }

    fn can_start_background(&self, goal: &OrchestratedGoal, about_to_start: usize) -> bool {
        if goal.priority != GoalPriority::Background {
            return true;
        }
        let running = self
            .goals
            .read()
            .values()
            .filter(|g| {
                g.state == OrchestratedGoalState::Running && g.priority == GoalPriority::Background
            })
            .count();
        running + about_to_start < self.config.max_background_goals
    }

    fn start_goal_inner(&self, goal_id: &str) -> Result<String, String> {
        let mut goals = self.goals.write();
        let goal = goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal '{}' not found", goal_id))?;

        // Acquire resources
        if !goal.required_resources.is_empty() {
            self.resource_lock
                .acquire(&goal.required_resources)
                .map_err(|e| format!("resource lock failed: {}", e))?;
        }

        goal.state = OrchestratedGoalState::Running;
        goal.started_at = Some(Utc::now().timestamp_millis());
        self.running_ids.write().insert(goal_id.to_string());

        // Update peak concurrent
        let running_count = self.running_ids.read().len();
        let mut metrics = self.metrics.write();
        if running_count > metrics.peak_concurrent {
            metrics.peak_concurrent = running_count;
        }

        // Record queue wait time
        if let Some(started) = goal.started_at {
            let wait = started - goal.created_at;
            if wait > 0 {
                metrics.cumulative_queue_wait_ms += wait;
            }
        }

        // Try to create a runtime session if runtime is available
        if let Some(ref rt) = *self.runtime.read() {
            match rt.create_session(goal.goal.clone()) {
                Ok(session_id) => {
                    goal.session_id = Some(session_id.clone());
                    let _ = rt.start_session(&session_id);
                }
                Err(e) => {
                    // Session creation failed — mark goal as failed
                    goal.state =
                        OrchestratedGoalState::Failed(format!("session creation failed: {}", e));
                    self.running_ids.write().remove(goal_id);
                    self.resource_lock.release(&goal.required_resources);
                    metrics.total_failed += 1;
                    return Err(e);
                }
            }
        }

        Ok(goal_id.to_string())
    }

    fn release_resources_for(&self, resources: &[Resource]) {
        if !resources.is_empty() {
            self.resource_lock.release(resources);
        }
    }

    fn reprioritize_aging_goals(&self) {
        let now = Utc::now().timestamp_millis();
        let goals = self.goals.read();
        let mut q = self.queue.write();

        // Collect entries that need re-prioritization
        let to_reinsert: Vec<String> = q
            .all_ids()
            .into_iter()
            .filter(|id| {
                goals.get(id).is_some_and(|g| {
                    now - g.created_at > self.config.starvation_check_interval_ms as i64
                })
            })
            .collect();

        for id in &to_reinsert {
            q.remove(id);
            if let Some(g) = goals.get(id) {
                q.enqueue(id, g.priority, g.created_at, 0);
            }
        }
    }

    /// Mark a goal as completed (called externally when a session finishes).
    pub fn complete_goal(
        &self,
        goal_id: &str,
        success: bool,
        error: Option<String>,
    ) -> Result<(), String> {
        let mut goals = self.goals.write();
        let goal = goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal '{}' not found", goal_id))?;

        if goal.state != OrchestratedGoalState::Running {
            return Err(format!("goal '{}' is not running", goal_id));
        }

        let now = Utc::now().timestamp_millis();
        goal.completed_at = Some(now);

        if let Some(started) = goal.started_at {
            let exec_time = now - started;
            if exec_time > 0 {
                self.metrics.write().cumulative_execution_ms += exec_time;
            }
        }

        if success {
            goal.state = OrchestratedGoalState::Completed;
            self.metrics.write().total_completed += 1;
        } else {
            let err_msg = error.unwrap_or_else(|| "unknown error".into());
            goal.state = OrchestratedGoalState::Failed(err_msg);
            self.metrics.write().total_failed += 1;
        }

        self.running_ids.write().remove(goal_id);
        self.resource_lock.release(&goal.required_resources);

        Ok(())
    }

    /// Record a retry for a goal.
    pub fn record_retry(&self, goal_id: &str) -> Result<(), String> {
        let mut goals = self.goals.write();
        let goal = goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal '{}' not found", goal_id))?;
        goal.retry_count += 1;
        self.metrics.write().total_retries += 1;
        Ok(())
    }

    /// Record a recovery for a goal.
    pub fn record_recovery(&self, goal_id: &str) -> Result<(), String> {
        let mut goals = self.goals.write();
        let goal = goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal '{}' not found", goal_id))?;
        goal.recovery_count += 1;
        self.metrics.write().total_recoveries += 1;
        Ok(())
    }

    /// Get all goals (for inspection).
    pub fn all_goals(&self) -> Vec<OrchestratedGoal> {
        self.goals.read().values().cloned().collect()
    }

    /// Get a specific goal.
    pub fn get_goal(&self, goal_id: &str) -> Option<OrchestratedGoal> {
        self.goals.read().get(goal_id).cloned()
    }

    /// Get the current queue depth.
    pub fn queue_depth(&self) -> usize {
        self.queue.read().len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_goal(desc: &str) -> Goal {
        Goal::new(desc)
    }

    // -----------------------------------------------------------------------
    // GoalPriority
    // -----------------------------------------------------------------------

    #[test]
    fn test_priority_as_u8() {
        assert_eq!(GoalPriority::Critical.as_u8(), 0);
        assert_eq!(GoalPriority::High.as_u8(), 1);
        assert_eq!(GoalPriority::Normal.as_u8(), 2);
        assert_eq!(GoalPriority::Low.as_u8(), 3);
        assert_eq!(GoalPriority::Background.as_u8(), 4);
    }

    #[test]
    fn test_priority_adjust_baseline() {
        let adj = GoalPriority::Normal.adjust(0, 0, None);
        assert_eq!(adj.score, 2000);
    }

    #[test]
    fn test_priority_adjust_age_increases_priority() {
        let fresh = GoalPriority::Normal.adjust(0, 0, None);
        let aged = GoalPriority::Normal.adjust(3_600_000 * 2, 0, None); // 2 hours
        assert!(
            aged.score < fresh.score,
            "aged goal should have higher priority"
        );
    }

    #[test]
    fn test_priority_adjust_retries_increase_priority() {
        let no_retry = GoalPriority::Normal.adjust(0, 0, None);
        let retried = GoalPriority::Normal.adjust(0, 3, None);
        assert!(retried.score < no_retry.score);
    }

    #[test]
    fn test_priority_adjust_deadline_soon_priority_boost() {
        let far =
            GoalPriority::Normal.adjust(0, 0, Some(Utc::now().timestamp_millis() + 3_600_000 * 24));
        let soon = GoalPriority::Normal.adjust(0, 0, Some(Utc::now().timestamp_millis() + 60_000));
        assert!(soon.score < far.score);
    }

    #[test]
    fn test_priority_adjust_deadline_critical_boost() {
        let adj = GoalPriority::Low.adjust(0, 0, Some(Utc::now().timestamp_millis() + 10_000));
        assert_eq!(adj.score, -1000);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(GoalPriority::Critical < GoalPriority::High);
        assert!(GoalPriority::High < GoalPriority::Normal);
        assert!(GoalPriority::Normal < GoalPriority::Low);
        assert!(GoalPriority::Low < GoalPriority::Background);
    }

    // -----------------------------------------------------------------------
    // OrchestratedGoalState
    // -----------------------------------------------------------------------

    #[test]
    fn test_goal_state_terminal() {
        assert!(OrchestratedGoalState::Completed.is_terminal());
        assert!(OrchestratedGoalState::Failed("err".into()).is_terminal());
        assert!(OrchestratedGoalState::Cancelled.is_terminal());
        assert!(!OrchestratedGoalState::Running.is_terminal());
    }

    #[test]
    fn test_goal_state_active() {
        assert!(OrchestratedGoalState::Running.is_active());
        assert!(OrchestratedGoalState::Paused.is_active());
        assert!(!OrchestratedGoalState::Queued.is_active());
    }

    #[test]
    fn test_goal_state_queued() {
        assert!(OrchestratedGoalState::Queued.is_queued());
        assert!(OrchestratedGoalState::Delayed { until: 0 }.is_queued());
        assert!(OrchestratedGoalState::WaitingForDependencies.is_queued());
        assert!(!OrchestratedGoalState::Running.is_queued());
    }

    // -----------------------------------------------------------------------
    // OrchestratedGoal
    // -----------------------------------------------------------------------

    #[test]
    fn test_orchestrated_goal_creation() {
        let g = OrchestratedGoal::new("g1", make_goal("test"), GoalPriority::Normal);
        assert_eq!(g.id, "g1");
        assert_eq!(g.goal.description, "test");
        assert_eq!(g.priority, GoalPriority::Normal);
        assert_eq!(g.state, OrchestratedGoalState::Queued);
        assert!(g.created_at > 0);
    }

    #[test]
    fn test_orchestrated_goal_builder() {
        let g = OrchestratedGoal::new("g1", make_goal("test"), GoalPriority::High)
            .with_dependency("dep1")
            .with_deadline(9999)
            .with_resource(Resource::Screen)
            .with_exclusive()
            .with_tag("env", "prod");
        assert_eq!(g.dependencies, vec!["dep1"]);
        assert_eq!(g.deadline, Some(9999));
        assert_eq!(g.required_resources, vec![Resource::Screen]);
        assert!(g.exclusive);
        assert_eq!(g.tags.get("env").unwrap(), "prod");
    }

    // -----------------------------------------------------------------------
    // ResourceLock
    // -----------------------------------------------------------------------

    #[test]
    fn test_resource_lock_acquire_release() {
        let lock = ResourceLock::new();
        assert!(lock.acquire(&[Resource::Screen]).is_ok());
        assert!(lock.is_held(&Resource::Screen));
        assert!(!lock.is_held(&Resource::Keyboard));
        lock.release(&[Resource::Screen]);
        assert!(!lock.is_held(&Resource::Screen));
    }

    #[test]
    fn test_resource_lock_conflict() {
        let lock = ResourceLock::new();
        lock.acquire(&[Resource::Screen]).unwrap();
        assert!(lock.acquire(&[Resource::Screen]).is_err());
        assert!(lock.acquire(&[Resource::Keyboard]).is_ok());
    }

    #[test]
    fn test_resource_lock_has_conflict() {
        let lock = ResourceLock::new();
        lock.acquire(&[Resource::Mouse]).unwrap();
        assert!(lock.has_conflict(&[Resource::Mouse]));
        assert!(!lock.has_conflict(&[Resource::Keyboard]));
    }

    #[test]
    fn test_resource_lock_held_resources() {
        let lock = ResourceLock::new();
        lock.acquire(&[Resource::Screen, Resource::Audio]).unwrap();
        let held = lock.held_resources();
        assert_eq!(held.len(), 2);
        assert!(held.contains(&Resource::Screen));
        assert!(held.contains(&Resource::Audio));
    }

    #[test]
    fn test_resource_lock_release_multiple() {
        let lock = ResourceLock::new();
        lock.acquire(&[Resource::Screen, Resource::Keyboard, Resource::Mouse])
            .unwrap();
        lock.release(&[Resource::Screen, Resource::Mouse]);
        assert!(!lock.is_held(&Resource::Screen));
        assert!(lock.is_held(&Resource::Keyboard));
        assert!(!lock.is_held(&Resource::Mouse));
    }

    #[test]
    fn test_resource_lock_multiple_acquire_same_denied() {
        let lock = ResourceLock::new();
        assert!(lock.acquire(&[Resource::Clipboard]).is_ok());
        let result = lock.acquire(&[Resource::Clipboard, Resource::Audio]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // GoalQueue
    // -----------------------------------------------------------------------

    #[test]
    fn test_queue_enqueue_dequeue_fifo() {
        let mut q = GoalQueue::new();
        q.enqueue("a", GoalPriority::Normal, 100, 0);
        q.enqueue("b", GoalPriority::Normal, 200, 0);
        assert_eq!(q.dequeue(), Some("a".to_string()));
        assert_eq!(q.dequeue(), Some("b".to_string()));
        assert!(q.dequeue().is_none());
    }

    #[test]
    fn test_queue_priority_ordering() {
        let mut q = GoalQueue::new();
        q.enqueue("low", GoalPriority::Low, 300, 0);
        q.enqueue("high", GoalPriority::High, 100, 0);
        q.enqueue("critical", GoalPriority::Critical, 200, 0);
        assert_eq!(q.dequeue(), Some("critical".to_string()));
        assert_eq!(q.dequeue(), Some("high".to_string()));
        assert_eq!(q.dequeue(), Some("low".to_string()));
    }

    #[test]
    fn test_queue_fifo_within_same_priority() {
        let mut q = GoalQueue::new();
        q.enqueue("first", GoalPriority::Normal, 100, 0);
        q.enqueue("second", GoalPriority::Normal, 200, 0);
        q.enqueue("third", GoalPriority::Normal, 300, 0);
        assert_eq!(q.dequeue(), Some("first".to_string()));
        assert_eq!(q.dequeue(), Some("second".to_string()));
        assert_eq!(q.dequeue(), Some("third".to_string()));
    }

    #[test]
    fn test_queue_delayed_not_ready() {
        let mut q = GoalQueue::new();
        q.enqueue("delayed", GoalPriority::Normal, 100, 60_000);
        assert!(q.dequeue().is_none());
        assert_eq!(q.delayed_ids().len(), 1);
    }

    #[test]
    fn test_queue_pop_due_delayed() {
        let mut q = GoalQueue::new();
        q.enqueue("now", GoalPriority::Normal, 100, 0);
        q.enqueue("later", GoalPriority::Normal, 200, 60_000);
        let due = q.pop_due_delayed();
        assert!(due.is_empty());
        // Enqueue with zero delay should not be in delayed
        assert_eq!(q.delayed_ids().len(), 1);
        assert_eq!(q.all_ids().len(), 1);
    }

    #[test]
    fn test_queue_remove() {
        let mut q = GoalQueue::new();
        q.enqueue("a", GoalPriority::Normal, 100, 0);
        q.enqueue("b", GoalPriority::Normal, 200, 0);
        q.remove("a");
        assert_eq!(q.dequeue(), Some("b".to_string()));
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn test_queue_peek() {
        let mut q = GoalQueue::new();
        assert!(q.peek().is_none());
        q.enqueue("a", GoalPriority::Low, 100, 0);
        q.enqueue("b", GoalPriority::High, 200, 0);
        assert_eq!(q.peek(), Some("b".to_string()));
        assert_eq!(q.len(), 2); // peek doesn't remove
    }

    #[test]
    fn test_queue_empty() {
        let mut q = GoalQueue::new();
        assert!(q.is_empty());
        q.enqueue("a", GoalPriority::Normal, 100, 0);
        assert!(!q.is_empty());
    }

    #[test]
    fn test_queue_all_ids() {
        let mut q = GoalQueue::new();
        q.enqueue("x", GoalPriority::High, 100, 0);
        q.enqueue("y", GoalPriority::Low, 200, 0);
        let ids = q.all_ids();
        // High priority first
        assert_eq!(ids[0], "x");
        assert_eq!(ids[1], "y");
    }

    // -----------------------------------------------------------------------
    // GoalOrchestrator — Submission & Status
    // -----------------------------------------------------------------------

    #[test]
    fn test_submit_goal() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("test"), GoalPriority::Normal, 0)
            .unwrap();
        assert!(!id.is_empty());
        assert_eq!(orch.queue_depth(), 1);
    }

    #[test]
    fn test_submit_goal_max_limit() {
        let config = GoalOrchestratorConfig {
            max_queued_goals: 2,
            ..Default::default()
        };
        let orch = GoalOrchestrator::new(config);
        orch.submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        orch.submit_goal(make_goal("g2"), GoalPriority::Normal, 0)
            .unwrap();
        let result = orch.submit_goal(make_goal("g3"), GoalPriority::Normal, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_submit_goal_with_delay() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("delayed"), GoalPriority::Normal, 60_000)
            .unwrap();
        let status = orch.goal_status(&id).unwrap();
        assert_eq!(status, OrchestratedGoalState::Queued);
    }

    #[test]
    fn test_goal_status_not_found() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        assert!(orch.goal_status("nonexistent").is_none());
    }

    #[test]
    fn test_cancel_goal() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("cancel-me"), GoalPriority::Normal, 0)
            .unwrap();
        assert!(orch.cancel_goal(&id).is_ok());
        assert_eq!(
            orch.goal_status(&id),
            Some(OrchestratedGoalState::Cancelled)
        );
    }

    #[test]
    fn test_cancel_goal_nonexistent() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        assert!(orch.cancel_goal("idontexist").is_err());
    }

    #[test]
    fn test_cancel_goal_twice_fails() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        orch.cancel_goal(&id).unwrap();
        assert!(orch.cancel_goal(&id).is_err());
    }

    #[test]
    fn test_goal_status_after_submit() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        assert_eq!(orch.goal_status(&id), Some(OrchestratedGoalState::Queued));
    }

    #[test]
    fn test_priorities_api() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Critical, 0)
            .unwrap();
        assert_eq!(orch.priority(&id), Some(GoalPriority::Critical));
    }

    #[test]
    fn test_priority_nonexistent() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        assert!(orch.priority("nope").is_none());
    }

    // -----------------------------------------------------------------------
    // Dependencies & DAG
    // -----------------------------------------------------------------------

    #[test]
    fn test_submit_goal_with_dependencies() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let dep_id = orch
            .submit_goal(make_goal("dependency"), GoalPriority::Normal, 0)
            .unwrap();
        let id = orch
            .submit_goal_with_deps(
                make_goal("dependent"),
                GoalPriority::Normal,
                vec![dep_id.clone()],
                0,
            )
            .unwrap();
        let deps = orch.dependencies(&id).unwrap();
        assert_eq!(deps, vec![dep_id]);
        assert_eq!(
            orch.goal_status(&id),
            Some(OrchestratedGoalState::WaitingForDependencies)
        );
    }

    #[test]
    fn test_submit_goal_empty_dependencies_is_queued() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal_with_deps(make_goal("independent"), GoalPriority::Normal, vec![], 0)
            .unwrap();
        assert_eq!(orch.goal_status(&id), Some(OrchestratedGoalState::Queued));
    }

    #[test]
    fn test_submit_goal_unknown_dependency_rejected() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let result = orch.submit_goal_with_deps(
            make_goal("g1"),
            GoalPriority::Normal,
            vec!["nonexistent".into()],
            0,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_submit_goal_self_dependency_rejected() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        // Self-dependency is caught by validate_dependencies
        let result = orch.submit_goal_with_deps(
            make_goal("self-dep"),
            GoalPriority::Normal,
            vec!["self".into()],
            0,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_cycle_direct() {
        let config = GoalOrchestratorConfig::default();
        let _orch = GoalOrchestrator::new(config);
        let _a = _orch
            .submit_goal(make_goal("A"), GoalPriority::Normal, 0)
            .unwrap();
        let b = _orch
            .submit_goal(make_goal("B"), GoalPriority::Normal, 0)
            .unwrap();

        // Chain (A → B → AB) is OK — no cycle
        assert!(_orch
            .submit_goal_with_deps(make_goal("AB"), GoalPriority::Normal, vec![b.clone()], 0,)
            .is_ok());

        // Build a direct cycle A ↔ B in a standalone graph and verify detection
        let mut graph: HashMap<String, OrchestratedGoal> = HashMap::new();
        let g_a =
            OrchestratedGoal::new("a", make_goal("A"), GoalPriority::Normal).with_dependency("b");
        let g_b =
            OrchestratedGoal::new("b", make_goal("B"), GoalPriority::Normal).with_dependency("a");
        graph.insert("a".to_string(), g_a);
        graph.insert("b".to_string(), g_b);

        assert!(GoalOrchestrator::detect_cycle_for("a", &graph));
        assert!(GoalOrchestrator::detect_cycle_for("b", &graph));
    }

    #[test]
    fn test_detect_cycle_indirect() {
        // Build a graph with an indirect cycle A → B → C → A
        let mut graph: HashMap<String, OrchestratedGoal> = HashMap::new();
        let g_a =
            OrchestratedGoal::new("a", make_goal("A"), GoalPriority::Normal).with_dependency("b");
        let g_b =
            OrchestratedGoal::new("b", make_goal("B"), GoalPriority::Normal).with_dependency("c");
        let g_c =
            OrchestratedGoal::new("c", make_goal("C"), GoalPriority::Normal).with_dependency("a");
        graph.insert("a".to_string(), g_a);
        graph.insert("b".to_string(), g_b);
        graph.insert("c".to_string(), g_c);

        assert!(GoalOrchestrator::detect_cycle_for("a", &graph));
        assert!(GoalOrchestrator::detect_cycle_for("b", &graph));
        assert!(GoalOrchestrator::detect_cycle_for("c", &graph));
    }

    #[test]
    fn test_dependencies_list() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let dep = orch
            .submit_goal(make_goal("dep"), GoalPriority::Normal, 0)
            .unwrap();
        let id = orch
            .submit_goal_with_deps(make_goal("main"), GoalPriority::High, vec![dep.clone()], 0)
            .unwrap();
        let deps = orch.dependencies(&id).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0], dep);
    }

    #[test]
    fn test_dependencies_nonexistent() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        assert!(orch.dependencies("nope").is_none());
    }

    // -----------------------------------------------------------------------
    // Concurrent Execution Limits
    // -----------------------------------------------------------------------

    #[test]
    fn test_max_concurrent_goals() {
        let config = GoalOrchestratorConfig {
            max_concurrent_goals: 2,
            ..Default::default()
        };
        let orch = GoalOrchestrator::new(config);
        let _g1 = orch
            .submit_goal(make_goal("g1"), GoalPriority::Critical, 0)
            .unwrap();
        let _g2 = orch
            .submit_goal(make_goal("g2"), GoalPriority::High, 0)
            .unwrap();
        let _g3 = orch
            .submit_goal(make_goal("g3"), GoalPriority::Normal, 0)
            .unwrap();

        // Tick should start up to 2 goals
        let changed = orch.tick();
        assert!(changed.len() <= 2);

        let running = orch.running_goals();
        assert!(running.len() <= 2);
    }

    #[test]
    fn test_background_goal_limit() {
        let config = GoalOrchestratorConfig {
            max_background_goals: 1,
            max_concurrent_goals: 5,
            ..Default::default()
        };
        let orch = GoalOrchestrator::new(config);
        orch.submit_goal(make_goal("bg1"), GoalPriority::Background, 0)
            .unwrap();
        orch.submit_goal(make_goal("bg2"), GoalPriority::Background, 0)
            .unwrap();
        orch.submit_goal(make_goal("bg3"), GoalPriority::Background, 0)
            .unwrap();

        orch.tick();

        let running = orch.running_goals();
        assert!(running.len() <= 1);
    }

    // -----------------------------------------------------------------------
    // Exclusive Goals
    // -----------------------------------------------------------------------

    #[test]
    fn test_exclusive_goal_limits_concurrent() {
        let config = GoalOrchestratorConfig {
            max_concurrent_goals: 5,
            ..Default::default()
        };
        let orch = GoalOrchestrator::new(config);

        // Submit an exclusive goal
        let excl = {
            let id = orch
                .submit_goal(make_goal("shutdown"), GoalPriority::Critical, 0)
                .unwrap();
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.exclusive = true;
            }
            id
        };

        orch.submit_goal(make_goal("other"), GoalPriority::Normal, 0)
            .unwrap();

        orch.tick();

        // Exclusive should start, other should still be queued
        let running = orch.running_goals();
        let running_ids: Vec<String> = running.iter().map(|g| g.id.clone()).collect();
        assert!(running_ids.contains(&excl));
    }

    // -----------------------------------------------------------------------
    // Queue Ordering
    // -----------------------------------------------------------------------

    #[test]
    fn test_queue_ordering_mixed_priorities() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let low = orch
            .submit_goal(make_goal("low"), GoalPriority::Low, 0)
            .unwrap();
        let crit = orch
            .submit_goal(make_goal("crit"), GoalPriority::Critical, 0)
            .unwrap();
        let high = orch
            .submit_goal(make_goal("high"), GoalPriority::High, 0)
            .unwrap();

        let q_ids = orch.queue.read().all_ids();
        // Critical first, then High, then Low
        assert_eq!(q_ids[0], crit);
        assert_eq!(q_ids[1], high);
        assert_eq!(q_ids[2], low);
    }

    // -----------------------------------------------------------------------
    // Metrics
    // -----------------------------------------------------------------------

    #[test]
    fn test_metrics_initial() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let m = orch.metrics();
        assert_eq!(m.total_submitted, 0);
        assert_eq!(m.total_completed, 0);
        assert_eq!(m.total_failed, 0);
        assert_eq!(m.total_cancelled, 0);
    }

    #[test]
    fn test_metrics_after_submit() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        orch.submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        let m = orch.metrics();
        assert_eq!(m.total_submitted, 1);
    }

    #[test]
    fn test_metrics_after_cancel() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        orch.cancel_goal(&id).unwrap();
        let m = orch.metrics();
        assert_eq!(m.total_cancelled, 1);
    }

    #[test]
    fn test_metrics_after_complete() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        // Manually set to running then complete
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.state = OrchestratedGoalState::Running;
                g.started_at = Some(Utc::now().timestamp_millis());
            }
        }
        orch.running_ids.write().insert(id.clone());
        orch.complete_goal(&id, true, None).unwrap();
        let m = orch.metrics();
        assert_eq!(m.total_completed, 1);
    }

    #[test]
    fn test_metrics_after_failure() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.state = OrchestratedGoalState::Running;
                g.started_at = Some(Utc::now().timestamp_millis());
            }
        }
        orch.running_ids.write().insert(id.clone());
        orch.complete_goal(&id, false, Some("error".into()))
            .unwrap();
        let m = orch.metrics();
        assert_eq!(m.total_failed, 1);
    }

    #[test]
    fn test_metrics_retry_and_recovery() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        orch.record_retry(&id).unwrap();
        orch.record_recovery(&id).unwrap();
        let m = orch.metrics();
        assert_eq!(m.total_retries, 1);
        assert_eq!(m.total_recoveries, 1);
    }

    #[test]
    fn test_metrics_peak_concurrent() {
        let config = GoalOrchestratorConfig {
            max_concurrent_goals: 10,
            ..Default::default()
        };
        let orch = GoalOrchestrator::new(config);
        orch.submit_goal(make_goal("g1"), GoalPriority::Critical, 0)
            .unwrap();
        orch.submit_goal(make_goal("g2"), GoalPriority::High, 0)
            .unwrap();

        // Tick to start goals (this calls start_goal_inner which updates peak_concurrent)
        let changed = orch.tick();
        assert_eq!(changed.len(), 2);

        let m = orch.metrics();
        assert!(m.peak_concurrent >= 2);
    }

    // -----------------------------------------------------------------------
    // queued_goals / running_goals / completed_goals / failed_goals
    // -----------------------------------------------------------------------

    #[test]
    fn test_queued_goals_filter() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        orch.submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        let queued = orch.queued_goals();
        assert_eq!(queued.len(), 1);
    }

    #[test]
    fn test_running_goals_filter() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.state = OrchestratedGoalState::Running;
            }
        }
        let running = orch.running_goals();
        assert_eq!(running.len(), 1);
    }

    #[test]
    fn test_completed_goals_filter() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.state = OrchestratedGoalState::Completed;
            }
        }
        let completed = orch.completed_goals();
        assert_eq!(completed.len(), 1);
    }

    #[test]
    fn test_failed_goals_filter() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.state = OrchestratedGoalState::Failed("err".into());
            }
        }
        let failed = orch.failed_goals();
        assert_eq!(failed.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Pause / Resume
    // -----------------------------------------------------------------------

    #[test]
    fn test_pause_goal() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.state = OrchestratedGoalState::Running;
            }
        }
        assert!(orch.pause_goal(&id).is_ok());
        assert_eq!(orch.goal_status(&id), Some(OrchestratedGoalState::Paused));
    }

    #[test]
    fn test_pause_not_running_fails() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        assert!(orch.pause_goal(&id).is_err());
    }

    #[test]
    fn test_pause_nonexistent() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        assert!(orch.pause_goal("nope").is_err());
    }

    #[test]
    fn test_resume_goal() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.state = OrchestratedGoalState::Paused;
            }
        }
        assert!(orch.resume_goal(&id).is_ok());
        assert_eq!(orch.goal_status(&id), Some(OrchestratedGoalState::Queued));
    }

    #[test]
    fn test_resume_not_paused_fails() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        assert!(orch.resume_goal(&id).is_err());
    }

    #[test]
    fn test_pause_resume_multiple_cycles() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        for _ in 0..3 {
            {
                let mut goals = orch.goals.write();
                if let Some(g) = goals.get_mut(&id) {
                    g.state = OrchestratedGoalState::Running;
                }
            }
            orch.pause_goal(&id).unwrap();
            assert_eq!(orch.goal_status(&id), Some(OrchestratedGoalState::Paused));
            orch.resume_goal(&id).unwrap();
            assert_eq!(orch.goal_status(&id), Some(OrchestratedGoalState::Queued));
        }
    }

    // -----------------------------------------------------------------------
    // Starvation prevention via aging
    // -----------------------------------------------------------------------

    #[test]
    fn test_starvation_reprioritization() {
        let config = GoalOrchestratorConfig {
            starvation_check_interval_ms: 1, // Immediate aging
            enable_dynamic_priority: true,
            ..Default::default()
        };
        let orch = GoalOrchestrator::new(config);
        let old_id = orch
            .submit_goal(make_goal("old"), GoalPriority::Low, 0)
            .unwrap();

        // Simulate age by creating goal with past timestamp
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&old_id) {
                g.created_at = Utc::now().timestamp_millis() - 3_600_000; // 1 hour ago
            }
        }

        // Re-prioritize
        orch.reprioritize_aging_goals();

        // The aged low-priority goal should still be in the queue
        let q = orch.queue.read();
        let ids = q.all_ids();
        assert!(ids.contains(&old_id));
    }

    // -----------------------------------------------------------------------
    // Complete goal lifecycle
    // -----------------------------------------------------------------------

    #[test]
    fn test_complete_goal_success() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.state = OrchestratedGoalState::Running;
                g.started_at = Some(Utc::now().timestamp_millis());
            }
        }
        orch.running_ids.write().insert(id.clone());
        assert!(orch.complete_goal(&id, true, None).is_ok());
        assert_eq!(
            orch.goal_status(&id),
            Some(OrchestratedGoalState::Completed)
        );
    }

    #[test]
    fn test_complete_goal_failure() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.state = OrchestratedGoalState::Running;
                g.started_at = Some(Utc::now().timestamp_millis());
            }
        }
        orch.running_ids.write().insert(id.clone());
        assert!(orch.complete_goal(&id, false, Some("oops".into())).is_ok());
        match orch.goal_status(&id) {
            Some(OrchestratedGoalState::Failed(reason)) => assert_eq!(reason, "oops"),
            _ => panic!("expected Failed state"),
        }
    }

    #[test]
    fn test_complete_goal_not_running_fails() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        assert!(orch.complete_goal(&id, true, None).is_err());
    }

    #[test]
    fn test_complete_goal_nonexistent() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        assert!(orch.complete_goal("nope", true, None).is_err());
    }

    #[test]
    fn test_record_retry() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        orch.record_retry(&id).unwrap();
        let goal = orch.get_goal(&id).unwrap();
        assert_eq!(goal.retry_count, 1);
    }

    #[test]
    fn test_record_recovery() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        orch.record_recovery(&id).unwrap();
        let goal = orch.get_goal(&id).unwrap();
        assert_eq!(goal.recovery_count, 1);
    }

    // -----------------------------------------------------------------------
    // Goal retrieval
    // -----------------------------------------------------------------------

    #[test]
    fn test_all_goals() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        orch.submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        orch.submit_goal(make_goal("g2"), GoalPriority::High, 0)
            .unwrap();
        assert_eq!(orch.all_goals().len(), 2);
    }

    #[test]
    fn test_get_goal_by_id() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("find-me"), GoalPriority::Normal, 0)
            .unwrap();
        let goal = orch.get_goal(&id).unwrap();
        assert_eq!(goal.goal.description, "find-me");
    }

    #[test]
    fn test_get_goal_nonexistent() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        assert!(orch.get_goal("nope").is_none());
    }

    // -----------------------------------------------------------------------
    // Resource locking integration
    // -----------------------------------------------------------------------

    #[test]
    fn test_resource_lock_in_goal() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.required_resources.push(Resource::Screen);
            }
        }
        // Try to start the goal — should acquire the resource
        let result = orch.start_goal_inner(&id);
        assert!(result.is_ok());
        assert!(orch.resource_lock.is_held(&Resource::Screen));

        // Cancel releases resources
        if let Some(g) = orch.goals.write().get_mut(&id) {
            g.state = OrchestratedGoalState::Running;
        }
        orch.cancel_goal(&id).unwrap();
        assert!(!orch.resource_lock.is_held(&Resource::Screen));
    }

    // -----------------------------------------------------------------------
    // Config
    // -----------------------------------------------------------------------

    #[test]
    fn test_orchestrator_config_default() {
        let config = GoalOrchestratorConfig::default();
        assert_eq!(config.max_concurrent_goals, 5);
        assert_eq!(config.max_background_goals, 2);
        assert_eq!(config.max_queued_goals, 100);
        assert!(config.enable_dynamic_priority);
    }

    #[test]
    fn test_orchestrator_with_config() {
        let config = GoalOrchestratorConfig {
            max_concurrent_goals: 3,
            max_background_goals: 1,
            max_queued_goals: 50,
            ..Default::default()
        };
        let orch = GoalOrchestrator::new(config.clone());
        let retrieved = orch.config();
        assert_eq!(retrieved.max_concurrent_goals, 3);
        assert_eq!(retrieved.max_background_goals, 1);
        assert_eq!(retrieved.max_queued_goals, 50);
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_queue_empty_initial() {
        let mut q = GoalQueue::new();
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
        assert!(q.dequeue().is_none());
        assert!(q.peek().is_none());
    }

    #[test]
    fn test_submit_goal_empty_description() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let result = orch.submit_goal(make_goal(""), GoalPriority::Normal, 0);
        assert!(result.is_ok()); // Empty description is allowed by Goal
    }

    #[test]
    fn test_goal_with_exclusive_flag_not_submitted_with_exclusive() {
        // Test that exclusive goal created via submit_goal can later be marked exclusive
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Critical, 0)
            .unwrap();
        // Mark exclusive
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.exclusive = true;
            }
        }
        let goal = orch.get_goal(&id).unwrap();
        assert!(goal.exclusive);
    }

    #[test]
    fn test_submit_critical_background_ordering() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let bg = orch
            .submit_goal(make_goal("bg"), GoalPriority::Background, 0)
            .unwrap();
        let crit = orch
            .submit_goal(make_goal("crit"), GoalPriority::Critical, 0)
            .unwrap();

        let q_ids = orch.queue.read().all_ids();
        assert_eq!(q_ids[0], crit);
        assert_eq!(q_ids[1], bg);
    }

    #[test]
    fn test_multiple_goals_same_priority_fifo() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let a = orch
            .submit_goal(make_goal("A"), GoalPriority::Normal, 0)
            .unwrap();
        let b = orch
            .submit_goal(make_goal("B"), GoalPriority::Normal, 100)
            .unwrap();
        let c = orch
            .submit_goal(make_goal("C"), GoalPriority::Normal, 200)
            .unwrap();

        // Manually override timestamps for ordering test
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&a) {
                g.created_at = 100;
            }
            if let Some(g) = goals.get_mut(&b) {
                g.created_at = 200;
            }
            if let Some(g) = goals.get_mut(&c) {
                g.created_at = 300;
            }
        }

        // Re-enqueue to get proper ordering
        {
            let mut q = orch.queue.write();
            q.remove(&a);
            q.remove(&b);
            q.remove(&c);
            q.enqueue(&a, GoalPriority::Normal, 100, 0);
            q.enqueue(&b, GoalPriority::Normal, 200, 0);
            q.enqueue(&c, GoalPriority::Normal, 300, 0);
        }

        let q_ids = orch.queue.read().all_ids();
        assert_eq!(q_ids[0], a);
        assert_eq!(q_ids[1], b);
        assert_eq!(q_ids[2], c);
    }

    #[test]
    fn test_tick_with_no_goals() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let changed = orch.tick();
        assert!(changed.is_empty());
    }

    #[test]
    fn test_complete_goal_releases_resources() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let id = orch
            .submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&id) {
                g.required_resources.push(Resource::Keyboard);
                g.state = OrchestratedGoalState::Running;
                g.started_at = Some(Utc::now().timestamp_millis());
            }
        }
        orch.resource_lock.acquire(&[Resource::Keyboard]).unwrap();
        orch.running_ids.write().insert(id.clone());
        orch.complete_goal(&id, true, None).unwrap();
        assert!(!orch.resource_lock.is_held(&Resource::Keyboard));
    }

    #[test]
    fn test_dependency_chain_completes_goal() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let dep = orch
            .submit_goal(make_goal("dependency"), GoalPriority::Normal, 0)
            .unwrap();
        let main = orch
            .submit_goal_with_deps(
                make_goal("main"),
                GoalPriority::Normal,
                vec![dep.clone()],
                0,
            )
            .unwrap();

        assert_eq!(
            orch.goal_status(&main),
            Some(OrchestratedGoalState::WaitingForDependencies)
        );

        // Complete the dependency
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&dep) {
                g.state = OrchestratedGoalState::Completed;
                g.completed_at = Some(Utc::now().timestamp_millis());
            }
        }

        // Tick should resolve dependencies
        orch.tick();

        // Main should now be queued (or running if immediately dequeued)
        let status = orch.goal_status(&main).unwrap();
        assert!(
            status == OrchestratedGoalState::Queued || status == OrchestratedGoalState::Running
        );
    }

    #[test]
    fn test_dependency_not_completed_still_waiting() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        let dep = orch
            .submit_goal(make_goal("dependency"), GoalPriority::Normal, 0)
            .unwrap();
        let main = orch
            .submit_goal_with_deps(
                make_goal("main"),
                GoalPriority::Normal,
                vec![dep.clone()],
                0,
            )
            .unwrap();

        // Mark dependency as failed
        {
            let mut goals = orch.goals.write();
            if let Some(g) = goals.get_mut(&dep) {
                g.state = OrchestratedGoalState::Failed("err".into());
            }
        }

        orch.tick();

        // Main should still be waiting because dep failed (not completed)
        assert_eq!(
            orch.goal_status(&main),
            Some(OrchestratedGoalState::WaitingForDependencies)
        );
    }

    #[test]
    fn test_record_retry_nonexistent() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        assert!(orch.record_retry("nope").is_err());
    }

    #[test]
    fn test_record_recovery_nonexistent() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        assert!(orch.record_recovery("nope").is_err());
    }

    #[test]
    fn test_queue_depth() {
        let config = GoalOrchestratorConfig::default();
        let orch = GoalOrchestrator::new(config);
        assert_eq!(orch.queue_depth(), 0);
        orch.submit_goal(make_goal("g1"), GoalPriority::Normal, 0)
            .unwrap();
        assert_eq!(orch.queue_depth(), 1);
        orch.submit_goal(make_goal("g2"), GoalPriority::Normal, 0)
            .unwrap();
        assert_eq!(orch.queue_depth(), 2);
    }
}
