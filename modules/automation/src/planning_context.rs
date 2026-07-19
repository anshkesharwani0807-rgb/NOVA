use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::goal_registry::GoalRegistry;
use crate::history::{ExecutionRecord, ExecutionStatus, HistoryStore};
use crate::planner::{Capability, Goal, Planner};
use crate::world_state::{DeviceTelemetry, NetworkState, WorldSnapshot, WorldState};

/// Rich context for AI-powered planning, providing environment awareness.
#[derive(Debug, Clone)]
pub struct PlanningContext {
    pub goal: Goal,
    pub world_snapshot: Option<WorldSnapshot>,
    pub active_app: Option<String>,
    pub screen_summary: Option<String>,
    pub device_telemetry: Option<DeviceTelemetry>,
    pub network_state: Option<NetworkState>,
    pub planner_capabilities: Vec<Capability>,
    pub execution_history: Option<ExecutionHistorySummary>,
    pub available_tools: Vec<String>,
    pub user_preferences: HashMap<String, String>,
    pub timestamp: i64,
}

/// Compact summary of past execution history for AI context.
#[derive(Debug, Clone)]
pub struct ExecutionHistorySummary {
    pub recent_successes: Vec<String>,
    pub recent_failures: Vec<String>,
    pub frequent_actions: Vec<String>,
    pub recent_apps: Vec<String>,
}

/// Builds a PlanningContext from available data sources.
pub struct PlanningContextBuilder {
    world_state: Option<Arc<RwLock<WorldState>>>,
    history_store: Option<Arc<dyn HistoryStore>>,
    planner: Option<Arc<Planner>>,
    goal_registry: Option<Arc<GoalRegistry>>,
    user_preferences: HashMap<String, String>,
    max_compressed_tokens: usize,
}

impl PlanningContextBuilder {
    pub fn new() -> Self {
        Self {
            world_state: None,
            history_store: None,
            planner: None,
            goal_registry: None,
            user_preferences: HashMap::new(),
            max_compressed_tokens: 1536,
        }
    }

    pub fn with_world_state(mut self, ws: Arc<RwLock<WorldState>>) -> Self {
        self.world_state = Some(ws);
        self
    }

    pub fn with_history(mut self, h: Arc<dyn HistoryStore>) -> Self {
        self.history_store = Some(h);
        self
    }

    pub fn with_planner(mut self, p: Arc<Planner>) -> Self {
        self.planner = Some(p);
        self
    }

    pub fn with_goal_registry(mut self, gr: Arc<GoalRegistry>) -> Self {
        self.goal_registry = Some(gr);
        self
    }

    pub fn with_user_preferences(mut self, prefs: HashMap<String, String>) -> Self {
        self.user_preferences = prefs;
        self
    }

    pub fn with_max_compressed_tokens(mut self, max: usize) -> Self {
        self.max_compressed_tokens = max;
        self
    }

    /// Build a PlanningContext for the given goal.
    /// Gracefully handles unavailable components (None sources are skipped).
    pub fn build(&self, goal: &Goal) -> PlanningContext {
        let timestamp = chrono::Utc::now().timestamp_millis();

        let (world_snapshot, active_app, device_telemetry, network_state, screen_summary) =
            if let Some(ref ws) = self.world_state {
                let guard = ws.read();
                let snap = guard.snapshot();
                let app = guard.active_app().map(|s| s.to_string());
                let telemetry = guard.device_telemetry().cloned();
                let net = guard.network_state().cloned();
                let screen = Self::summarize_screen(&snap);
                drop(guard);
                (Some(snap), app, telemetry, net, screen)
            } else {
                (None, None, None, None, None)
            };

        let planner_capabilities = self
            .planner
            .as_ref()
            .map(|_| {
                vec![
                    Capability::ScreenCapture,
                    Capability::ScreenGrounding,
                    Capability::Ocr,
                    Capability::InputMouse,
                    Capability::InputKeyboard,
                    Capability::InputTouch,
                    Capability::AutomationWorkflow,
                    Capability::MemoryQuery,
                    Capability::MemoryStore,
                    Capability::PluginInvocation,
                    Capability::AiInference,
                    Capability::VoiceCapture,
                    Capability::DeviceControl,
                ]
            })
            .unwrap_or_default();

        let execution_history = self.history_store.as_ref().map(|h| {
            let records = h.recent(50);
            HistorySummarizer::summarize(&records)
        });

        let available_tools = vec![
            "execute_goal".to_string(),
            "check_status".to_string(),
            "list_history".to_string(),
            "cancel".to_string(),
        ];

        PlanningContext {
            goal: goal.clone(),
            world_snapshot,
            active_app,
            screen_summary,
            device_telemetry,
            network_state,
            planner_capabilities,
            execution_history,
            available_tools,
            user_preferences: self.user_preferences.clone(),
            timestamp,
        }
    }

