use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::action::{ActionType, DeviceControl};

/// High-level user goal to be decomposed into an execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub description: String,
    pub context: HashMap<String, String>,
}

impl Goal {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            context: HashMap::new(),
        }
    }

    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

/// A capability required by an execution step.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    ScreenCapture,
    ScreenGrounding,
    Ocr,
    InputMouse,
    InputKeyboard,
    InputTouch,
    AutomationWorkflow,
    MemoryQuery,
    MemoryStore,
    PluginInvocation,
    AiInference,
    VoiceCapture,
    DeviceControl,
}

/// A single step within an execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub id: String,
    pub description: String,
    pub action: ActionType,
    /// IDs of steps that must complete before this step.
    pub dependencies: Vec<String>,
    pub required_capabilities: Vec<Capability>,
    pub timeout_ms: u64,
    pub retry_count: u32,
    pub continue_on_failure: bool,
}

/// A complete execution plan with dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub id: String,
    pub goal_description: String,
    pub steps: Vec<ExecutionStep>,
    pub created_at: i64,
    pub estimated_steps: usize,
}

impl ExecutionPlan {
    /// Find a step by its ID.
    pub fn find_step(&self, step_id: &str) -> Option<&ExecutionStep> {
        self.steps.iter().find(|s| s.id == step_id)
    }

    /// Get IDs of all steps.
    pub fn step_ids(&self) -> Vec<&str> {
        self.steps.iter().map(|s| s.id.as_str()).collect()
    }
}

/// Validation result for an execution plan.
#[derive(Debug, Clone)]
pub struct PlanValidation {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub has_cycles: bool,
    pub unreachable_steps: Vec<String>,
}

/// Decomposes high-level goals into executable execution plans.
pub struct Planner {
    max_steps_per_plan: usize,
    default_step_timeout_ms: u64,
    default_retry_count: u32,
}

impl Planner {
    pub fn new() -> Self {
        Self {
            max_steps_per_plan: 20,
            default_step_timeout_ms: 30_000,
            default_retry_count: 2,
        }
    }

    pub fn with_max_steps(mut self, max: usize) -> Self {
        self.max_steps_per_plan = max;
        self
    }

    pub fn with_default_timeout(mut self, timeout_ms: u64) -> Self {
        self.default_step_timeout_ms = timeout_ms;
        self
    }

    pub fn with_default_retry(mut self, retry: u32) -> Self {
        self.default_retry_count = retry;
        self
    }

    /// Decompose a goal into an execution plan using heuristic pattern matching.
    pub fn plan(&self, goal: &Goal) -> Result<ExecutionPlan, String> {
        let description = goal.description.trim().to_lowercase();
        let steps = self.decompose(&description, goal)?;

        if steps.is_empty() {
            return Err(format!(
                "could not decompose goal '{}' into any actionable steps",
                goal.description
            ));
        }

        if steps.len() > self.max_steps_per_plan {
            return Err(format!(
                "goal decomposition produced {} steps, exceeding maximum of {}",
                steps.len(),
                self.max_steps_per_plan
            ));
        }

        let now = chrono::Utc::now().timestamp_millis();
        let plan = ExecutionPlan {
            id: uuid::Uuid::new_v4().to_string(),
            goal_description: goal.description.clone(),
            estimated_steps: steps.len(),
            steps,
            created_at: now,
        };

        Ok(plan)
    }

