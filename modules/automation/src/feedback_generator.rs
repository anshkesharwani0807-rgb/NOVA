use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeedbackLevel {
    Debug,
    Info,
    Success,
    Warning,
    Error,
}

impl FeedbackLevel {
    pub fn label(&self) -> &'static str {
        match self {
            FeedbackLevel::Debug => "DEBUG",
            FeedbackLevel::Info => "INFO",
            FeedbackLevel::Success => "SUCCESS",
            FeedbackLevel::Warning => "WARNING",
            FeedbackLevel::Error => "ERROR",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            FeedbackLevel::Debug => "🔍",
            FeedbackLevel::Info => "ℹ️",
            FeedbackLevel::Success => "✅",
            FeedbackLevel::Warning => "⚠️",
            FeedbackLevel::Error => "❌",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum FeedbackStyle {
    Concise,
    #[default]
    Normal,
    Detailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackConfig {
    pub default_style: FeedbackStyle,
    pub enable_emoji: bool,
    pub show_timestamps: bool,
    pub max_history: usize,
    pub structured_summary: bool,
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            default_style: FeedbackStyle::Normal,
            enable_emoji: true,
            show_timestamps: false,
            max_history: 100,
            structured_summary: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FeedbackContext {
    pub execution_id: Option<String>,
    pub goal: Option<String>,
    pub current_step: Option<String>,
    pub step_index: usize,
    pub total_steps: usize,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeedbackEvent {
    GoalAccepted {
        goal: String,
        confidence: f32,
    },
    PlanningStarted {
        goal: String,
    },
    PlanningFinished {
        goal: String,
        steps: usize,
        success: bool,
    },
    ExecutionStarted {
        execution_id: String,
        goal: String,
        total_steps: usize,
    },
    StepStarted {
        execution_id: String,
        goal: String,
        step: String,
        index: usize,
        total: usize,
    },
    StepCompleted {
        execution_id: String,
        goal: String,
        step: String,
        index: usize,
        total: usize,
        success: bool,
    },
    VerificationPassed {
        execution_id: String,
        details: String,
    },
    VerificationFailed {
        execution_id: String,
        details: String,
        expected: String,
        actual: String,
    },
    RecoveryStarted {
        execution_id: String,
        reason: String,
        attempt: u32,
    },
    RecoveryCompleted {
        execution_id: String,
        success: bool,
        result: String,
    },
    ExecutionCompleted {
        execution_id: String,
        goal: String,
        steps_succeeded: usize,
        steps_failed: usize,
        duration_ms: u64,
    },
    ExecutionCancelled {
        execution_id: String,
        goal: String,
        reason: String,
        steps_completed: usize,
    },
    ExecutionFailed {
        execution_id: String,
        goal: String,
        error: String,
        steps_succeeded: usize,
        steps_failed: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackMessage {
    pub level: FeedbackLevel,
    pub event: String,
    pub message: String,
    pub detail: String,
    pub timestamp_ms: i64,
    pub style: FeedbackStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackProgress {
    pub execution_id: String,
    pub goal: String,
    pub current_step: usize,
    pub total_steps: usize,
    pub percentage: f32,
    pub elapsed_ms: u64,
    pub message: String,
    pub steps_completed: Vec<String>,
    pub steps_remaining: Vec<String>,
    pub level: FeedbackLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackSummary {
    pub execution_id: String,
    pub goal: String,
    pub status: String,
    pub total_steps: usize,
    pub steps_succeeded: usize,
    pub steps_failed: usize,
    pub duration_ms: u64,
    pub feedback_count: usize,
    pub messages: Vec<FeedbackMessage>,
    pub level: FeedbackLevel,
}

impl FeedbackSummary {
    pub fn success_rate(&self) -> f32 {
        if self.total_steps == 0 {
            return 1.0;
        }
        self.steps_succeeded as f32 / self.total_steps as f32
    }

    pub fn is_success(&self) -> bool {
        self.steps_failed == 0 && self.status == "completed"
    }
}

#[derive(Debug, Clone)]
pub struct FeedbackTemplate {
    pub name: String,
    pub template: String,
}

#[derive(Debug, Clone, Default)]
pub struct FeedbackMetricsSnapshot {
    pub total_messages: u64,
    pub average_generation_time_ms: u64,
    pub debug_count: u64,
    pub info_count: u64,
    pub success_count: u64,
    pub warning_count: u64,
    pub error_count: u64,
    pub success_failure_ratio: f64,
    pub total_success_events: u64,
    pub total_failure_events: u64,
}

#[derive(Debug)]
pub struct FeedbackMetrics {
    total_messages: AtomicU64,
    total_generation_time_ms: AtomicU64,
    generation_count: AtomicU64,
    debug_count: AtomicU64,
    info_count: AtomicU64,
    success_count: AtomicU64,
    warning_count: AtomicU64,
    error_count: AtomicU64,
    success_events: AtomicU64,
    failure_events: AtomicU64,
}

impl FeedbackMetrics {
    pub fn new() -> Self {
        Self {
            total_messages: AtomicU64::new(0),
            total_generation_time_ms: AtomicU64::new(0),
            generation_count: AtomicU64::new(0),
            debug_count: AtomicU64::new(0),
            info_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            warning_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            success_events: AtomicU64::new(0),
            failure_events: AtomicU64::new(0),
        }
    }

    fn record(&self, level: &FeedbackLevel, elapsed_ms: u64) {
        self.total_messages.fetch_add(1, Ordering::SeqCst);
        self.total_generation_time_ms
            .fetch_add(elapsed_ms, Ordering::SeqCst);
        self.generation_count.fetch_add(1, Ordering::SeqCst);
        match level {
            FeedbackLevel::Debug => self.debug_count.fetch_add(1, Ordering::SeqCst),
            FeedbackLevel::Info => self.info_count.fetch_add(1, Ordering::SeqCst),
            FeedbackLevel::Success => self.success_count.fetch_add(1, Ordering::SeqCst),
            FeedbackLevel::Warning => self.warning_count.fetch_add(1, Ordering::SeqCst),
            FeedbackLevel::Error => self.error_count.fetch_add(1, Ordering::SeqCst),
        };
    }

    fn record_event(&self, is_success: bool) {
        if is_success {
            self.success_events.fetch_add(1, Ordering::SeqCst);
        } else {
            self.failure_events.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub fn snapshot(&self) -> FeedbackMetricsSnapshot {
        let total = self.total_messages.load(Ordering::SeqCst);
        let gen_count = self.generation_count.load(Ordering::SeqCst);
        let total_time = self.total_generation_time_ms.load(Ordering::SeqCst);
        let avg = total_time.checked_div(gen_count).unwrap_or(0);
        let successes = self.success_events.load(Ordering::SeqCst);
        let failures = self.failure_events.load(Ordering::SeqCst);
        let ratio = if failures == 0 {
            successes as f64
        } else if successes == 0 {
            0.0
        } else {
            successes as f64 / failures as f64
        };
        FeedbackMetricsSnapshot {
            total_messages: total,
            average_generation_time_ms: avg,
            debug_count: self.debug_count.load(Ordering::SeqCst),
            info_count: self.info_count.load(Ordering::SeqCst),
            success_count: self.success_count.load(Ordering::SeqCst),
            warning_count: self.warning_count.load(Ordering::SeqCst),
            error_count: self.error_count.load(Ordering::SeqCst),
            success_failure_ratio: ratio,
            total_success_events: successes,
            total_failure_events: failures,
        }
    }
}

impl Default for FeedbackMetrics {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FeedbackGenerator {
    config: Arc<RwLock<FeedbackConfig>>,
    metrics: Arc<FeedbackMetrics>,
    history: Arc<RwLock<Vec<FeedbackMessage>>>,
}

impl FeedbackGenerator {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(FeedbackConfig::default())),
            metrics: Arc::new(FeedbackMetrics::new()),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn with_config(config: FeedbackConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            metrics: Arc::new(FeedbackMetrics::new()),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn config(&self) -> FeedbackConfig {
        self.config.read().clone()
    }

    pub fn set_config(&self, config: FeedbackConfig) {
        *self.config.write() = config;
    }

    pub fn metrics(&self) -> FeedbackMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub fn history(&self) -> Vec<FeedbackMessage> {
        self.history.read().clone()
    }

    pub fn clear_history(&self) {
        self.history.write().clear();
    }

    fn push_history(&self, msg: &FeedbackMessage) {
        let mut hist = self.history.write();
        hist.push(msg.clone());
        let max = self.config.read().max_history;
        while hist.len() > max {
            hist.remove(0);
        }
    }

    pub fn generate(&self, event: &FeedbackEvent) -> FeedbackMessage {
        let start = Instant::now();
        let (level, event_name, message, detail) = self.format_event(event);
        let msg = FeedbackMessage {
            level,
            event: event_name,
            message,
            detail,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            style: self.config.read().default_style,
        };
        let elapsed = start.elapsed().as_millis() as u64;
        self.metrics.record(&level, elapsed);
        let is_success = matches!(
            event,
            FeedbackEvent::GoalAccepted { .. }
                | FeedbackEvent::PlanningFinished { success: true, .. }
                | FeedbackEvent::StepCompleted { success: true, .. }
                | FeedbackEvent::VerificationPassed { .. }
                | FeedbackEvent::RecoveryCompleted { success: true, .. }
                | FeedbackEvent::ExecutionCompleted { .. }
        );
        self.metrics.record_event(is_success);
        self.push_history(&msg);
        msg
    }

    fn format_event(&self, event: &FeedbackEvent) -> (FeedbackLevel, String, String, String) {
        match event {
            FeedbackEvent::GoalAccepted { goal, confidence } => {
                let pct = (confidence * 100.0) as u32;
                (
                    FeedbackLevel::Info,
                    "goal_accepted".into(),
                    format!("Goal accepted: {}", goal),
                    format!("Confidence: {}%", pct),
                )
            }
            FeedbackEvent::PlanningStarted { goal } => (
                FeedbackLevel::Info,
                "planning_started".into(),
                format!("Planning started for: {}", goal),
                "Decomposing goal into actionable steps".into(),
            ),
            FeedbackEvent::PlanningFinished {
                goal,
                steps,
                success,
            } => {
                if *success {
                    (
                        FeedbackLevel::Success,
                        "planning_finished".into(),
                        format!("Planning complete for: {}", goal),
                        format!("Plan created with {} step(s)", steps),
                    )
                } else {
                    (
                        FeedbackLevel::Warning,
                        "planning_finished".into(),
                        format!("Planning finished with issues for: {}", goal),
                        "Plan may be incomplete or requires refinement".into(),
                    )
                }
            }
            FeedbackEvent::ExecutionStarted {
                execution_id,
                goal,
                total_steps,
            } => (
                FeedbackLevel::Info,
                "execution_started".into(),
                format!("Execution started: {} — {}", execution_id, goal),
                format!("Total steps: {}", total_steps),
            ),
            FeedbackEvent::StepStarted {
                execution_id: _,
                goal: _,
                step,
                index,
                total,
            } => (
                FeedbackLevel::Debug,
                "step_started".into(),
                format!("Step {}/{}: {}", index + 1, total, step),
                "Executing step...".into(),
            ),
            FeedbackEvent::StepCompleted {
                execution_id: _,
                goal: _,
                step,
                index,
                total,
                success,
            } => {
                if *success {
                    (
                        FeedbackLevel::Success,
                        "step_completed".into(),
                        format!("Step {}/{} completed: {}", index + 1, total, step),
                        "Step executed successfully".into(),
                    )
                } else {
                    (
                        FeedbackLevel::Error,
                        "step_completed".into(),
                        format!("Step {}/{} failed: {}", index + 1, total, step),
                        "Step execution encountered an error".into(),
                    )
                }
            }
            FeedbackEvent::VerificationPassed {
                execution_id: _,
                details,
            } => (
                FeedbackLevel::Success,
                "verification_passed".into(),
                "Verification passed".into(),
                details.clone(),
            ),
            FeedbackEvent::VerificationFailed {
                execution_id: _,
                details,
                expected,
                actual,
            } => (
                FeedbackLevel::Error,
                "verification_failed".into(),
                format!("Verification failed: {}", details),
                format!("Expected: {} | Actual: {}", expected, actual),
            ),
            FeedbackEvent::RecoveryStarted {
                execution_id: _,
                reason,
                attempt,
            } => (
                FeedbackLevel::Warning,
                "recovery_started".into(),
                format!("Recovery attempt #{}", attempt),
                format!("Reason: {}", reason),
            ),
            FeedbackEvent::RecoveryCompleted {
                execution_id: _,
                success,
                result,
            } => {
                if *success {
                    (
                        FeedbackLevel::Success,
                        "recovery_completed".into(),
                        "Recovery completed".into(),
                        format!("Result: {}", result),
                    )
                } else {
                    (
                        FeedbackLevel::Error,
                        "recovery_completed".into(),
                        "Recovery failed".into(),
                        format!("Result: {}", result),
                    )
                }
            }
            FeedbackEvent::ExecutionCompleted {
                execution_id,
                goal,
                steps_succeeded,
                steps_failed,
                duration_ms,
            } => {
                let secs = *duration_ms as f64 / 1000.0;
                (
                    FeedbackLevel::Success,
                    "execution_completed".into(),
                    format!("Execution completed: {} — {}", execution_id, goal),
                    format!(
                        "{} succeeded, {} failed, {:.1}s elapsed",
                        steps_succeeded, steps_failed, secs
                    ),
                )
            }
            FeedbackEvent::ExecutionCancelled {
                execution_id,
                goal,
                reason,
                steps_completed,
            } => (
                FeedbackLevel::Warning,
                "execution_cancelled".into(),
                format!("Execution cancelled: {} — {}", execution_id, goal),
                format!("Reason: {} ({} step(s) completed)", reason, steps_completed),
            ),
            FeedbackEvent::ExecutionFailed {
                execution_id,
                goal,
                error,
                steps_succeeded,
                steps_failed,
            } => (
                FeedbackLevel::Error,
                "execution_failed".into(),
                format!("Execution failed: {} — {}", execution_id, goal),
                format!(
                    "Error: {} ({} succeeded, {} failed)",
                    error, steps_succeeded, steps_failed
                ),
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn generate_progress(
        &self,
        execution_id: &str,
        goal: &str,
        current_step: usize,
        total_steps: usize,
        elapsed_ms: u64,
        steps_completed: Vec<String>,
        steps_remaining: Vec<String>,
    ) -> FeedbackProgress {
        let percentage = if total_steps == 0 {
            100.0
        } else {
            (current_step as f32 / total_steps as f32) * 100.0
        };
        let level = if percentage >= 100.0 {
            FeedbackLevel::Success
        } else {
            FeedbackLevel::Info
        };
        let message = format!(
            "Progress: {:.0}% — step {}/{} ({:.1}s elapsed)",
            percentage,
            current_step.min(total_steps),
            total_steps,
            elapsed_ms as f64 / 1000.0
        );
        FeedbackProgress {
            execution_id: execution_id.to_string(),
            goal: goal.to_string(),
            current_step: current_step.min(total_steps),
            total_steps,
            percentage,
            elapsed_ms,
            message,
            steps_completed,
            steps_remaining,
            level,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn generate_summary(
        &self,
        execution_id: &str,
        goal: &str,
        status: &str,
        total_steps: usize,
        steps_succeeded: usize,
        steps_failed: usize,
        duration_ms: u64,
    ) -> FeedbackSummary {
        let level = match status {
            "completed" => FeedbackLevel::Success,
            "cancelled" => FeedbackLevel::Warning,
            _ => FeedbackLevel::Error,
        };
        let history = self.history.read().clone();
        FeedbackSummary {
            execution_id: execution_id.to_string(),
            goal: goal.to_string(),
            status: status.to_string(),
            total_steps,
            steps_succeeded,
            steps_failed,
            duration_ms,
            feedback_count: history.len(),
            messages: history,
            level,
        }
    }

    pub fn generate_completion(
        &self,
        execution_id: &str,
        goal: &str,
        steps_succeeded: usize,
        steps_failed: usize,
        duration_ms: u64,
    ) -> FeedbackMessage {
        let event = FeedbackEvent::ExecutionCompleted {
            execution_id: execution_id.to_string(),
            goal: goal.to_string(),
            steps_succeeded,
            steps_failed,
            duration_ms,
        };
        self.generate(&event)
    }

    pub fn generate_failure(
        &self,
        execution_id: &str,
        goal: &str,
        error: &str,
        steps_succeeded: usize,
        steps_failed: usize,
    ) -> FeedbackMessage {
        let event = FeedbackEvent::ExecutionFailed {
            execution_id: execution_id.to_string(),
            goal: goal.to_string(),
            error: error.to_string(),
            steps_succeeded,
            steps_failed,
        };
        self.generate(&event)
    }

    pub fn generate_recovery(
        &self,
        execution_id: &str,
        reason: &str,
        attempt: u32,
    ) -> FeedbackMessage {
        let event = FeedbackEvent::RecoveryStarted {
            execution_id: execution_id.to_string(),
            reason: reason.to_string(),
            attempt,
        };
        self.generate(&event)
    }

    pub fn format_plain(&self, msg: &FeedbackMessage) -> String {
        let config = self.config.read();
        let emoji_prefix = if config.enable_emoji {
            format!("{} ", msg.level.emoji())
        } else {
            String::new()
        };
        let timestamp = if config.show_timestamps {
            format!("[{}] ", msg.timestamp_ms)
        } else {
            String::new()
        };
        match msg.style {
            FeedbackStyle::Concise => {
                format!("{}{}{}", timestamp, emoji_prefix, msg.message)
            }
            FeedbackStyle::Normal => {
                format!(
                    "{}{}[{}] {}",
                    timestamp,
                    emoji_prefix,
                    msg.level.label(),
                    msg.message
                )
            }
            FeedbackStyle::Detailed => {
                format!(
                    "{}{}[{}] {}\n  → {}",
                    timestamp,
                    emoji_prefix,
                    msg.level.label(),
                    msg.message,
                    msg.detail
                )
            }
        }
    }

    pub fn format_markdown(&self, msg: &FeedbackMessage) -> String {
        let config = self.config.read();
        let emoji = if config.enable_emoji {
            format!("{} ", msg.level.emoji())
        } else {
            String::new()
        };
        let timestamp = if config.show_timestamps {
            format!("_`{}`_ ", msg.timestamp_ms)
        } else {
            String::new()
        };
        let level_badge = match msg.level {
            FeedbackLevel::Debug => "`DEBUG`",
            FeedbackLevel::Info => "`INFO`",
            FeedbackLevel::Success => "`SUCCESS`",
            FeedbackLevel::Warning => "`WARN`",
            FeedbackLevel::Error => "`ERROR`",
        };
        match msg.style {
            FeedbackStyle::Concise => {
                format!("{}{}{}", timestamp, emoji, msg.message)
            }
            FeedbackStyle::Normal => {
                format!("{}{}{} {}", timestamp, emoji, level_badge, msg.message)
            }
            FeedbackStyle::Detailed => {
                format!(
                    "{}{}{} {}\n> {}",
                    timestamp, emoji, level_badge, msg.message, msg.detail
                )
            }
        }
    }

    pub fn format_summary_plain(&self, summary: &FeedbackSummary) -> String {
        let emoji = match summary.level {
            FeedbackLevel::Success => "✅ ",
            FeedbackLevel::Warning => "⚠️ ",
            FeedbackLevel::Error => "❌ ",
            _ => "",
        };
        let dur_secs = summary.duration_ms as f64 / 1000.0;
        let lines = [
            format!(
                "{}Execution Summary: {} — {}",
                emoji, summary.execution_id, summary.goal
            ),
            format!("  Status: {}", summary.status),
            format!(
                "  Steps: {}/{} succeeded, {}/{} failed",
                summary.steps_succeeded,
                summary.total_steps,
                summary.steps_failed,
                summary.total_steps
            ),
            format!("  Duration: {:.1}s", dur_secs),
            format!("  Success rate: {:.0}%", summary.success_rate() * 100.0),
            format!("  Feedback messages generated: {}", summary.feedback_count),
        ];
        lines.join("\n")
    }

    pub fn format_summary_markdown(&self, summary: &FeedbackSummary) -> String {
        let dur_secs = summary.duration_ms as f64 / 1000.0;
        let status_badge = match summary.level {
            FeedbackLevel::Success => "🟢 **Completed**",
            FeedbackLevel::Warning => "🟡 **Cancelled**",
            FeedbackLevel::Error => "🔴 **Failed**",
            _ => "⚪ **Unknown**",
        };
        let lines = [
            format!(
                "## Execution Summary: {} — {}",
                summary.execution_id, summary.goal
            ),
            format!("- **Status**: {}", status_badge),
            format!(
                "- **Steps**: `{}/{}` succeeded, `{}/{}` failed",
                summary.steps_succeeded,
                summary.total_steps,
                summary.steps_failed,
                summary.total_steps
            ),
            format!("- **Duration**: `{:.1}s`", dur_secs),
            format!(
                "- **Success rate**: `{:.0}%`",
                summary.success_rate() * 100.0
            ),
            format!("- **Feedback messages**: `{}`", summary.feedback_count),
        ];
        lines.join("\n")
    }
}

impl Default for FeedbackGenerator {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for FeedbackGenerator {}
unsafe impl Sync for FeedbackGenerator {}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_generator() -> FeedbackGenerator {
        FeedbackGenerator::new()
    }

    fn make_config() -> FeedbackConfig {
        FeedbackConfig {
            default_style: FeedbackStyle::Normal,
            enable_emoji: true,
            show_timestamps: true,
            max_history: 50,
            structured_summary: true,
        }
    }

    #[test]
    fn test_level_labels() {
        assert_eq!(FeedbackLevel::Debug.label(), "DEBUG");
        assert_eq!(FeedbackLevel::Info.label(), "INFO");
        assert_eq!(FeedbackLevel::Success.label(), "SUCCESS");
        assert_eq!(FeedbackLevel::Warning.label(), "WARNING");
        assert_eq!(FeedbackLevel::Error.label(), "ERROR");
    }

    #[test]
    fn test_level_emoji() {
        assert!(!FeedbackLevel::Debug.emoji().is_empty());
        assert!(!FeedbackLevel::Info.emoji().is_empty());
        assert!(!FeedbackLevel::Success.emoji().is_empty());
        assert!(!FeedbackLevel::Warning.emoji().is_empty());
        assert!(!FeedbackLevel::Error.emoji().is_empty());
    }

    #[test]
    fn test_config_default() {
        let cfg = FeedbackConfig::default();
        assert_eq!(cfg.default_style, FeedbackStyle::Normal);
        assert!(cfg.enable_emoji);
        assert_eq!(cfg.max_history, 100);
    }

    #[test]
    fn test_generator_create() {
        let gen = make_generator();
        let metrics = gen.metrics();
        assert_eq!(metrics.total_messages, 0);
    }

    #[test]
    fn test_generator_with_config() {
        let cfg = make_config();
        let gen = FeedbackGenerator::with_config(cfg.clone());
        let retrieved = gen.config();
        assert_eq!(retrieved.default_style, cfg.default_style);
        assert_eq!(retrieved.max_history, cfg.max_history);
    }

    #[test]
    fn test_set_config() {
        let gen = make_generator();
        let cfg = make_config();
        gen.set_config(cfg.clone());
        let retrieved = gen.config();
        assert_eq!(retrieved.enable_emoji, cfg.enable_emoji);
    }

    #[test]
    fn test_generate_goal_accepted() {
        let gen = make_generator();
        let event = FeedbackEvent::GoalAccepted {
            goal: "open chrome".into(),
            confidence: 0.95,
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Info);
        assert_eq!(msg.event, "goal_accepted");
        assert!(msg.message.contains("open chrome"));
        assert!(msg.detail.contains("95%"));
    }

    #[test]
    fn test_generate_planning_started() {
        let gen = make_generator();
        let event = FeedbackEvent::PlanningStarted {
            goal: "set brightness to 50".into(),
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Info);
        assert_eq!(msg.event, "planning_started");
    }

    #[test]
    fn test_generate_planning_finished_success() {
        let gen = make_generator();
        let event = FeedbackEvent::PlanningFinished {
            goal: "open chrome".into(),
            steps: 3,
            success: true,
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Success);
        assert!(msg.detail.contains("3 step(s)"));
    }

    #[test]
    fn test_generate_planning_finished_warning() {
        let gen = make_generator();
        let event = FeedbackEvent::PlanningFinished {
            goal: "complex task".into(),
            steps: 0,
            success: false,
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Warning);
    }

    #[test]
    fn test_generate_execution_started() {
        let gen = make_generator();
        let event = FeedbackEvent::ExecutionStarted {
            execution_id: "exec-1".into(),
            goal: "open chrome".into(),
            total_steps: 5,
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Info);
        assert_eq!(msg.event, "execution_started");
        assert!(msg.message.contains("exec-1"));
    }

    #[test]
    fn test_generate_step_started() {
        let gen = make_generator();
        let event = FeedbackEvent::StepStarted {
            execution_id: "exec-1".into(),
            goal: "open chrome".into(),
            step: "launch browser".into(),
            index: 0,
            total: 3,
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Debug);
        assert!(msg.message.contains("Step 1/3"));
    }

    #[test]
    fn test_generate_step_completed_success() {
        let gen = make_generator();
        let event = FeedbackEvent::StepCompleted {
            execution_id: "exec-1".into(),
            goal: "open chrome".into(),
            step: "launch browser".into(),
            index: 0,
            total: 3,
            success: true,
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Success);
    }

    #[test]
    fn test_generate_step_completed_failed() {
        let gen = make_generator();
        let event = FeedbackEvent::StepCompleted {
            execution_id: "exec-1".into(),
            goal: "open chrome".into(),
            step: "launch browser".into(),
            index: 0,
            total: 3,
            success: false,
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Error);
    }

    #[test]
    fn test_generate_verification_passed() {
        let gen = make_generator();
        let event = FeedbackEvent::VerificationPassed {
            execution_id: "exec-1".into(),
            details: "brightness set to 50".into(),
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Success);
        assert_eq!(msg.event, "verification_passed");
    }

    #[test]
    fn test_generate_verification_failed() {
        let gen = make_generator();
        let event = FeedbackEvent::VerificationFailed {
            execution_id: "exec-1".into(),
            details: "brightness mismatch".into(),
            expected: "50".into(),
            actual: "75".into(),
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Error);
        assert!(msg.detail.contains("Expected: 50"));
        assert!(msg.detail.contains("Actual: 75"));
    }

    #[test]
    fn test_generate_recovery_started() {
        let gen = make_generator();
        let event = FeedbackEvent::RecoveryStarted {
            execution_id: "exec-1".into(),
            reason: "step failed".into(),
            attempt: 1,
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Warning);
        assert!(msg.message.contains("attempt #1"));
    }

    #[test]
    fn test_generate_recovery_completed_success() {
        let gen = make_generator();
        let event = FeedbackEvent::RecoveryCompleted {
            execution_id: "exec-1".into(),
            success: true,
            result: "recovered by retry".into(),
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Success);
    }

    #[test]
    fn test_generate_recovery_completed_failed() {
        let gen = make_generator();
        let event = FeedbackEvent::RecoveryCompleted {
            execution_id: "exec-1".into(),
            success: false,
            result: "all retries exhausted".into(),
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Error);
    }

    #[test]
    fn test_generate_execution_completed() {
        let gen = make_generator();
        let event = FeedbackEvent::ExecutionCompleted {
            execution_id: "exec-1".into(),
            goal: "open chrome".into(),
            steps_succeeded: 3,
            steps_failed: 0,
            duration_ms: 1500,
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Success);
        assert!(msg.detail.contains("3 succeeded"));
        assert!(msg.detail.contains("0 failed"));
    }

    #[test]
    fn test_generate_execution_cancelled() {
        let gen = make_generator();
        let event = FeedbackEvent::ExecutionCancelled {
            execution_id: "exec-1".into(),
            goal: "open chrome".into(),
            reason: "user requested".into(),
            steps_completed: 2,
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Warning);
        assert!(msg.detail.contains("user requested"));
    }

    #[test]
    fn test_generate_execution_failed() {
        let gen = make_generator();
        let event = FeedbackEvent::ExecutionFailed {
            execution_id: "exec-1".into(),
            goal: "open chrome".into(),
            error: "app not found".into(),
            steps_succeeded: 1,
            steps_failed: 2,
        };
        let msg = gen.generate(&event);
        assert_eq!(msg.level, FeedbackLevel::Error);
        assert!(msg.detail.contains("app not found"));
    }

    #[test]
    fn test_generate_progress() {
        let gen = make_generator();
        let progress = gen.generate_progress(
            "exec-1",
            "open chrome",
            2,
            5,
            1200,
            vec!["step1".into(), "step2".into()],
            vec!["step3".into(), "step4".into(), "step5".into()],
        );
        assert_eq!(progress.execution_id, "exec-1");
        assert_eq!(progress.current_step, 2);
        assert_eq!(progress.total_steps, 5);
        assert!((progress.percentage - 40.0).abs() < 0.01);
        assert_eq!(progress.steps_completed.len(), 2);
        assert_eq!(progress.steps_remaining.len(), 3);
    }

    #[test]
    fn test_generate_progress_complete() {
        let gen = make_generator();
        let progress = gen.generate_progress("exec-1", "test", 5, 5, 500, vec![], vec![]);
        assert!(progress.percentage >= 100.0);
        assert_eq!(progress.level, FeedbackLevel::Success);
    }

    #[test]
    fn test_generate_progress_zero_steps() {
        let gen = make_generator();
        let progress = gen.generate_progress("exec-1", "test", 0, 0, 0, vec![], vec![]);
        assert!((progress.percentage - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_generate_summary_completed() {
        let gen = make_generator();
        let summary = gen.generate_summary("exec-1", "open chrome", "completed", 5, 5, 0, 2000);
        assert_eq!(summary.level, FeedbackLevel::Success);
        assert!(summary.is_success());
        assert!((summary.success_rate() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_generate_summary_cancelled() {
        let gen = make_generator();
        let summary = gen.generate_summary("exec-1", "open chrome", "cancelled", 5, 3, 2, 1000);
        assert_eq!(summary.level, FeedbackLevel::Warning);
        assert!(!summary.is_success());
    }

    #[test]
    fn test_generate_summary_failed() {
        let gen = make_generator();
        let summary = gen.generate_summary("exec-1", "open chrome", "failed", 5, 1, 4, 3000);
        assert_eq!(summary.level, FeedbackLevel::Error);
        let rate = summary.success_rate();
        assert!((rate - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_generate_completion_helper() {
        let gen = make_generator();
        let msg = gen.generate_completion("exec-1", "open chrome", 3, 0, 1000);
        assert_eq!(msg.level, FeedbackLevel::Success);
        assert_eq!(msg.event, "execution_completed");
    }

    #[test]
    fn test_generate_failure_helper() {
        let gen = make_generator();
        let msg = gen.generate_failure("exec-1", "open chrome", "app not found", 1, 2);
        assert_eq!(msg.level, FeedbackLevel::Error);
        assert_eq!(msg.event, "execution_failed");
    }

    #[test]
    fn test_generate_recovery_helper() {
        let gen = make_generator();
        let msg = gen.generate_recovery("exec-1", "step failed", 2);
        assert_eq!(msg.level, FeedbackLevel::Warning);
        assert!(msg.message.contains("attempt #2"));
    }

    #[test]
    fn test_format_plain_concise() {
        let gen = make_generator();
        let mut cfg = gen.config();
        cfg.default_style = FeedbackStyle::Concise;
        cfg.enable_emoji = false;
        cfg.show_timestamps = false;
        gen.set_config(cfg);
        let event = FeedbackEvent::GoalAccepted {
            goal: "test".into(),
            confidence: 1.0,
        };
        let msg = gen.generate(&event);
        let plain = gen.format_plain(&msg);
        assert!(plain.contains("Goal accepted"));
    }

    #[test]
    fn test_format_plain_normal() {
        let gen = make_generator();
        let mut cfg = gen.config();
        cfg.default_style = FeedbackStyle::Normal;
        cfg.enable_emoji = false;
        cfg.show_timestamps = false;
        gen.set_config(cfg);
        let event = FeedbackEvent::ExecutionCompleted {
            execution_id: "e1".into(),
            goal: "test".into(),
            steps_succeeded: 1,
            steps_failed: 0,
            duration_ms: 100,
        };
        let msg = gen.generate(&event);
        let plain = gen.format_plain(&msg);
        assert!(plain.contains("[SUCCESS]"));
    }

    #[test]
    fn test_format_plain_detailed() {
        let gen = make_generator();
        let mut cfg = gen.config();
        cfg.default_style = FeedbackStyle::Detailed;
        cfg.enable_emoji = false;
        cfg.show_timestamps = false;
        gen.set_config(cfg);
        let event = FeedbackEvent::VerificationFailed {
            execution_id: "e1".into(),
            details: "mismatch".into(),
            expected: "50".into(),
            actual: "75".into(),
        };
        let msg = gen.generate(&event);
        let plain = gen.format_plain(&msg);
        assert!(plain.contains("→"));
        assert!(plain.contains("Expected: 50"));
    }

    #[test]
    fn test_format_markdown() {
        let gen = make_generator();
        let event = FeedbackEvent::ExecutionCompleted {
            execution_id: "e1".into(),
            goal: "test".into(),
            steps_succeeded: 2,
            steps_failed: 0,
            duration_ms: 500,
        };
        let msg = gen.generate(&event);
        let md = gen.format_markdown(&msg);
        assert!(md.contains("`SUCCESS`") || md.contains("✅"));
    }

    #[test]
    fn test_format_plain_with_emoji() {
        let gen = make_generator();
        let mut cfg = gen.config();
        cfg.enable_emoji = true;
        cfg.show_timestamps = false;
        gen.set_config(cfg);
        let event = FeedbackEvent::GoalAccepted {
            goal: "test".into(),
            confidence: 1.0,
        };
        let msg = gen.generate(&event);
        let plain = gen.format_plain(&msg);
        assert!(
            plain.contains("🔍")
                || plain.contains("ℹ️")
                || plain.contains("✅")
                || plain.contains("⚠️")
                || plain.contains("❌")
        );
    }

    #[test]
    fn test_format_plain_with_timestamp() {
        let gen = make_generator();
        let mut cfg = gen.config();
        cfg.show_timestamps = true;
        cfg.enable_emoji = false;
        gen.set_config(cfg);
        let event = FeedbackEvent::GoalAccepted {
            goal: "test".into(),
            confidence: 1.0,
        };
        let msg = gen.generate(&event);
        let plain = gen.format_plain(&msg);
        assert!(plain.contains('['));
        assert!(plain.contains(']'));
    }

    #[test]
    fn test_format_summary_plain() {
        let gen = make_generator();
        let summary = gen.generate_summary("exec-1", "open chrome", "completed", 10, 9, 1, 5000);
        let text = gen.format_summary_plain(&summary);
        assert!(text.contains("Execution Summary"));
        assert!(text.contains("90%"));
    }

    #[test]
    fn test_format_summary_markdown() {
        let gen = make_generator();
        let summary = gen.generate_summary("exec-1", "open chrome", "completed", 10, 10, 0, 2500);
        let md = gen.format_summary_markdown(&summary);
        assert!(md.contains("## Execution Summary"));
        assert!(md.contains("**Completed**"));
    }

    #[test]
    fn test_metrics_after_generation() {
        let gen = make_generator();
        let event = FeedbackEvent::GoalAccepted {
            goal: "test".into(),
            confidence: 0.8,
        };
        gen.generate(&event);
        let metrics = gen.metrics();
        assert_eq!(metrics.total_messages, 1);
        assert_eq!(metrics.info_count, 1);
    }

    #[test]
    fn test_metrics_multiple_generations() {
        let gen = make_generator();
        gen.generate(&FeedbackEvent::PlanningStarted { goal: "a".into() });
        gen.generate(&FeedbackEvent::ExecutionCompleted {
            execution_id: "e1".into(),
            goal: "a".into(),
            steps_succeeded: 1,
            steps_failed: 0,
            duration_ms: 100,
        });
        gen.generate(&FeedbackEvent::ExecutionFailed {
            execution_id: "e2".into(),
            goal: "b".into(),
            error: "err".into(),
            steps_succeeded: 0,
            steps_failed: 1,
        });
        let metrics = gen.metrics();
        assert_eq!(metrics.total_messages, 3);
        assert!(metrics.average_generation_time_ms > 0 || metrics.total_messages > 0);
    }

    #[test]
    fn test_metrics_event_counts() {
        let gen = make_generator();
        gen.generate(&FeedbackEvent::ExecutionCompleted {
            execution_id: "e1".into(),
            goal: "a".into(),
            steps_succeeded: 1,
            steps_failed: 0,
            duration_ms: 100,
        });
        gen.generate(&FeedbackEvent::ExecutionFailed {
            execution_id: "e2".into(),
            goal: "b".into(),
            error: "err".into(),
            steps_succeeded: 0,
            steps_failed: 1,
        });
        gen.generate(&FeedbackEvent::RecoveryStarted {
            execution_id: "e2".into(),
            reason: "err".into(),
            attempt: 1,
        });
        let metrics = gen.metrics();
        assert_eq!(metrics.total_success_events, 1);
        assert_eq!(metrics.total_failure_events, 2);
    }

    #[test]
    fn test_history_tracking() {
        let gen = make_generator();
        gen.generate(&FeedbackEvent::GoalAccepted {
            goal: "g1".into(),
            confidence: 0.9,
        });
        gen.generate(&FeedbackEvent::PlanningStarted { goal: "g1".into() });
        let history = gen.history();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_clear_history() {
        let gen = make_generator();
        gen.generate(&FeedbackEvent::GoalAccepted {
            goal: "g1".into(),
            confidence: 0.9,
        });
        assert_eq!(gen.history().len(), 1);
        gen.clear_history();
        assert_eq!(gen.history().len(), 0);
    }

    #[test]
    fn test_history_max_entries() {
        let gen = FeedbackGenerator::with_config(FeedbackConfig {
            max_history: 3,
            ..Default::default()
        });
        for i in 0..10 {
            gen.generate(&FeedbackEvent::GoalAccepted {
                goal: format!("g{}", i),
                confidence: 0.5,
            });
        }
        assert_eq!(gen.history().len(), 3);
    }

    #[test]
    fn test_concurrent_generations() {
        use std::thread;
        let gen = Arc::new(FeedbackGenerator::new());
        let mut handles = Vec::new();
        for i in 0..10 {
            let g = gen.clone();
            handles.push(thread::spawn(move || {
                let event = FeedbackEvent::GoalAccepted {
                    goal: format!("task-{}", i),
                    confidence: 0.5,
                };
                g.generate(&event);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let metrics = gen.metrics();
        assert_eq!(metrics.total_messages, 10);
    }

    #[test]
    fn test_concurrent_progress_and_summary() {
        use std::thread;
        let gen = Arc::new(FeedbackGenerator::new());
        let mut handles = Vec::new();
        for i in 0..5 {
            let g = gen.clone();
            handles.push(thread::spawn(move || {
                let p = g.generate_progress("exec-1", "test", i, 5, 100 * i as u64, vec![], vec![]);
                assert!(p.percentage >= (i as f32 / 5.0) * 100.0 - 1.0);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_summary_success_rate() {
        let gen = make_generator();
        let s = gen.generate_summary("e1", "test", "completed", 10, 7, 3, 1000);
        assert!((s.success_rate() - 0.7).abs() < 0.01);
        assert!(!s.is_success());
        let s2 = gen.generate_summary("e2", "test", "completed", 10, 10, 0, 1000);
        assert!((s2.success_rate() - 1.0).abs() < 0.01);
        assert!(s2.is_success());
    }

    #[test]
    fn test_summary_empty_steps() {
        let gen = make_generator();
        let s = gen.generate_summary("e1", "test", "completed", 0, 0, 0, 0);
        assert!((s.success_rate() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_generate_all_event_types() {
        let gen = make_generator();
        let events: Vec<FeedbackEvent> = vec![
            FeedbackEvent::GoalAccepted {
                goal: "g".into(),
                confidence: 1.0,
            },
            FeedbackEvent::PlanningStarted { goal: "g".into() },
            FeedbackEvent::PlanningFinished {
                goal: "g".into(),
                steps: 3,
                success: true,
            },
            FeedbackEvent::ExecutionStarted {
                execution_id: "e".into(),
                goal: "g".into(),
                total_steps: 3,
            },
            FeedbackEvent::StepStarted {
                execution_id: "e".into(),
                goal: "g".into(),
                step: "s1".into(),
                index: 0,
                total: 3,
            },
            FeedbackEvent::StepCompleted {
                execution_id: "e".into(),
                goal: "g".into(),
                step: "s1".into(),
                index: 0,
                total: 3,
                success: true,
            },
            FeedbackEvent::VerificationPassed {
                execution_id: "e".into(),
                details: "ok".into(),
            },
            FeedbackEvent::VerificationFailed {
                execution_id: "e".into(),
                details: "bad".into(),
                expected: "a".into(),
                actual: "b".into(),
            },
            FeedbackEvent::RecoveryStarted {
                execution_id: "e".into(),
                reason: "fail".into(),
                attempt: 1,
            },
            FeedbackEvent::RecoveryCompleted {
                execution_id: "e".into(),
                success: true,
                result: "ok".into(),
            },
            FeedbackEvent::ExecutionCompleted {
                execution_id: "e".into(),
                goal: "g".into(),
                steps_succeeded: 3,
                steps_failed: 0,
                duration_ms: 100,
            },
            FeedbackEvent::ExecutionCancelled {
                execution_id: "e".into(),
                goal: "g".into(),
                reason: "manual".into(),
                steps_completed: 1,
            },
            FeedbackEvent::ExecutionFailed {
                execution_id: "e".into(),
                goal: "g".into(),
                error: "err".into(),
                steps_succeeded: 1,
                steps_failed: 2,
            },
        ];
        for event in &events {
            let msg = gen.generate(event);
            assert!(!msg.message.is_empty());
        }
        assert_eq!(gen.metrics().total_messages, events.len() as u64);
    }

    #[test]
    fn test_message_timestamp() {
        let gen = make_generator();
        let event = FeedbackEvent::GoalAccepted {
            goal: "g".into(),
            confidence: 0.5,
        };
        let msg = gen.generate(&event);
        assert!(msg.timestamp_ms > 0);
    }

    #[test]
    fn test_style_default() {
        let gen = make_generator();
        assert_eq!(gen.config().default_style, FeedbackStyle::Normal);
    }

    #[test]
    fn test_feedback_context_default() {
        let ctx = FeedbackContext::default();
        assert!(ctx.execution_id.is_none());
        assert_eq!(ctx.total_steps, 0);
    }
}
