use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;

use crate::action::{ActionType, DeviceControl};
use crate::planning_context::PlanningContext;

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

/// AI completion provider for planning.
#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn complete_structured(&self, system: &str, prompt: &str) -> Result<String, String>;
}

/// Result of AI-powered plan generation.
#[derive(Debug)]
pub enum AiPlanResult {
    Plan(ExecutionPlan),
    Clarification { question: String },
    Failed { reason: String },
}

/// Decomposes high-level goals into executable execution plans.
pub struct Planner {
    max_steps_per_plan: usize,
    default_step_timeout_ms: u64,
    default_retry_count: u32,
    ai: Option<Arc<dyn AiProvider>>,
    max_ai_retries: u32,
}

impl Planner {
    pub fn new() -> Self {
        Self {
            max_steps_per_plan: 20,
            default_step_timeout_ms: 30_000,
            default_retry_count: 2,
            ai: None,
            max_ai_retries: 2,
        }
    }

    /// Attach an AI provider for novel goal decomposition.
    pub fn with_ai(mut self, ai: Arc<dyn AiProvider>) -> Self {
        self.ai = Some(ai);
        self
    }

    /// Check whether an AI provider is configured.
    pub fn has_ai(&self) -> bool {
        self.ai.is_some()
    }

    /// Set the maximum number of AI retry attempts.
    pub fn with_max_ai_retries(mut self, max: u32) -> Self {
        self.max_ai_retries = max;
        self
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

    /// Check if a plan is the RunAI fallback (heuristic couldn't handle it).
    pub fn is_runai_plan(&self, plan: &ExecutionPlan) -> bool {
        plan.steps.len() == 1
            && matches!(plan.steps[0].action, ActionType::RunAI { .. })
            && plan.steps[0].required_capabilities == vec![Capability::AiInference]
    }

    /// Decompose a novel goal using the AI provider, optionally with execution context.
    pub async fn plan_with_ai(
        &self,
        goal: &Goal,
        ctx: Option<&PlanningContext>,
    ) -> Result<AiPlanResult, String> {
        let ai = self
            .ai
            .as_ref()
            .ok_or_else(|| "no AI provider configured for planning".to_string())?;

        let system = self.build_planning_prompt(ctx);
        let goal_json = serde_json::json!({
            "description": goal.description,
            "context": goal.context,
        })
        .to_string();

        let mut remaining = self.max_ai_retries;
        let mut current_prompt = goal_json;

        loop {
            let raw = ai.complete_structured(&system, &current_prompt).await?;
            let trimmed = raw.trim();

            if let Some(q) = self.extract_clarification(trimmed) {
                return Ok(AiPlanResult::Clarification { question: q });
            }

            match self.parse_ai_plan(trimmed) {
                Ok(plan) => {
                    let validation = self.validate(&plan);
                    if validation.is_valid {
                        return Ok(AiPlanResult::Plan(plan));
                    } else if remaining > 0 {
                        let errs = validation.errors.join("; ");
                        current_prompt = format!(
                            "The plan has validation errors: {}. Please fix them and return a corrected plan.",
                            errs
                        );
                        remaining -= 1;
                    } else {
                        return Ok(AiPlanResult::Failed {
                            reason: format!(
                                "plan validation failed after retries: {}",
                                validation.errors.join("; ")
                            ),
                        });
                    }
                }
                Err(errors) => {
                    if remaining > 0 {
                        let errs = errors.join("; ");
                        current_prompt = format!(
                            "Parsing failed: {}. Return only valid JSON matching the ExecutionPlan schema.",
                            errs
                        );
                        remaining -= 1;
                    } else {
                        return Ok(AiPlanResult::Failed {
                            reason: format!(
                                "could not parse AI response into a valid plan: {}",
                                errors.join("; ")
                            ),
                        });
                    }
                }
            }
        }
    }

    /// Extract a clarification question from an AI response if present.
    fn extract_clarification(&self, response: &str) -> Option<String> {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(response) {
            if let Some(obj) = v.as_object() {
                if let Some(q) = obj.get("clarification").and_then(|c| c.as_str()) {
                    return Some(q.to_string());
                }
            }
        }
        // Also check for a "needs_clarification" boolean.
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(response) {
            if let Some(obj) = v.as_object() {
                if obj.get("needs_clarification").and_then(|c| c.as_bool()) == Some(true) {
                    return obj
                        .get("clarification_question")
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string());
                }
            }
        }
        None
    }