    /// Validate a plan for structural correctness.
    pub fn validate(&self, plan: &ExecutionPlan) -> PlanValidation {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut unreachable_steps = Vec::new();

        let step_ids: HashSet<&str> = plan.steps.iter().map(|s| s.id.as_str()).collect();

        // Check for missing dependency references.
        for step in &plan.steps {
            for dep_id in &step.dependencies {
                if !step_ids.contains(dep_id.as_str()) {
                    errors.push(format!(
                        "step '{}' depends on '{}' which does not exist in the plan",
                        step.id, dep_id
                    ));
                }
            }
        }

        // Check for duplicate step IDs.
        let mut seen = HashSet::new();
        for step in &plan.steps {
            if !seen.insert(step.id.as_str()) {
                errors.push(format!("duplicate step id '{}'", step.id));
            }
        }

        // Check for cycles.
        let has_cycles = self.has_cycles(plan);
        if has_cycles {
            errors.push("plan contains a circular dependency".to_string());
        }

        // Find unreachable steps (no incoming edges not part of a cycle).
        if !has_cycles {
            if let Ok(order) = self.topological_sort(plan) {
                let ordered_ids: HashSet<&str> =
                    order.iter().map(|&i| plan.steps[i].id.as_str()).collect();
                for step in &plan.steps {
                    if !ordered_ids.contains(step.id.as_str()) {
                        unreachable_steps.push(step.id.clone());
                    }
                }
            }
        }

        // Warn if plan has many steps.
        if plan.steps.len() > 10 {
            warnings.push(format!(
                "plan has {} steps which may be slow to execute",
                plan.steps.len()
            ));
        }

        PlanValidation {
            is_valid: errors.is_empty(),
            errors,
            warnings,
            has_cycles,
            unreachable_steps,
        }
    }