    fn summarize_screen(snap: &WorldSnapshot) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();

        if let Some(ref ocr) = snap.ocr {
            let texts: Vec<&str> = ocr
                .regions
                .iter()
                .take(5)
                .map(|r| r.text.as_str())
                .collect();
            if !texts.is_empty() {
                parts.push(format!("visible text: {}", texts.join(" | ")));
            } else if !ocr.text.is_empty() {
                let truncated: String = ocr.text.chars().take(200).collect();
                parts.push(format!("text: {}", truncated));
            }
        }

        if let Some(ref tree) = snap.ui_tree {
            let app = tree
                .root
                .attributes
                .get("package")
                .or_else(|| tree.root.attributes.get("activity"));
            if let Some(f) = app {
                parts.push(format!("focused UI: {}", f));
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join("; "))
        }
    }
}

impl Default for PlanningContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Compresses context to fit within token limits using priority-based truncation.
pub struct ContextCompressor {
    max_tokens: usize,
}

impl ContextCompressor {
    pub fn new(max_tokens: usize) -> Self {
        Self { max_tokens }
    }

    /// Compress context text into a compact string, truncating low-priority sections first.
    /// Priority order (highest first): current goal, active app, screen summary,
    /// recent execution history, device/network state, older history.
    pub fn compress(&self, ctx: &PlanningContext) -> String {
        let mut sections: Vec<(u32, String)> = Vec::new();

        // Priority 1: goal description.
        sections.push((1, format!("Goal: {}", ctx.goal.description)));

        // Priority 2: active app.
        if let Some(ref app) = ctx.active_app {
            sections.push((2, format!("App: {}", app)));
        }

        // Priority 3: screen summary.
        if let Some(ref screen) = ctx.screen_summary {
            sections.push((3, format!("Screen: {}", screen)));
        }

        // Priority 4: recent execution history.
        if let Some(ref hist) = ctx.execution_history {
            let hist_text = self.format_history_summary(hist);
            sections.push((4, hist_text));
        }

        // Priority 5: device/network state.
        let device_text = self.format_device_state(&ctx.device_telemetry, &ctx.network_state);
        if !device_text.is_empty() {
            sections.push((5, device_text));
        }

        // Priority 6: user preferences.
        if !ctx.user_preferences.is_empty() {
            let prefs_text = self.format_preferences(&ctx.user_preferences);
            if !prefs_text.is_empty() {
                sections.push((6, prefs_text));
            }
        }

        // Sort by priority (ascending), then build string respecting token budget.
        sections.sort_by_key(|(p, _)| *p);

        let mut result = String::with_capacity(2048);
        let mut estimated_tokens = 0usize;

        for (_, section) in &sections {
            let section_tokens = section.len().div_ceil(4);
            if estimated_tokens + section_tokens > self.max_tokens {
                if (estimated_tokens as f64) < (self.max_tokens as f64 * 0.5) {
                    result.push_str(&self.truncate(section, self.max_tokens - estimated_tokens));
                }
                break;
            }
            result.push_str(section);
            result.push('\n');
            estimated_tokens += section_tokens;
        }

        result
    }