    /// Build the system prompt describing available actions and schema.
    /// Optionally includes rich context from PlanningContext.
    pub fn build_planning_prompt(&self, ctx: Option<&PlanningContext>) -> String {
        let mut prompt = String::with_capacity(2048);

        prompt.push_str("You are an AI planning assistant that decomposes user goals into executable step sequences.\n\n");
        prompt.push_str("## Available Action Types\n");
        prompt.push_str(&action_type_descriptions());
        prompt.push_str("\n## Available Capabilities\n");
        prompt.push_str(&capability_descriptions());
        prompt.push_str("\n## Step Schema\n");
        prompt.push_str(&step_schema());
        prompt.push_str("\n## Validation Rules\n");
        prompt.push_str("- Return valid JSON matching the schema exactly\n");
        prompt.push_str("- Use only the ActionType and Capability values listed above\n");
        prompt.push_str("- Step IDs must be unique, sequential (\"s1\", \"s2\", ...)\n");
        prompt.push_str("- Dependencies must reference existing step IDs\n");
        prompt.push_str("- No circular dependencies\n");
        prompt.push_str("- Each step must have a clear, actionable description\n");
        prompt.push_str("- Maximum 20 steps per plan\n");

        // Add context if available.
        if let Some(ctx) = ctx {
            prompt.push_str("\n## Current Context\n");
            if let Some(ref app) = ctx.active_app {
                prompt.push_str(&format!("- Active application: {}\n", app));
            }
            if let Some(ref telemetry) = ctx.device_telemetry {
                prompt.push_str(&format!(
                    "- Device: battery={}%, wifi={}, bluetooth={}\n",
                    telemetry
                        .battery_level
                        .map_or("unknown".into(), |v| v.to_string()),
                    telemetry
                        .wifi_enabled
                        .map_or("unknown", |v| if v { "on" } else { "off" }),
                    telemetry
                        .bluetooth_enabled
                        .map_or("unknown", |v| if v { "on" } else { "off" }),
                ));
            }
            if let Some(ref net) = ctx.network_state {
                prompt.push_str(&format!(
                    "- Network: online={}, type={}\n",
                    net.is_online
                        .map_or("unknown", |v| if v { "yes" } else { "no" }),
                    net.network_type.as_deref().unwrap_or("unknown"),
                ));
            }
            if let Some(ref screen) = ctx.screen_summary {
                if !screen.is_empty() {
                    prompt.push_str(&format!("- Screen: {}\n", screen));
                }
            }
            if let Some(ref hist) = ctx.execution_history {
                prompt.push_str("- Recent execution history:\n");
                if !hist.recent_successes.is_empty() {
                    prompt.push_str(&format!(
                        "  - Recent successes: {}\n",
                        hist.recent_successes.join(", ")
                    ));
                }
                if !hist.recent_failures.is_empty() {
                    prompt.push_str(&format!(
                        "  - Recent failures: {}\n",
                        hist.recent_failures.join(", ")
                    ));
                }
                if !hist.frequent_actions.is_empty() {
                    prompt.push_str(&format!(
                        "  - Frequent actions: {}\n",
                        hist.frequent_actions.join(", ")
                    ));
                }
                if !hist.recent_apps.is_empty() {
                    prompt.push_str(&format!(
                        "  - Recent apps: {}\n",
                        hist.recent_apps.join(", ")
                    ));
                }
            }
            let filtered =
                crate::planning_context::CapabilityFilter::filter_capabilities_for_context(ctx);
            prompt.push_str(&format!(
                "- Available capabilities: {}\n",
                filtered
                    .iter()
                    .map(|c| format!("{:?}", c))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        prompt.push_str("\n## Examples\n");
        prompt.push_str(r#"Example 1: {"steps":[{"id":"s1","description":"open calculator app","action":{"OpenApp":{"app_id":"calculator","data":null}},"dependencies":[],"required_capabilities":["AutomationWorkflow"],"timeout_ms":30000,"retry_count":2,"continue_on_failure":false}]}"#);
        prompt.push_str("\n\n");
        prompt.push_str(r#"Example 2: {"steps":[{"id":"s1","description":"search for documents","action":{"SearchMemory":{"query":"documents","max_results":10}},"dependencies":[],"required_capabilities":["MemoryQuery"],"timeout_ms":30000,"retry_count":2,"continue_on_failure":false}]}"#);

        prompt.push_str("\n\n## Output Format\n");
        prompt.push_str("Return a JSON object with one of two forms:\n");
        prompt.push_str("1. A plan: {\"steps\": [...]}\n");
        prompt.push_str("2. A clarification: {\"needs_clarification\": true, \"clarification_question\": \"...\"}\n");

        prompt
    }

    /// Parse an AI response string into an ExecutionPlan.
    pub fn parse_ai_plan(&self, json: &str) -> Result<ExecutionPlan, Vec<String>> {
        let mut errors = Vec::new();

        let value: serde_json::Value = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!("invalid JSON: {}", e));
                return Err(errors);
            }
        };

        let obj = match value.as_object() {
            Some(o) => o,
            None => {
                errors.push("response is not a JSON object".to_string());
                return Err(errors);
            }
        };

        let steps_val = match obj.get("steps") {
            Some(v) => v,
            None => {
                errors.push("missing 'steps' array in response".to_string());
                return Err(errors);
            }
        };

        let steps_arr = match steps_val.as_array() {
            Some(a) => a,
            None => {
                errors.push("'steps' is not an array".to_string());
                return Err(errors);
            }
        };

        if steps_arr.is_empty() {
            errors.push("plan has zero steps".to_string());
            return Err(errors);
        }

        let mut steps = Vec::with_capacity(steps_arr.len());
        let mut seen_ids = HashSet::new();

        for (i, step_val) in steps_arr.iter().enumerate() {
            let step_obj = match step_val.as_object() {
                Some(o) => o,
                None => {
                    errors.push(format!("step {} is not a JSON object", i));
                    continue;
                }
            };

            let id = match step_obj.get("id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => {
                    errors.push(format!("step {} missing 'id' field", i));
                    continue;
                }
            };

            if !seen_ids.insert(id.clone()) {
                errors.push(format!("duplicate step id '{}'", id));
                continue;
            }

            let description = match step_obj.get("description").and_then(|v| v.as_str()) {
                Some(d) => d.to_string(),
                None => {
                    errors.push(format!("step '{}' missing 'description'", id));
                    continue;
                }
            };

            let action = match step_obj.get("action") {
                Some(a) => match serde_json::from_value::<ActionType>(a.clone()) {
                    Ok(act) => act,
                    Err(e) => {
                        errors.push(format!("step '{}' invalid action: {}", id, e));
                        continue;
                    }
                },
                None => {
                    errors.push(format!("step '{}' missing 'action'", id));
                    continue;
                }
            };

            let dependencies: Vec<String> = step_obj
                .get("dependencies")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let required_capabilities: Vec<Capability> = step_obj
                .get("required_capabilities")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let timeout_ms = step_obj
                .get("timeout_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(self.default_step_timeout_ms);

            let retry_count = step_obj
                .get("retry_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(self.default_retry_count as u64) as u32;

            let continue_on_failure = step_obj
                .get("continue_on_failure")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            steps.push(ExecutionStep {
                id,
                description,
                action,
                dependencies,
                required_capabilities,
                timeout_ms,
                retry_count,
                continue_on_failure,
            });
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        let step_count = steps.len();

        if step_count > self.max_steps_per_plan {
            errors.push(format!(
                "plan has {} steps, exceeding maximum of {}",
                step_count, self.max_steps_per_plan
            ));
            return Err(errors);
        }

        let plan = ExecutionPlan {
            id: uuid::Uuid::new_v4().to_string(),
            goal_description: String::new(),
            steps,
            created_at: chrono::Utc::now().timestamp_millis(),
            estimated_steps: step_count,
        };

        Ok(plan)
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

fn action_type_descriptions() -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(1024);
    writeln!(s, "- Speak: Speak text aloud").ok();
    writeln!(
        s,
        "- Notify: Show a notification with title, body, priority"
    )
    .ok();
    writeln!(s, "- OpenApp: Open an application by app_id").ok();
    writeln!(s, "- LaunchActivity: Launch a specific activity in an app").ok();
    writeln!(s, "- Clipboard: Copy/paste/clear clipboard").ok();
    writeln!(
        s,
        "- CreateMemory: Store a memory with title, content, category, tags, importance"
    )
    .ok();
    writeln!(s, "- SearchMemory: Search stored memories by query").ok();
    writeln!(s, "- RunAI: Run an AI inference with a prompt").ok();
    writeln!(s, "- CaptureVoice: Capture voice audio for a duration").ok();
    writeln!(s, "- AnalyzeImage: Analyze an image at a path").ok();
    writeln!(s, "- DeviceControl: Control device settings (brightness, volume, wifi, bluetooth, dnd, lock, power)").ok();
    writeln!(
        s,
        "- PluginInvocation: Invoke a plugin by plugin_id and method"
    )
    .ok();
    writeln!(s, "- Wait: Wait for a duration in milliseconds").ok();
    writeln!(s, "- SubWorkflow: Execute a sub-workflow by workflow_id").ok();
    writeln!(
        s,
        "- InputInjection: Inject input events (click, type, key, scroll, etc.)"
    )
    .ok();
    writeln!(
        s,
        "- ClickScreenElement: Click a screen element matching a query"
    )
    .ok();
    writeln!(
        s,
        "- TypeIntoScreenElement: Type text into a screen element"
    )
    .ok();
    writeln!(s, "- ClickScreenText: Click on screen text").ok();
    writeln!(s, "- DragScreenElements: Drag from one element to another").ok();
    writeln!(s, "- SwipeScreenElements: Swipe across screen elements").ok();
    s
}

fn capability_descriptions() -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(512);
    writeln!(s, "- ScreenCapture: Capture screen frames").ok();
    writeln!(s, "- ScreenGrounding: Ground UI element coordinates").ok();
    writeln!(s, "- Ocr: Optical character recognition").ok();
    writeln!(s, "- InputMouse: Mouse input simulation").ok();
    writeln!(s, "- InputKeyboard: Keyboard input simulation").ok();
    writeln!(s, "- InputTouch: Touch input simulation").ok();
    writeln!(s, "- AutomationWorkflow: Run automation workflows").ok();
    writeln!(s, "- MemoryQuery: Query stored memories").ok();
    writeln!(s, "- MemoryStore: Store new memories").ok();
    writeln!(s, "- PluginInvocation: Invoke external plugins").ok();
    writeln!(s, "- AiInference: Run AI inference").ok();
    writeln!(s, "- VoiceCapture: Capture voice input").ok();
    writeln!(s, "- DeviceControl: Control device hardware settings").ok();
    s
}

fn step_schema() -> String {
    r#"{
  "id": "string (unique, sequential like 's1', 's2')",
  "description": "string (concrete, actionable description)",
  "action": { "ActionTypeName": { "param1": "value1", ... } },
  "dependencies": ["step_id", ...],
  "required_capabilities": ["CapabilityName", ...],
  "timeout_ms": 30000,
  "retry_count": 2,
  "continue_on_failure": false
}"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    pub(crate) struct MockAiProvider {
        response: std::sync::Mutex<Result<String, String>>,
        call_count: std::sync::atomic::AtomicU32,
    }

    #[allow(dead_code)]
    impl MockAiProvider {
        pub(crate) fn new(response: Result<String, String>) -> Self {
            Self {
                response: std::sync::Mutex::new(response),
                call_count: std::sync::atomic::AtomicU32::new(0),
            }
        }

        pub(crate) fn call_count(&self) -> u32 {
            self.call_count.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl AiProvider for MockAiProvider {
        async fn complete_structured(
            &self,
            _system: &str,
            _prompt: &str,
        ) -> Result<String, String> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.response.lock().unwrap().clone()
        }
    }

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

    #[test]
    fn test_is_runai_fallback_true() {
        let planner = Planner::new();
        let steps = vec![ExecutionStep {
            id: "s1".into(),
            description: "ai fallback".into(),
            action: ActionType::RunAI {
                prompt: "test".into(),
                session_id: None,
            },
            dependencies: vec![],
            required_capabilities: vec![Capability::AiInference],
            timeout_ms: 30000,
            retry_count: 2,
            continue_on_failure: false,
        }];
        let plan = ExecutionPlan {
            id: "test".into(),
            goal_description: "test".into(),
            steps,
            created_at: 0,
            estimated_steps: 1,
        };
        assert!(planner.is_runai_plan(&plan));
    }

    #[test]
    fn test_is_runai_fallback_false_heuristic() {
        let planner = Planner::new();
        let goal = Goal::new("open calculator");
        let plan = planner.plan(&goal).unwrap();
        assert!(!planner.is_runai_plan(&plan));
    }

    #[test]
    fn test_is_runai_fallback_false_multistep() {
        let planner = Planner::new();
        let steps = vec![
            ExecutionStep {
                id: "s1".into(),
                description: "first".into(),
                action: ActionType::Wait { duration_ms: 100 },
                dependencies: vec![],
                required_capabilities: vec![],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
            ExecutionStep {
                id: "s2".into(),
                description: "run ai".into(),
                action: ActionType::RunAI {
                    prompt: "test".into(),
                    session_id: None,
                },
                dependencies: vec![],
                required_capabilities: vec![Capability::AiInference],
                timeout_ms: 1000,
                retry_count: 0,
                continue_on_failure: false,
            },
        ];
        let plan = ExecutionPlan {
            id: "test".into(),
            goal_description: "test".into(),
            steps,
            created_at: 0,
            estimated_steps: 2,
        };
        // Multi-step plan with RunAI should NOT be considered fallback.
        assert!(!planner.is_runai_plan(&plan));
    }

    #[test]
    fn test_build_planning_prompt_contains_action_types() {
        let planner = Planner::new();
        let prompt = planner.build_planning_prompt(None);
        assert!(prompt.contains("Speak"));
        assert!(prompt.contains("OpenApp"));
        assert!(prompt.contains("DeviceControl"));
    }

    #[test]
    fn test_build_planning_prompt_contains_capabilities() {
        let planner = Planner::new();
        let prompt = planner.build_planning_prompt(None);
        assert!(prompt.contains("ScreenCapture"));
        assert!(prompt.contains("InputKeyboard"));
        assert!(prompt.contains("AiInference"));
    }

    #[test]
    fn test_build_planning_prompt_contains_schema() {
        let planner = Planner::new();
        let prompt = planner.build_planning_prompt(None);
        assert!(prompt.contains("id"));
        assert!(prompt.contains("action"));
        assert!(prompt.contains("dependencies"));
    }

    #[test]
    fn test_parse_ai_plan_valid() {
        let planner = Planner::new();
        let json = r#"{"steps":[{"id":"s1","description":"open calculator","action":{"OpenApp":{"app_id":"calculator","data":null}},"dependencies":[],"required_capabilities":["AutomationWorkflow"],"timeout_ms":30000,"retry_count":2,"continue_on_failure":false}]}"#;
        let result = planner.parse_ai_plan(json);
        assert!(result.is_ok());
        let plan = result.unwrap();
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].id, "s1");
    }

    #[test]
    fn test_parse_ai_plan_invalid_json() {
        let planner = Planner::new();
        let result = planner.parse_ai_plan("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_ai_plan_missing_steps() {
        let planner = Planner::new();
        let result = planner.parse_ai_plan(r#"{"foo":"bar"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_ai_plan_duplicate_step_ids() {
        let planner = Planner::new();
        let json = r#"{"steps":[
            {"id":"s1","description":"first","action":{"Wait":{"duration_ms":100}},"dependencies":[],"required_capabilities":[]},
            {"id":"s1","description":"duplicate","action":{"Wait":{"duration_ms":100}},"dependencies":[],"required_capabilities":[]}
        ]}"#;
        let result = planner.parse_ai_plan(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_ai_plan_empty_steps() {
        let planner = Planner::new();
        let json = r#"{"steps":[]}"#;
        let result = planner.parse_ai_plan(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_planner_with_ai_constructs() {
        let mock = Arc::new(MockAiProvider::new(Ok("{\"steps\":[]}".into())));
        let planner = Planner::new().with_ai(mock);
        assert!(planner.has_ai());
    }

    #[test]
    fn test_plan_unchanged_without_ai() {
        let planner = Planner::new();
        assert!(!planner.has_ai());
        let goal = Goal::new("open calculator");
        let plan = planner.plan(&goal).unwrap();
        assert!(matches!(plan.steps[0].action, ActionType::OpenApp { .. }));
    }

    #[test]
    fn test_plan_with_ai_success() {
        let json = r#"{"steps":[{"id":"s1","description":"custom action","action":{"Wait":{"duration_ms":100}},"dependencies":[],"required_capabilities":[],"timeout_ms":1000,"retry_count":0,"continue_on_failure":false}]}"#;
        let mock = Arc::new(MockAiProvider::new(Ok(json.to_string())));
        let planner = Planner::new().with_ai(mock);
        let goal = Goal::new("do something novel");
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(planner.plan_with_ai(&goal, None));
        match result {
            Ok(AiPlanResult::Plan(p)) => {
                assert_eq!(p.steps.len(), 1);
                assert_eq!(p.steps[0].description, "custom action");
            }
            _ => panic!("expected Plan variant"),
        }
    }

    #[test]
    fn test_plan_with_ai_clarification() {
        let json =
            r#"{"needs_clarification":true,"clarification_question":"What app should I open?"}"#;
        let mock = Arc::new(MockAiProvider::new(Ok(json.to_string())));
        let planner = Planner::new().with_ai(mock);
        let goal = Goal::new("open something");
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(planner.plan_with_ai(&goal, None));
        match result {
            Ok(AiPlanResult::Clarification { question }) => {
                assert_eq!(question, "What app should I open?");
            }
            _ => panic!("expected Clarification variant"),
        }
    }

    #[test]
    fn test_plan_with_ai_engine_unavailable() {
        let planner = Planner::new();
        let goal = Goal::new("do something");
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(planner.plan_with_ai(&goal, None));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no AI provider configured"));
    }

    #[test]
    fn test_plan_with_ai_context_included() {
        let json = r#"{"steps":[{"id":"s1","description":"process in /home/docs","action":{"SearchMemory":{"query":"files","max_results":10}},"dependencies":[],"required_capabilities":["MemoryQuery"],"timeout_ms":30000,"retry_count":2,"continue_on_failure":false}]}"#;
        let mock = Arc::new(MockAiProvider::new(Ok(json.to_string())));
        let planner = Planner::new().with_ai(mock);
        let goal = Goal::new("find files").with_context("location", "/home/docs");
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(planner.plan_with_ai(&goal, None));
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_clarification_detected() {
        let planner = Planner::new();
        let response = r#"{"needs_clarification":true,"clarification_question":"Which app?"}"#;
        let q = planner.extract_clarification(response);
        assert_eq!(q, Some("Which app?".to_string()));
    }

    #[test]
    fn test_extract_clarification_not_present() {
        let planner = Planner::new();
        let response = r#"{"steps":[]}"#;
        let q = planner.extract_clarification(response);
        assert_eq!(q, None);
    }

    #[test]
    fn test_build_planning_prompt_with_context() {
        use crate::planning_context::PlanningContextBuilder;

        let planner = Planner::new();
        let goal = Goal::new("test");
        let ctx = PlanningContextBuilder::new().build(&goal);
        let prompt = planner.build_planning_prompt(Some(&ctx));
        assert!(prompt.contains("Current Context"));
        assert!(prompt.contains("Available capabilities"));
    }
}