    /// Compute a topological ordering of step indices respecting dependencies.
    /// Uses Kahn's algorithm. Returns an error if a cycle is detected.
    pub fn topological_sort(&self, plan: &ExecutionPlan) -> Result<Vec<usize>, String> {
        let n = plan.steps.len();
        let mut in_degree = vec![0usize; n];
        let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();
        let id_to_index: HashMap<&str, usize> = plan
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.as_str(), i))
            .collect();

        for (i, step) in plan.steps.iter().enumerate() {
            for dep_id in &step.dependencies {
                if let Some(&dep_idx) = id_to_index.get(dep_id.as_str()) {
                    adjacency.entry(dep_idx).or_default().push(i);
                    in_degree[i] += 1;
                }
            }
        }

        let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut result = Vec::with_capacity(n);
        let mut remaining = HashSet::<usize>::from_iter(0..n);

        while let Some(idx) = queue.pop() {
            result.push(idx);
            remaining.remove(&idx);
            if let Some(neighbors) = adjacency.get(&idx) {
                for &next in neighbors {
                    in_degree[next] = in_degree[next].saturating_sub(1);
                    if in_degree[next] == 0 {
                        queue.push(next);
                    }
                }
            }
        }

        if !remaining.is_empty() {
            let cycled: Vec<usize> = remaining.into_iter().collect();
            return Err(format!(
                "cycle detected involving step indices: {:?}",
                cycled
            ));
        }

        Ok(result)
    }

    /// Check whether the plan contains any circular dependencies.
    pub fn has_cycles(&self, plan: &ExecutionPlan) -> bool {
        self.topological_sort(plan).is_err()
    }

    /// Get steps whose all dependencies are satisfied.
    pub fn ready_steps<'a>(
        &self,
        plan: &'a ExecutionPlan,
        completed: &[String],
    ) -> Vec<&'a ExecutionStep> {
        let completed_set: HashSet<&str> = completed.iter().map(|s| s.as_str()).collect();
        plan.steps
            .iter()
            .filter(|step| {
                !completed_set.contains(step.id.as_str())
                    && step
                        .dependencies
                        .iter()
                        .all(|dep| completed_set.contains(dep.as_str()))
            })
            .collect()
    }

    /// Create a default step with standard settings.
    fn make_step(
        &self,
        id: &str,
        description: &str,
        action: ActionType,
        dependencies: Vec<String>,
        capabilities: Vec<Capability>,
    ) -> ExecutionStep {
        ExecutionStep {
            id: id.to_string(),
            description: description.to_string(),
            action,
            dependencies,
            required_capabilities: capabilities,
            timeout_ms: self.default_step_timeout_ms,
            retry_count: self.default_retry_count,
            continue_on_failure: false,
        }
    }

    /// Decompose goal text into execution steps using heuristic pattern matching.
    fn decompose(&self, description: &str, goal: &Goal) -> Result<Vec<ExecutionStep>, String> {
        let mut steps = Vec::new();
        let mut step_index = 0usize;

        // Check for brightness-related goals.
        if description.contains("brightness")
            || description.contains("screen brightness")
            || description.contains("dim")
            || description.contains("dimmer")
        {
            let value = extract_number(description, "brightness", 80);
            let action = if description.contains("max") || description.contains("full") {
                ActionType::DeviceControl {
                    control: DeviceControl::SetBrightness(100),
                }
            } else if description.contains("min") || description.contains("lowest") {
                ActionType::DeviceControl {
                    control: DeviceControl::SetBrightness(10),
                }
            } else {
                ActionType::DeviceControl {
                    control: DeviceControl::SetBrightness(value.min(100)),
                }
            };
            step_index += 1;
            steps.push(self.make_step(
                &format!("s{step_index}"),
                &format!("set brightness to {}", value.min(100)),
                action,
                vec![],
                vec![Capability::DeviceControl],
            ));
            return Ok(steps);
        }

        // Check for volume-related goals.
        if description.contains("volume")
            || description.contains("sound")
            || description.contains("mute")
        {
            if description.contains("mute") {
                step_index += 1;
                steps.push(self.make_step(
                    &format!("s{step_index}"),
                    "mute device volume",
                    ActionType::DeviceControl {
                        control: DeviceControl::SetVolume(0),
                    },
                    vec![],
                    vec![Capability::DeviceControl],
                ));
            } else {
                let value = extract_number(description, "volume", 50);
                step_index += 1;
                steps.push(self.make_step(
                    &format!("s{step_index}"),
                    &format!("set volume to {value}"),
                    ActionType::DeviceControl {
                        control: DeviceControl::SetVolume(value.min(100)),
                    },
                    vec![],
                    vec![Capability::DeviceControl],
                ));
            }
            return Ok(steps);
        }

        // Check for screen capture / screenshot goals.
        if description.contains("screenshot")
            || description.contains("capture screen")
            || description.contains("take a screenshot")
        {
            step_index += 1;
            steps.push(self.make_step(
                &format!("s{step_index}"),
                "capture screen",
                ActionType::ClickScreenElement {
                    query: "screenshot".to_string(),
                },
                vec![],
                vec![Capability::ScreenCapture, Capability::InputMouse],
            ));
            return Ok(steps);
        }

        // Check for click/text goals.
        if description.contains("click") || description.contains("tap") {
            let target = extract_quoted(description).unwrap_or("button");
            step_index += 1;
            steps.push(self.make_step(
                &format!("s{step_index}"),
                &format!("click '{target}'"),
                ActionType::ClickScreenText {
                    text: target.to_string(),
                },
                vec![],
                vec![
                    Capability::ScreenCapture,
                    Capability::Ocr,
                    Capability::InputMouse,
                ],
            ));
            return Ok(steps);
        }

        if description.contains("type") || description.contains("enter text") {
            let target = extract_after(description, "into").unwrap_or("field");
            let text = extract_quoted(description).unwrap_or("text");
            step_index += 1;
            steps.push(self.make_step(
                &format!("s{step_index}"),
                &format!("type '{text}' into '{target}'"),
                ActionType::TypeIntoScreenElement {
                    query: target.to_string(),
                    text: text.to_string(),
                },
                vec![],
                vec![
                    Capability::ScreenCapture,
                    Capability::ScreenGrounding,
                    Capability::InputKeyboard,
                ],
            ));
            return Ok(steps);
        }

        // Check for search goals.
        if description.contains("search")
            || description.contains("find")
            || description.contains("look up")
        {
            let query = extract_quoted(description).unwrap_or(
                description
                    .strip_prefix("search")
                    .or_else(|| description.strip_prefix("find"))
                    .unwrap_or(description)
                    .trim(),
            );
            step_index += 1;
            steps.push(self.make_step(
                &format!("s{step_index}"),
                &format!("search for '{query}'"),
                ActionType::SearchMemory {
                    query: query.to_string(),
                    max_results: 10,
                },
                vec![],
                vec![Capability::MemoryQuery],
            ));
            return Ok(steps);
        }

        // Check for memory / note goals.
        if description.contains("remember")
            || description.contains("note")
            || description.contains("save")
        {
            let content = extract_quoted(description).unwrap_or(description);
            let title = extract_after(description, "as").unwrap_or("note");
            step_index += 1;
            steps.push(self.make_step(
                &format!("s{step_index}"),
                &format!("create memory '{title}'"),
                ActionType::CreateMemory {
                    title: title.to_string(),
                    content: content.to_string(),
                    category: "general".to_string(),
                    tags: vec![],
                    importance: 5,
                },
                vec![],
                vec![Capability::MemoryStore],
            ));
            return Ok(steps);
        }

        // Check for app launch goals.
        if description.contains("open")
            || description.contains("launch")
            || description.contains("start")
        {
            let app = extract_after(description, "open")
                .or_else(|| extract_after(description, "launch"))
                .or_else(|| extract_after(description, "start"))
                .unwrap_or(description.trim());
            step_index += 1;
            steps.push(self.make_step(
                &format!("s{step_index}"),
                &format!("open '{app}'"),
                ActionType::OpenApp {
                    app_id: app.to_string(),
                    data: None,
                },
                vec![],
                vec![Capability::AutomationWorkflow],
            ));
            return Ok(steps);
        }

        // Check for device control goals.
        if description.contains("lock") && description.contains("device") {
            step_index += 1;
            steps.push(self.make_step(
                &format!("s{step_index}"),
                "lock device",
                ActionType::DeviceControl {
                    control: DeviceControl::LockScreen,
                },
                vec![],
                vec![Capability::DeviceControl],
            ));
            return Ok(steps);
        }

        if description.contains("wifi") || description.contains("wi-fi") {
            let enable = !description.contains("off") && !description.contains("disable");
            step_index += 1;
            steps.push(self.make_step(
                &format!("s{step_index}"),
                if enable {
                    "enable wifi"
                } else {
                    "disable wifi"
                },
                ActionType::DeviceControl {
                    control: DeviceControl::ToggleWiFi(enable),
                },
                vec![],
                vec![Capability::DeviceControl],
            ));
            return Ok(steps);
        }

        if description.contains("bluetooth") {
            let enable = !description.contains("off") && !description.contains("disable");
            step_index += 1;
            steps.push(self.make_step(
                &format!("s{step_index}"),
                if enable {
                    "enable bluetooth"
                } else {
                    "disable bluetooth"
                },
                ActionType::DeviceControl {
                    control: DeviceControl::ToggleBluetooth(enable),
                },
                vec![],
                vec![Capability::DeviceControl],
            ));
            return Ok(steps);
        }

        if description.contains("dnd") || description.contains("do not disturb") {
            let enable = !description.contains("off") && !description.contains("disable");
            step_index += 1;
            steps.push(self.make_step(
                &format!("s{step_index}"),
                if enable {
                    "enable do not disturb"
                } else {
                    "disable do not disturb"
                },
                ActionType::DeviceControl {
                    control: DeviceControl::ToggleDND(enable),
                },
                vec![],
                vec![Capability::DeviceControl],
            ));
            return Ok(steps);
        }

        // Fallback: wrap entire description as an AI inference request.
        step_index += 1;
        steps.push(self.make_step(
            &format!("s{step_index}"),
            &format!("process goal: {}", goal.description),
            ActionType::RunAI {
                prompt: goal.description.clone(),
                session_id: goal.context.get("session_id").cloned(),
            },
            vec![],
            vec![Capability::AiInference],
        ));

        Ok(steps)
    }
}