    fn truncate(&self, text: &str, budget_chars: usize) -> String {
        if text.len() <= budget_chars || budget_chars < 20 {
            return text.chars().take(budget_chars).collect();
        }
        let mut s: String = text.chars().take(budget_chars.saturating_sub(3)).collect();
        s.push_str("...");
        s
    }

    fn format_history_summary(&self, hist: &ExecutionHistorySummary) -> String {
        let mut parts = Vec::new();

        if !hist.recent_successes.is_empty() {
            let successes: Vec<&str> = hist
                .recent_successes
                .iter()
                .take(3)
                .map(|s| s.as_str())
                .collect();
            parts.push(format!("recent successes: {}", successes.join(", ")));
        }

        if !hist.recent_failures.is_empty() {
            let failures: Vec<&str> = hist
                .recent_failures
                .iter()
                .take(3)
                .map(|s| s.as_str())
                .collect();
            parts.push(format!("recent fails: {}", failures.join(", ")));
        }

        if !hist.frequent_actions.is_empty() {
            let freq: Vec<&str> = hist
                .frequent_actions
                .iter()
                .take(5)
                .map(|s| s.as_str())
                .collect();
            parts.push(format!("frequent: {}", freq.join(", ")));
        }

        if !hist.recent_apps.is_empty() {
            let apps: Vec<&str> = hist
                .recent_apps
                .iter()
                .take(3)
                .map(|s| s.as_str())
                .collect();
            parts.push(format!("apps: {}", apps.join(", ")));
        }

        if parts.is_empty() {
            "History: none".to_string()
        } else {
            format!("History: {}", parts.join(" | "))
        }
    }

    fn format_device_state(
        &self,
        telemetry: &Option<DeviceTelemetry>,
        network: &Option<NetworkState>,
    ) -> String {
        let mut parts = Vec::new();

        if let Some(ref t) = telemetry {
            if let Some(b) = t.battery_level {
                parts.push(format!("battery={}%", b));
            }
            if let Some(c) = t.is_charging {
                parts.push(if c { "charging" } else { "not charging" }.to_string());
            }
            if let Some(w) = t.wifi_enabled {
                parts.push(if w { "wifi=on" } else { "wifi=off" }.to_string());
            }
            if let Some(b) = t.bluetooth_enabled {
                parts.push(if b { "bt=on" } else { "bt=off" }.to_string());
            }
        }

        if let Some(ref n) = network {
            if let Some(o) = n.is_online {
                parts.push(if o { "online" } else { "offline" }.to_string());
            }
        }

        if parts.is_empty() {
            String::new()
        } else {
            format!("Device: {}", parts.join(", "))
        }
    }

    fn format_preferences(&self, prefs: &HashMap<String, String>) -> String {
        let items: Vec<String> = prefs.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        if items.is_empty() {
            String::new()
        } else {
            format!("Preferences: {}", items.join(", "))
        }
    }
}

/// Generates compact execution history summaries from raw records.
pub struct HistorySummarizer;

impl HistorySummarizer {
    pub fn summarize(records: &[ExecutionRecord]) -> ExecutionHistorySummary {
        let mut recent_successes = Vec::new();
        let mut recent_failures = Vec::new();
        let mut action_counts: HashMap<String, u32> = HashMap::new();
        let mut app_counts: HashMap<String, u32> = HashMap::new();

        for record in records.iter().take(50) {
            match record.status {
                ExecutionStatus::Completed => {
                    recent_successes.push(record.workflow_name.clone());
                }
                ExecutionStatus::Failed
                | ExecutionStatus::TimedOut
                | ExecutionStatus::Cancelled => {
                    recent_failures.push(record.workflow_name.clone());
                }
                _ => {}
            }

            *action_counts
                .entry(record.workflow_name.clone())
                .or_insert(0) += 1;

            if let Some(app) = Self::extract_app_name(&record.workflow_name) {
                *app_counts.entry(app).or_insert(0) += 1;
            }
        }

        // Keep only most recent 5 successes/failures.
        recent_successes.truncate(5);
        recent_failures.truncate(5);

        let mut frequent_actions: Vec<(u32, String)> =
            action_counts.into_iter().map(|(k, v)| (v, k)).collect();
        frequent_actions.sort_by_key(|b| std::cmp::Reverse(b.0));
        let frequent_actions: Vec<String> = frequent_actions
            .into_iter()
            .take(5)
            .map(|(_, name)| name)
            .collect();

        let mut recent_apps: Vec<(u32, String)> =
            app_counts.into_iter().map(|(k, v)| (v, k)).collect();
        recent_apps.sort_by_key(|b| std::cmp::Reverse(b.0));
        let recent_apps: Vec<String> = recent_apps
            .into_iter()
            .take(3)
            .map(|(_, name)| name)
            .collect();

        ExecutionHistorySummary {
            recent_successes,
            recent_failures,
            frequent_actions,
            recent_apps,
        }
    }

    fn extract_app_name(workflow_name: &str) -> Option<String> {
        // Try to extract app name from common patterns like "open chrome", "launch settings"
        let lower = workflow_name.to_lowercase();
        for prefix in &["open ", "launch ", "start "] {
            if let Some(name) = lower.strip_prefix(prefix) {
                return Some(name.to_string());
            }
        }
        None
    }
}

/// Filters capabilities based on current context availability.
pub struct CapabilityFilter;

impl CapabilityFilter {
    /// Return only capabilities that are currently available given the context.
    pub fn available_capabilities(
        ctx: &PlanningContext,
        all_capabilities: &[Capability],
    ) -> Vec<Capability> {
        all_capabilities
            .iter()
            .filter(|cap| Self::is_capability_available(cap, ctx))
            .cloned()
            .collect()
    }

    /// Return a filtered list of capabilities aware of context constraints.
    pub fn filter_capabilities_for_context(ctx: &PlanningContext) -> Vec<Capability> {
        let all = vec![
            Capability::ScreenCapture,
            Capability::ScreenGrounding,
            Capability::Ocr,
            Capability::InputMouse,
            Capability::InputKeyboard,
            Capability::InputTouch,
            Capability::AutomationWorkflow,
            Capability::MemoryQuery,
            Capability::MemoryStore,
            Capability::PluginInvocation,
            Capability::AiInference,
            Capability::VoiceCapture,
            Capability::DeviceControl,
        ];
        Self::available_capabilities(ctx, &all)
    }