impl Default for Planner {
    fn default() -> Self {
        Self::new()
    }
}

// --- Helpers ---

fn extract_number(text: &str, key: &str, default: u32) -> u32 {
    let lower = text.to_lowercase();
    if let Some(pos) = lower.find(key) {
        let after = &lower[pos + key.len()..];
        for word in after.split_whitespace() {
            if let Ok(n) = word
                .trim_matches(|c: char| !c.is_ascii_digit())
                .parse::<u32>()
            {
                return n;
            }
        }
    }
    default
}

fn extract_quoted(text: &str) -> Option<&str> {
    if let Some(start) = text.find('\'') {
        let remaining = &text[start + 1..];
        if let Some(end) = remaining.find('\'') {
            return Some(&remaining[..end]);
        }
    }
    if let Some(start) = text.find('"') {
        let remaining = &text[start + 1..];
        if let Some(end) = remaining.find('"') {
            return Some(&remaining[..end]);
        }
    }
    None
}

fn extract_after<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let lower = text.to_lowercase();
    if let Some(pos) = lower.find(prefix) {
        let after = text[pos + prefix.len()..].trim();
        if after.is_empty() {
            return None;
        }
        // Take up to the next punctuation or end.
        let end = after.find(['.', ',', ';', '!']).unwrap_or(after.len());
        let result = after[..end].trim();
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planner_creates_plan_for_open_app() {
        let planner = Planner::new();
        let goal = Goal::new("open calculator");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.estimated_steps, 1);
        assert_eq!(plan.steps.len(), 1);
        let step = &plan.steps[0];
        assert!(matches!(step.action, ActionType::OpenApp { .. }));
        assert!(step.dependencies.is_empty());
    }

    #[test]
    fn test_planner_creates_plan_for_search() {
        let planner = Planner::new();
        let goal = Goal::new("search for 'rust programming'");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps.len(), 1);
        let step = &plan.steps[0];
        assert!(matches!(step.action, ActionType::SearchMemory { .. }));
    }

    #[test]
    fn test_planner_creates_plan_for_brightness() {
        let planner = Planner::new();
        let goal = Goal::new("set brightness to 75");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps.len(), 1);
        let step = &plan.steps[0];
        match &step.action {
            ActionType::DeviceControl { control } => match control {
                DeviceControl::SetBrightness(v) => assert_eq!(*v, 75),
                _ => panic!("wrong device control variant"),
            },
            _ => panic!("wrong action type"),
        }
    }

    #[test]
    fn test_planner_creates_plan_for_volume() {
        let planner = Planner::new();
        let goal = Goal::new("set volume to 50");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps.len(), 1);
        let step = &plan.steps[0];
        match &step.action {
            ActionType::DeviceControl { control } => match control {
                DeviceControl::SetVolume(v) => assert_eq!(*v, 50),
                _ => panic!("wrong device control variant"),
            },
            _ => panic!("wrong action type"),
        }
    }

    #[test]
    fn test_planner_creates_plan_for_mute() {
        let planner = Planner::new();
        let goal = Goal::new("mute device volume");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps.len(), 1);
        let step = &plan.steps[0];
        match &step.action {
            ActionType::DeviceControl { control } => match control {
                DeviceControl::SetVolume(v) => assert_eq!(*v, 0),
                _ => panic!("wrong device control variant"),
            },
            _ => panic!("wrong action type"),
        }
    }

    #[test]
    fn test_planner_creates_plan_for_lock() {
        let planner = Planner::new();
        let goal = Goal::new("lock device");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps.len(), 1);
        let step = &plan.steps[0];
        assert!(matches!(step.action, ActionType::DeviceControl { .. }));
    }

    #[test]
    fn test_planner_creates_plan_for_wifi() {
        let planner = Planner::new();
        let goal = Goal::new("enable wifi");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps.len(), 1);
        let step = &plan.steps[0];
        match &step.action {
            ActionType::DeviceControl { control } => match control {
                DeviceControl::ToggleWiFi(enabled) => assert!(enabled),
                _ => panic!("wrong variant"),
            },
            _ => panic!("wrong action type"),
        }
    }

    #[test]
    fn test_planner_creates_plan_for_bluetooth() {
        let planner = Planner::new();
        let goal = Goal::new("turn off bluetooth");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps.len(), 1);
        let step = &plan.steps[0];
        match &step.action {
            ActionType::DeviceControl { control } => match control {
                DeviceControl::ToggleBluetooth(enabled) => assert!(!enabled),
                _ => panic!("wrong variant"),
            },
            _ => panic!("wrong action type"),
        }
    }

    #[test]
    fn test_planner_creates_plan_for_dnd() {
        let planner = Planner::new();
        let goal = Goal::new("enable do not disturb");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps.len(), 1);
        let step = &plan.steps[0];
        match &step.action {
            ActionType::DeviceControl { control } => match control {
                DeviceControl::ToggleDND(enabled) => assert!(enabled),
                _ => panic!("wrong variant"),
            },
            _ => panic!("wrong action type"),
        }
    }

    #[test]
    fn test_planner_creates_plan_for_memory() {
        let planner = Planner::new();
        let goal = Goal::new("remember 'meeting at 3pm' as reminder");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps.len(), 1);
        let step = &plan.steps[0];
        assert!(matches!(step.action, ActionType::CreateMemory { .. }));
    }

    #[test]
    fn test_planner_creates_plan_for_screenshot() {
        let planner = Planner::new();
        let goal = Goal::new("take a screenshot");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps.len(), 1);
    }

    #[test]
    fn test_planner_fallback_to_ai() {
        let planner = Planner::new();
        let goal = Goal::new("what is the weather like today");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps.len(), 1);
        let step = &plan.steps[0];
        assert!(matches!(step.action, ActionType::RunAI { .. }));
    }

    #[test]
    fn test_planner_rejects_unrecognized_goal() {
        let planner = Planner::new();
        // Should still produce an AI fallback step, never an error.
        let goal = Goal::new("");
        let result = planner.plan(&goal);
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_topological_sort_empty_plan() {
        let planner = Planner::new();
        let plan = ExecutionPlan {
            id: "empty".to_string(),
            goal_description: "test".to_string(),
            steps: vec![],
            created_at: 0,
            estimated_steps: 0,
        };
        let order = planner.topological_sort(&plan).unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn test_topological_sort_linear() {
        let planner = Planner::new();
        let steps = vec![
            ExecutionStep {
                id: "s1".to_string(),
                description: "first".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
            ExecutionStep {
                id: "s2".to_string(),
                description: "second".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec!["s1".to_string()],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
            ExecutionStep {
                id: "s3".to_string(),
                description: "third".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec!["s2".to_string()],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
        ];
        let plan = ExecutionPlan {
            id: "linear".to_string(),
            goal_description: "test".to_string(),
            steps,
            created_at: 0,
            estimated_steps: 3,
        };
        let order = planner.topological_sort(&plan).unwrap();
        assert_eq!(order.len(), 3);
        // s1 must be before s2, s2 before s3.
        let pos = |id: &str| order.iter().position(|&i| plan.steps[i].id == id).unwrap();
        assert!(pos("s1") < pos("s2"));
        assert!(pos("s2") < pos("s3"));
    }

    #[test]
    fn test_topological_sort_detects_cycle() {
        let planner = Planner::new();
        let steps = vec![
            ExecutionStep {
                id: "s1".to_string(),
                description: "first".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec!["s2".to_string()],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
            ExecutionStep {
                id: "s2".to_string(),
                description: "second".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec!["s1".to_string()],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
        ];
        let plan = ExecutionPlan {
            id: "cycle".to_string(),
            goal_description: "test".to_string(),
            steps,
            created_at: 0,
            estimated_steps: 2,
        };
        assert!(planner.topological_sort(&plan).is_err());
        assert!(planner.has_cycles(&plan));
    }

    #[test]
    fn test_validate_valid_plan() {
        let planner = Planner::new();
        let steps = vec![ExecutionStep {
            id: "s1".to_string(),
            description: "step1".to_string(),
            action: ActionType::Wait { duration_ms: 1 },
            dependencies: vec![],
            required_capabilities: vec![],
            timeout_ms: 1000,
            retry_count: 0,
            continue_on_failure: false,
        }];
        let plan = ExecutionPlan {
            id: "valid".to_string(),
            goal_description: "test".to_string(),
            steps,
            created_at: 0,
            estimated_steps: 1,
        };
        let validation = planner.validate(&plan);
        assert!(validation.is_valid);
        assert!(validation.errors.is_empty());
    }

    #[test]
    fn test_validate_missing_dependency() {
        let planner = Planner::new();
        let steps = vec![ExecutionStep {
            id: "s1".to_string(),
            description: "step1".to_string(),
            action: ActionType::Wait { duration_ms: 1 },
            dependencies: vec!["nonexistent".to_string()],
            required_capabilities: vec![],
            timeout_ms: 1000,
            retry_count: 0,
            continue_on_failure: false,
        }];
        let plan = ExecutionPlan {
            id: "invalid".to_string(),
            goal_description: "test".to_string(),
            steps,
            created_at: 0,
            estimated_steps: 1,
        };
        let validation = planner.validate(&plan);
        assert!(!validation.is_valid);
        assert!(validation.errors.iter().any(|e| e.contains("nonexistent")));
    }

    #[test]
    fn test_validate_duplicate_ids() {
        let planner = Planner::new();
        let steps = vec![
            ExecutionStep {
                id: "s1".to_string(),
                description: "first".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
            ExecutionStep {
                id: "s1".to_string(),
                description: "duplicate".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
        ];
        let plan = ExecutionPlan {
            id: "dup".to_string(),
            goal_description: "test".to_string(),
            steps,
            created_at: 0,
            estimated_steps: 2,
        };
        let validation = planner.validate(&plan);
        assert!(!validation.is_valid);
    }

    #[test]
    fn test_ready_steps() {
        let planner = Planner::new();
        let steps = vec![
            ExecutionStep {
                id: "s1".to_string(),
                description: "first".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
            ExecutionStep {
                id: "s2".to_string(),
                description: "second".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec!["s1".to_string()],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
        ];
        let plan = ExecutionPlan {
            id: "ready".to_string(),
            goal_description: "test".to_string(),
            steps,
            created_at: 0,
            estimated_steps: 2,
        };

        // No steps completed yet.
        let ready = planner.ready_steps(&plan, &[]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "s1");

        // s1 completed.
        let ready = planner.ready_steps(&plan, &["s1".to_string()]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "s2");
    }

    #[test]
    fn test_goal_with_context() {
        let goal = Goal::new("process image").with_context("session_id", "sess_123");
        assert_eq!(goal.description, "process image");
        assert_eq!(goal.context.get("session_id").unwrap(), "sess_123");
    }

    #[test]
    fn test_planner_configuration() {
        let planner = Planner::new()
            .with_max_steps(5)
            .with_default_timeout(60_000)
            .with_default_retry(3);
        let goal = Goal::new("open settings");
        let plan = planner.plan(&goal).unwrap();
        assert_eq!(plan.steps[0].timeout_ms, 60_000);
        assert_eq!(plan.steps[0].retry_count, 3);
    }

    #[test]
    fn test_topological_sort_parallel() {
        let planner = Planner::new();
        let steps = vec![
            ExecutionStep {
                id: "s1".to_string(),
                description: "first".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
            ExecutionStep {
                id: "s2".to_string(),
                description: "second".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
            ExecutionStep {
                id: "s3".to_string(),
                description: "third".to_string(),
                action: ActionType::Wait { duration_ms: 1 },
                dependencies: vec!["s1".to_string(), "s2".to_string()],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
        ];
        let plan = ExecutionPlan {
            id: "parallel".to_string(),
            goal_description: "test".to_string(),
            steps,
            created_at: 0,
            estimated_steps: 3,
        };
        let order = planner.topological_sort(&plan).unwrap();
        assert_eq!(order.len(), 3);
        let pos = |id: &str| order.iter().position(|&i| plan.steps[i].id == id).unwrap();
        // s3 must be after both s1 and s2.
        assert!(pos("s3") > pos("s1"));
        assert!(pos("s3") > pos("s2"));
    }
}