    fn is_capability_available(cap: &Capability, ctx: &PlanningContext) -> bool {
        match cap {
            // Network-dependent capabilities.
            Capability::MemoryQuery | Capability::MemoryStore => ctx
                .network_state
                .as_ref()
                .and_then(|n| n.is_online)
                .unwrap_or(true),
            // Voice requires audio hardware.
            Capability::VoiceCapture => true, // Assume available unless explicitly disabled.
            // Screen-dependent capabilities.
            Capability::ScreenCapture | Capability::ScreenGrounding | Capability::Ocr => {
                ctx.world_snapshot.is_some() || ctx.screen_summary.is_some()
            }
            // Input capabilities always available on a device.
            Capability::InputMouse | Capability::InputKeyboard | Capability::InputTouch => true,
            // AI inference requires AI provider.
            Capability::AiInference => true,
            // Device control always available.
            Capability::DeviceControl => true,
            // Automation workflows always available.
            Capability::AutomationWorkflow => true,
            // Plugin invocation always available.
            Capability::PluginInvocation => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::InMemoryHistory;
    use crate::planner::Goal;
    use std::sync::Arc;

    fn make_sample_records() -> Vec<ExecutionRecord> {
        vec![
            ExecutionRecord {
                execution_id: "e1".into(),
                workflow_id: "wf1".into(),
                workflow_name: "open chrome".into(),
                status: ExecutionStatus::Completed,
                started_at: 1000,
                duration_ms: 500,
                steps_succeeded: 1,
                steps_failed: 0,
                steps_total: 1,
                error: None,
            },
            ExecutionRecord {
                execution_id: "e2".into(),
                workflow_id: "wf2".into(),
                workflow_name: "search documents".into(),
                status: ExecutionStatus::Completed,
                started_at: 2000,
                duration_ms: 1000,
                steps_succeeded: 2,
                steps_failed: 0,
                steps_total: 2,
                error: None,
            },
            ExecutionRecord {
                execution_id: "e3".into(),
                workflow_id: "wf3".into(),
                workflow_name: "set brightness".into(),
                status: ExecutionStatus::Failed,
                started_at: 3000,
                duration_ms: 200,
                steps_succeeded: 0,
                steps_failed: 1,
                steps_total: 1,
                error: Some("permission denied".into()),
            },
            ExecutionRecord {
                execution_id: "e4".into(),
                workflow_id: "wf4".into(),
                workflow_name: "launch settings".into(),
                status: ExecutionStatus::Completed,
                started_at: 4000,
                duration_ms: 300,
                steps_succeeded: 1,
                steps_failed: 0,
                steps_total: 1,
                error: None,
            },
        ]
    }

    #[test]
    fn test_planning_context_builder_no_sources() {
        let builder = PlanningContextBuilder::new();
        let goal = Goal::new("open calculator");
        let ctx = builder.build(&goal);
        assert_eq!(ctx.goal.description, "open calculator");
        assert!(ctx.world_snapshot.is_none());
        assert!(ctx.active_app.is_none());
        assert!(ctx.execution_history.is_none());
        assert!(ctx.screen_summary.is_none());
    }

    #[test]
    fn test_planning_context_builder_with_history() {
        let history = Arc::new(InMemoryHistory::with_max(100)) as Arc<dyn HistoryStore>;
        for rec in make_sample_records() {
            history.store(rec);
        }
        let builder = PlanningContextBuilder::new().with_history(history);
        let ctx = builder.build(&Goal::new("do something"));
        assert!(ctx.execution_history.is_some());
        let hist = ctx.execution_history.unwrap();
        assert!(!hist.recent_successes.is_empty());
        assert!(!hist.recent_failures.is_empty());
    }

    #[test]
    fn test_planning_context_builder_with_preferences() {
        let mut prefs = HashMap::new();
        prefs.insert("theme".to_string(), "dark".to_string());
        prefs.insert("language".to_string(), "en".to_string());
        let builder = PlanningContextBuilder::new().with_user_preferences(prefs);
        let ctx = builder.build(&Goal::new("test"));
        assert_eq!(ctx.user_preferences.get("theme").unwrap(), "dark");
        assert_eq!(ctx.user_preferences.get("language").unwrap(), "en");
    }

    #[test]
    fn test_history_summarizer_empty() {
        let summary = HistorySummarizer::summarize(&[]);
        assert!(summary.recent_successes.is_empty());
        assert!(summary.recent_failures.is_empty());
        assert!(summary.frequent_actions.is_empty());
    }

    #[test]
    fn test_history_summarizer_with_records() {
        let summary = HistorySummarizer::summarize(&make_sample_records());
        assert!(summary
            .recent_successes
            .contains(&"open chrome".to_string()));
        assert!(summary
            .recent_failures
            .contains(&"set brightness".to_string()));
        assert!(!summary.frequent_actions.is_empty());
    }

    #[test]
    fn test_history_summarizer_app_extraction() {
        let summary = HistorySummarizer::summarize(&make_sample_records());
        assert!(summary.recent_apps.contains(&"chrome".to_string()));
        assert!(summary.recent_apps.contains(&"settings".to_string()));
    }

    #[test]
    fn test_context_compressor_empty_context() {
        let ctx = PlanningContextBuilder::new().build(&Goal::new("test"));
        let compressor = ContextCompressor::new(1024);
        let compressed = compressor.compress(&ctx);
        assert!(compressed.contains("Goal: test"));
    }

    #[test]
    fn test_context_compressor_with_history() {
        let history = Arc::new(InMemoryHistory::with_max(100)) as Arc<dyn HistoryStore>;
        for rec in make_sample_records() {
            history.store(rec);
        }
        let builder = PlanningContextBuilder::new().with_history(history);
        let ctx = builder.build(&Goal::new("open something"));
        let compressor = ContextCompressor::new(2048);
        let compressed = compressor.compress(&ctx);
        assert!(compressed.contains("Goal: open something"));
        assert!(compressed.contains("History:"));
    }

    #[test]
    fn test_context_compressor_tight_budget() {
        let ctx = PlanningContextBuilder::new().build(&Goal::new("test"));
        let compressor = ContextCompressor::new(16);
        let compressed = compressor.compress(&ctx);
        // Tight budget should still include at least the goal.
        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_context_compressor_device_state() {
        let mut ctx = PlanningContextBuilder::new().build(&Goal::new("test"));
        ctx.device_telemetry = Some(DeviceTelemetry {
            battery_level: Some(85),
            is_charging: Some(true),
            wifi_enabled: Some(true),
            bluetooth_enabled: Some(false),
            last_updated: Some(1000),
        });
        ctx.network_state = Some(NetworkState {
            is_online: Some(true),
            network_type: Some("wifi".into()),
            last_updated: Some(1000),
        });
        let compressor = ContextCompressor::new(2048);
        let compressed = compressor.compress(&ctx);
        assert!(compressed.contains("battery=85%"));
        assert!(compressed.contains("online"));
    }

    #[test]
    fn test_context_compressor_truncation() {
        let ctx = PlanningContextBuilder::new().build(&Goal::new("test"));
        let compressor = ContextCompressor::new(4); // tiny budget
        let compressed = compressor.compress(&ctx);
        // Should not panic; returns truncated or empty.
        assert!(compressed.len() <= 100);
    }

    #[test]
    fn test_capability_filter_all_available() {
        let ctx = PlanningContextBuilder::new().build(&Goal::new("test"));
        let caps = CapabilityFilter::filter_capabilities_for_context(&ctx);
        assert!(caps.contains(&Capability::DeviceControl));
        assert!(caps.contains(&Capability::AutomationWorkflow));
    }

    #[test]
    fn test_capability_filter_offline_restricts_memory() {
        let mut ctx = PlanningContextBuilder::new().build(&Goal::new("test"));
        ctx.network_state = Some(NetworkState {
            is_online: Some(false),
            network_type: None,
            last_updated: Some(1000),
        });
        let caps = CapabilityFilter::filter_capabilities_for_context(&ctx);
        assert!(!caps.contains(&Capability::MemoryQuery));
        assert!(!caps.contains(&Capability::MemoryStore));
    }

    #[test]
    fn test_capability_filter_screen_requires_world() {
        let ctx = PlanningContextBuilder::new().build(&Goal::new("test"));
        let caps = CapabilityFilter::filter_capabilities_for_context(&ctx);
        // No world_snapshot and no screen_summary → screen capabilities filtered.
        assert!(!caps.contains(&Capability::ScreenCapture));
        assert!(!caps.contains(&Capability::ScreenGrounding));
        assert!(!caps.contains(&Capability::Ocr));
    }

    #[test]
    fn test_planning_context_timestamp() {
        let ctx = PlanningContextBuilder::new().build(&Goal::new("test"));
        assert!(ctx.timestamp > 0);
    }

    #[test]
    fn test_history_summarizer_extract_app() {
        assert_eq!(
            HistorySummarizer::extract_app_name("open chrome"),
            Some("chrome".to_string())
        );
        assert_eq!(
            HistorySummarizer::extract_app_name("launch settings"),
            Some("settings".to_string())
        );
        assert_eq!(HistorySummarizer::extract_app_name("set brightness"), None);
    }

    #[test]
    fn test_context_compressor_preferences() {
        let mut prefs = HashMap::new();
        prefs.insert("theme".to_string(), "dark".to_string());
        let builder = PlanningContextBuilder::new().with_user_preferences(prefs);
        let ctx = builder.build(&Goal::new("test"));
        let compressor = ContextCompressor::new(2048);
        let compressed = compressor.compress(&ctx);
        assert!(compressed.contains("theme=dark"));
    }

    #[test]
    fn test_builder_graceful_missing_components() {
        let builder = PlanningContextBuilder::new();
        let ctx = builder.build(&Goal::new("test"));
        // Should not panic with any combination of missing sources.
        assert!(ctx.world_snapshot.is_none());
        assert!(ctx.active_app.is_none());
        assert!(ctx.device_telemetry.is_none());
        assert!(ctx.network_state.is_none());
        assert!(ctx.execution_history.is_none());
    }

    #[test]
    fn test_context_compressor_priority_order() {
        let mut ctx = PlanningContextBuilder::new().build(&Goal::new("critical goal"));
        ctx.active_app = Some("chrome".into());
        ctx.screen_summary = Some("login page".into());

        let compressor = ContextCompressor::new(2048);
        let compressed = compressor.compress(&ctx);

        // Goal should always be first.
        assert!(
            compressed.starts_with("Goal: critical goal")
                || compressed.starts_with("Goal: critical goal\n")
        );
        assert!(compressed.contains("App: chrome"));
        assert!(compressed.contains("Screen: login page"));
    }

    #[test]
    fn test_history_summarizer_frequent_actions() {
        let mut records = Vec::new();
        for i in 0..10 {
            records.push(ExecutionRecord {
                execution_id: format!("e{}", i),
                workflow_id: "wf".into(),
                workflow_name: "open app".into(),
                status: ExecutionStatus::Completed,
                started_at: i * 100,
                duration_ms: 100,
                steps_succeeded: 1,
                steps_failed: 0,
                steps_total: 1,
                error: None,
            });
        }
        for i in 0..3 {
            records.push(ExecutionRecord {
                execution_id: format!("e{}", i + 100),
                workflow_id: "wf2".into(),
                workflow_name: "search web".into(),
                status: ExecutionStatus::Completed,
                started_at: i * 100,
                duration_ms: 200,
                steps_succeeded: 1,
                steps_failed: 0,
                steps_total: 1,
                error: None,
            });
        }
        let summary = HistorySummarizer::summarize(&records);
        assert_eq!(summary.frequent_actions[0], "open app");
    }

    #[test]
    fn test_compressor_format_device_state_all_none() {
        let compressor = ContextCompressor::new(1024);
        let text = compressor.format_device_state(&None, &None);
        assert!(text.is_empty());
    }

    #[test]
    fn test_compressor_format_device_state_partial() {
        let compressor = ContextCompressor::new(1024);
        let telemetry = Some(DeviceTelemetry {
            battery_level: Some(50),
            is_charging: None,
            wifi_enabled: Some(true),
            bluetooth_enabled: None,
            last_updated: None,
        });
        let text = compressor.format_device_state(&telemetry, &None);
        assert!(text.contains("battery=50%"));
        assert!(text.contains("wifi=on"));
    }

    #[test]
    fn test_capability_filter_available_capabilities_offline() {
        let mut ctx = PlanningContextBuilder::new().build(&Goal::new("test"));
        ctx.network_state = Some(NetworkState {
            is_online: Some(false),
            network_type: None,
            last_updated: Some(1000),
        });
        let all = vec![
            Capability::MemoryQuery,
            Capability::AiInference,
            Capability::DeviceControl,
        ];
        let available = CapabilityFilter::available_capabilities(&ctx, &all);
        assert!(!available.contains(&Capability::MemoryQuery));
        assert!(available.contains(&Capability::AiInference));
        assert!(available.contains(&Capability::DeviceControl));
    }
}
