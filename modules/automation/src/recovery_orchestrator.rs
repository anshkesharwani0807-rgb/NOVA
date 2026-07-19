use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::outcome_verifier::{VerificationEvidence, VerificationResult};
use crate::pipeline_step::{PipelineStep, RetryPolicy};
use crate::world_state::WorldDiff;

/// Classification of the root cause that triggered the recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryReason {
    /// A UI element could not be found on screen.
    ElementNotFound,
    /// Generic verification failure with no specific cause identified.
    VerificationFailed,
    /// OCR text did not match the expected content.
    OCRMismatch,
    /// Visual grounding failed to locate the target element.
    GroundingFailed,
    /// A screen-capture or OCR operation timed out.
    Timeout,
    /// The foreground application changed unexpectedly.
    ActiveAppChanged,
    /// The screen content changed unexpectedly.
    ScreenChanged,
    /// A device property (wifi, bluetooth, etc.) did not match expectations.
    DeviceStateChanged,
    /// Network connectivity was unavailable when required.
    NetworkUnavailable,
    /// The operation was denied by a permission or consent gate.
    PermissionDenied,
    /// The cause could not be determined from available information.
    Unknown,
}

impl RecoveryReason {
    /// Classify a failure reason string into a structured [`RecoveryReason`].
    pub fn from_failure(reason: &str, verification: &VerificationResult) -> Self {
        let lower = reason.to_lowercase();
        if lower.contains("permission") || lower.contains("denied") {
            return Self::PermissionDenied;
        }
        if lower.contains("timeout") {
            return Self::Timeout;
        }
        if lower.contains("grounding") {
            return Self::GroundingFailed;
        }
        if lower.contains("ocr") || lower.contains("text") {
            return Self::OCRMismatch;
        }
        if lower.contains("not found") || lower.contains("element") {
            return Self::ElementNotFound;
        }
        if lower.contains("active app") || lower.contains("foreground") {
            return Self::ActiveAppChanged;
        }
        if lower.contains("network") || lower.contains("connectivity") {
            return Self::NetworkUnavailable;
        }
        if lower.contains("device") || lower.contains("telemetry") {
            return Self::DeviceStateChanged;
        }
        match verification {
            VerificationResult::Failed { .. } => Self::VerificationFailed,
            VerificationResult::Uncertain { .. } | VerificationResult::Passed => Self::Unknown,
        }
    }
}

/// Granular recovery strategy describing the specific action the
/// [`PlanExecutor`](crate::plan_executor::PlanExecutor) should take.
///
/// More detailed than [`RecoveryDecision`]; a single decision may map to
/// multiple strategies (e.g. retry + recapture + rerun OCR).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// Retry the step immediately with no delay.
    ImmediateRetry { attempt: u32, max_attempts: u32 },
    /// Retry with exponential backoff delay.
    ExponentialBackoff {
        attempt: u32,
        max_attempts: u32,
        delay_ms: u64,
        base_delay_ms: u64,
    },
    /// Re-capture the screen because the existing frame is stale.
    ReCaptureScreen { reason: String },
    /// Re-run OCR on the current frame because confidence was low.
    ReRunOCR { reason: String },
    /// Re-attempt visual grounding with a modified query.
    ReGroundElement { query: String, reason: String },
    /// Ask the planner to rebuild remaining steps.
    ReplanRemainingSteps { reason: String },
    /// Skip execution of this step (only for optional steps).
    SkipOptional { reason: String },
    /// Stop the entire plan — the failure is unrecoverable.
    AbortExecution { reason: String },
    /// Ask the user for guidance.
    RequestUserIntervention { reason: String, suggestion: String },
}

impl RecoveryStrategy {
    /// Return a short kebab-case label for this strategy.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ImmediateRetry { .. } => "immediate_retry",
            Self::ExponentialBackoff { .. } => "exponential_backoff",
            Self::ReCaptureScreen { .. } => "recapture_screen",
            Self::ReRunOCR { .. } => "rerun_ocr",
            Self::ReGroundElement { .. } => "reground_element",
            Self::ReplanRemainingSteps { .. } => "replan",
            Self::SkipOptional { .. } => "skip",
            Self::AbortExecution { .. } => "abort",
            Self::RequestUserIntervention { .. } => "escalate",
        }
    }
}

/// High-level decision produced by the [`RecoveryOrchestrator`] for a single
/// failed step.
///
/// The [`PlanExecutor`](crate::plan_executor::PlanExecutor) consumes this to
/// determine the next action in the execution loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RecoveryDecision {
    /// Retry the step after the specified delay.
    Retry {
        /// Attempt number (1-indexed).
        attempt: u32,
        /// Milliseconds to wait before retrying.
        delay_ms: u64,
    },
    /// Replan remaining steps from the given index.
    Replan {
        from_step_index: usize,
        reason: String,
    },
    /// Skip this step and continue with the plan.
    Skip { reason: String },
    /// Abort the entire plan immediately.
    Abort { reason: String },
    /// Escalate to the user for guidance.
    Escalate { reason: String, suggestion: String },
}

impl RecoveryDecision {
    /// Return a short human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Retry { .. } => "retry",
            Self::Replan { .. } => "replan",
            Self::Skip { .. } => "skip",
            Self::Abort { .. } => "abort",
            Self::Escalate { .. } => "escalate",
        }
    }

    /// Returns `true` if this decision allows the plan to continue (retry,
    /// replan, or skip).
    pub fn is_continuable(&self) -> bool {
        matches!(
            self,
            Self::Retry { .. } | Self::Replan { .. } | Self::Skip { .. }
        )
    }

    /// Returns `true` if this decision stops the plan.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Abort { .. } | Self::Escalate { .. })
    }
}

/// All inputs needed by the [`RecoveryOrchestrator`] to make a decision.
#[derive(Debug, Clone)]
pub struct RecoveryContext {
    /// The pipeline step that failed verification.
    pub step: PipelineStep,
    /// The verification result that triggered recovery.
    pub verification: VerificationResult,
    /// Evidence collected during verification.
    pub evidence: VerificationEvidence,
    /// World diff computed during verification (if available).
    pub world_diff: Option<WorldDiff>,
    /// Number of retries already attempted for this step.
    pub retry_count: u32,
    /// Human-readable failure reason (from the verification result or caller).
    pub failure_reason: String,
}

/// Full record of a single recovery decision and its outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryReport {
    /// The high-level decision made.
    pub decision: RecoveryDecision,
    /// The detailed recovery strategy.
    pub strategy: RecoveryStrategy,
    /// The classified root cause.
    pub recovery_reason: RecoveryReason,
    /// Timestamp (millis since epoch) when the decision was made.
    pub timestamp: i64,
    /// Index of the failed step within the pipeline.
    pub step_index: usize,
    /// ID of the failed step.
    pub step_id: String,
    /// Original failure reason string.
    pub failure_reason: String,
    /// Retry count at decision time.
    pub retry_count: u32,
    /// Whether the recovery was successful (retry was issued).
    pub success: bool,
}

/// Cumulative statistics maintained by the recovery history.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecoveryStatistics {
    /// Total number of recovery attempts.
    pub total_attempts: u64,
    /// Number of successful recoveries (retries issued).
    pub successful_recoveries: u64,
    /// Number of failed recoveries (abort/escalate/skip).
    pub failed_recoveries: u64,
    /// Number of aborts issued.
    pub aborted_count: u64,
    /// Number of skips issued.
    pub skipped_count: u64,
    /// Number of escalations issued.
    pub escalated_count: u64,
    /// Number of replan decisions issued.
    pub replanned_count: u64,
}

/// Thread-safe history of recovery actions for a single execution.
///
/// Tracks all decisions, provides cumulative statistics, and is safe to
/// share across concurrent tasks via [`Arc`]`<`[`RwLock`]`<Self>>`.
#[derive(Debug, Clone)]
pub struct RecoveryHistory {
    entries: Vec<RecoveryReport>,
}

impl RecoveryHistory {
    /// Create a new, empty history.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Create a history with the given pre-allocated capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            entries: Vec::with_capacity(cap),
        }
    }

    /// Record a recovery attempt.
    pub fn record_attempt(&mut self, report: RecoveryReport) {
        self.entries.push(report);
    }

    /// Record a successful recovery (convenience wrapper).
    #[allow(clippy::too_many_arguments)]
    pub fn record_success(
        &mut self,
        decision: RecoveryDecision,
        strategy: RecoveryStrategy,
        recovery_reason: RecoveryReason,
        step_index: usize,
        step_id: String,
        failure_reason: String,
        retry_count: u32,
    ) {
        let report = RecoveryReport {
            decision,
            strategy,
            recovery_reason,
            timestamp: now_millis(),
            step_index,
            step_id,
            failure_reason,
            retry_count,
            success: true,
        };
        self.entries.push(report);
    }

    /// Record a failed recovery (convenience wrapper).
    #[allow(clippy::too_many_arguments)]
    pub fn record_failure(
        &mut self,
        decision: RecoveryDecision,
        strategy: RecoveryStrategy,
        recovery_reason: RecoveryReason,
        step_index: usize,
        step_id: String,
        failure_reason: String,
        retry_count: u32,
    ) {
        let report = RecoveryReport {
            decision,
            strategy,
            recovery_reason,
            timestamp: now_millis(),
            step_index,
            step_id,
            failure_reason,
            retry_count,
            success: false,
        };
        self.entries.push(report);
    }

    /// Return all recorded reports.
    pub fn entries(&self) -> &[RecoveryReport] {
        &self.entries
    }

    /// Return the most recent report, if any.
    pub fn last(&self) -> Option<&RecoveryReport> {
        self.entries.last()
    }

    /// Return cumulative statistics from all entries.
    pub fn statistics(&self) -> RecoveryStatistics {
        let total_attempts = self.entries.len() as u64;
        let mut successful_recoveries = 0u64;
        let mut failed_recoveries = 0u64;
        let mut aborted_count = 0u64;
        let mut skipped_count = 0u64;
        let mut escalated_count = 0u64;
        let mut replanned_count = 0u64;
        for entry in &self.entries {
            if entry.success {
                successful_recoveries += 1;
            } else {
                failed_recoveries += 1;
            }
            match &entry.decision {
                RecoveryDecision::Retry { .. } => {}
                RecoveryDecision::Replan { .. } => replanned_count += 1,
                RecoveryDecision::Skip { .. } => skipped_count += 1,
                RecoveryDecision::Abort { .. } => aborted_count += 1,
                RecoveryDecision::Escalate { .. } => escalated_count += 1,
            }
        }
        RecoveryStatistics {
            total_attempts,
            successful_recoveries,
            failed_recoveries,
            aborted_count,
            skipped_count,
            escalated_count,
            replanned_count,
        }
    }

    /// Number of entries in the history.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the history is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all recorded history.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for RecoveryHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for the [`RecoveryOrchestrator`].
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Maximum backoff delay in milliseconds (cap).
    pub max_backoff_delay_ms: u64,
    /// Whether replan decisions are enabled.
    pub enable_replan: bool,
    /// Whether escalation decisions are enabled.
    pub enable_escalation: bool,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_backoff_delay_ms: 30_000,
            enable_replan: true,
            enable_escalation: true,
        }
    }
}

/// Decides what recovery action to take when a pipeline step fails
/// verification.
///
/// The orchestrator is a stateless decision engine (all mutable state lives
/// in [`RecoveryHistory`]). It accepts a [`RecoveryContext`] containing the
/// failed step, verification result, evidence, and attempt history, and
/// returns a [`RecoveryDecision`], [`RecoveryStrategy`], and
/// [`RecoveryReport`].
///
/// # Decision flow
///
/// 1. **Classify** the failure into a [`RecoveryReason`].
/// 2. **Retry** if the retry policy allows and maximum attempts have not been
///    exceeded.  The retry strategy (immediate vs. exponential backoff) is
///    selected based on the policy.
/// 3. **ReCapture / ReRunOCR / ReGround** if the specific failure suggests
///    a stale frame or low-confidence sensor data.
/// 4. **Skip** if the step is marked `continue_on_failure`.
/// 5. **Abort** if the failure is unrecoverable (permission denied, retries
///    exhausted on a required step).
/// 6. **Escalate** if user intervention may resolve the issue.
pub struct RecoveryOrchestrator {
    history: Arc<RwLock<RecoveryHistory>>,
    config: RecoveryConfig,
}

impl RecoveryOrchestrator {
    /// Create a new orchestrator with default configuration and an empty
    /// history.
    pub fn new() -> Self {
        Self {
            history: Arc::new(RwLock::new(RecoveryHistory::new())),
            config: RecoveryConfig::default(),
        }
    }

    /// Create a new orchestrator with the given configuration.
    pub fn with_config(config: RecoveryConfig) -> Self {
        Self {
            history: Arc::new(RwLock::new(RecoveryHistory::new())),
            config,
        }
    }

    /// Set a custom maximum backoff delay (cap).
    pub fn with_max_backoff_delay(mut self, ms: u64) -> Self {
        self.config.max_backoff_delay_ms = ms;
        self
    }

    /// Enable or disable replan decisions.
    pub fn with_replan(mut self, enabled: bool) -> Self {
        self.config.enable_replan = enabled;
        self
    }

    /// Enable or disable escalation decisions.
    pub fn with_escalation(mut self, enabled: bool) -> Self {
        self.config.enable_escalation = enabled;
        self
    }

    /// Return a shared reference to the internal history.
    pub fn history(&self) -> &Arc<RwLock<RecoveryHistory>> {
        &self.history
    }

    /// Return a clone of the current statistics.
    pub fn statistics(&self) -> RecoveryStatistics {
        self.history.read().statistics()
    }

    /// Decide on a recovery action for a failed step.
    ///
    /// Returns a tuple of `(decision, strategy, report)`. The caller
    /// ([`PlanExecutor`](crate::plan_executor::PlanExecutor)) should use
    /// the decision to determine the next action, the strategy for detailed
    /// guidance on *how* to retry, and the report for audit logging.
    ///
    /// This method does **not** modify the internal history — call
    /// [`record_outcome`](Self::record_outcome) separately to persist the
    /// result.
    pub fn decide(
        &self,
        ctx: &RecoveryContext,
    ) -> (RecoveryDecision, RecoveryStrategy, RecoveryReport) {
        let recovery_reason = RecoveryReason::from_failure(&ctx.failure_reason, &ctx.verification);
        let timestamp = now_millis();

        let (decision, strategy) = match &ctx.verification {
            VerificationResult::Passed => self.decide_passed(),
            VerificationResult::Failed { reason, suggestion } => {
                self.decide_failure(ctx, reason, suggestion, recovery_reason)
            }
            VerificationResult::Uncertain { reason } => {
                self.decide_uncertain(ctx, reason, recovery_reason)
            }
        };

        let report = RecoveryReport {
            decision: decision.clone(),
            strategy: strategy.clone(),
            recovery_reason,
            timestamp,
            step_index: ctx.step.step_index,
            step_id: ctx.step.step.id.clone(),
            failure_reason: ctx.failure_reason.clone(),
            retry_count: ctx.retry_count,
            success: matches!(&decision, RecoveryDecision::Retry { .. }),
        };

        (decision, strategy, report)
    }

    /// Record the outcome of a recovery decision into the history.
    ///
    /// Should be called after the decision has been acted upon.
    pub fn record_outcome(&self, report: RecoveryReport) {
        self.history.write().record_attempt(report);
    }

    /// Reset all history and statistics.
    pub fn clear_history(&self) {
        self.history.write().clear();
    }

    // -----------------------------------------------------------------------
    // Internal decision logic
    // -----------------------------------------------------------------------

    /// Handle the (unusual) case where verification passed — the step does
    /// not need recovery.
    fn decide_passed(&self) -> (RecoveryDecision, RecoveryStrategy) {
        (
            RecoveryDecision::Skip {
                reason: "step already passed; no recovery needed".into(),
            },
            RecoveryStrategy::SkipOptional {
                reason: "step already passed".into(),
            },
        )
    }

    /// Decide recovery for a definite failure.
    fn decide_failure(
        &self,
        ctx: &RecoveryContext,
        reason: &str,
        suggestion: &str,
        recovery_reason: RecoveryReason,
    ) -> (RecoveryDecision, RecoveryStrategy) {
        // Permission denied: never retry, always escalate.
        if recovery_reason == RecoveryReason::PermissionDenied {
            return (
                RecoveryDecision::Escalate {
                    reason: reason.to_string(),
                    suggestion: suggestion.to_string(),
                },
                RecoveryStrategy::RequestUserIntervention {
                    reason: reason.to_string(),
                    suggestion: suggestion.to_string(),
                },
            );
        }

        // Check if retry is available.
        if let Some((decision, strategy)) = self.try_retry(ctx, recovery_reason) {
            return (decision, strategy);
        }

        // Retries exhausted.
        self.decide_post_retry(ctx, reason, suggestion)
    }

    /// Decide recovery for an uncertain (inconclusive) verification.
    fn decide_uncertain(
        &self,
        ctx: &RecoveryContext,
        reason: &str,
        recovery_reason: RecoveryReason,
    ) -> (RecoveryDecision, RecoveryStrategy) {
        // Check if the specific uncertainty can be resolved with a targeted
        // redo (stale frame, low OCR confidence, failed grounding).
        if let Some((decision, strategy)) = self.try_specific_recovery(ctx, reason, recovery_reason)
        {
            return (decision, strategy);
        }

        // Otherwise, fall back to retry logic.
        if let Some((decision, strategy)) = self.try_retry(ctx, recovery_reason) {
            return (decision, strategy);
        }

        // Retries exhausted — check if the step is optional.
        if ctx.step.step.continue_on_failure {
            return (
                RecoveryDecision::Skip {
                    reason: format!("uncertain outcome after retries: {}", reason),
                },
                RecoveryStrategy::SkipOptional {
                    reason: format!("uncertain after retries: {}", reason),
                },
            );
        }

        // Escalate.
        (
            RecoveryDecision::Escalate {
                reason: format!("unresolved uncertainty: {}", reason),
                suggestion: "manual verification required".into(),
            },
            RecoveryStrategy::RequestUserIntervention {
                reason: format!("unresolved uncertainty: {}", reason),
                suggestion: "manual verification required".into(),
            },
        )
    }

    /// Attempt to issue a retry decision based on the step's retry policy.
    fn try_retry(
        &self,
        ctx: &RecoveryContext,
        recovery_reason: RecoveryReason,
    ) -> Option<(RecoveryDecision, RecoveryStrategy)> {
        let (max_retries, base_delay_ms) = match &ctx.step.retry_policy {
            RetryPolicy::NoRetry => return None,
            RetryPolicy::Fixed(n) if ctx.retry_count >= *n => return None,
            RetryPolicy::Fixed(n) => (*n, 0),
            RetryPolicy::ExponentialBackoff {
                max_retries,
                base_delay_ms,
            } if ctx.retry_count >= *max_retries => return None,
            RetryPolicy::ExponentialBackoff {
                max_retries,
                base_delay_ms,
            } => (*max_retries, *base_delay_ms),
        };

        let attempt = ctx.retry_count + 1;

        // Select a retry strategy based on the recovery reason and policy.
        if base_delay_ms == 0 {
            // Fixed retry — no delay.
            let strategy = match recovery_reason {
                RecoveryReason::GroundingFailed => RecoveryStrategy::ReGroundElement {
                    query: ctx.step.step.description.clone(),
                    reason: ctx.failure_reason.clone(),
                },
                RecoveryReason::OCRMismatch => RecoveryStrategy::ReRunOCR {
                    reason: ctx.failure_reason.clone(),
                },
                RecoveryReason::Timeout => RecoveryStrategy::ReCaptureScreen {
                    reason: ctx.failure_reason.clone(),
                },
                _ => RecoveryStrategy::ImmediateRetry {
                    attempt,
                    max_attempts: max_retries,
                },
            };
            Some((
                RecoveryDecision::Retry {
                    attempt,
                    delay_ms: 0,
                },
                strategy,
            ))
        } else {
            // Exponential backoff.
            let raw_delay = base_delay_ms.saturating_mul(1u64 << (attempt - 1).min(10));
            let delay_ms = raw_delay.min(self.config.max_backoff_delay_ms);
            Some((
                RecoveryDecision::Retry { attempt, delay_ms },
                RecoveryStrategy::ExponentialBackoff {
                    attempt,
                    max_attempts: max_retries,
                    delay_ms,
                    base_delay_ms,
                },
            ))
        }
    }

    /// Attempt a targeted recovery for specific uncertainty types.
    fn try_specific_recovery(
        &self,
        ctx: &RecoveryContext,
        reason: &str,
        recovery_reason: RecoveryReason,
    ) -> Option<(RecoveryDecision, RecoveryStrategy)> {
        match recovery_reason {
            RecoveryReason::GroundingFailed => {
                let query = ctx.step.step.description.clone();
                Some((
                    RecoveryDecision::Retry {
                        attempt: ctx.retry_count + 1,
                        delay_ms: 0,
                    },
                    RecoveryStrategy::ReGroundElement {
                        query,
                        reason: reason.to_string(),
                    },
                ))
            }
            RecoveryReason::OCRMismatch => Some((
                RecoveryDecision::Retry {
                    attempt: ctx.retry_count + 1,
                    delay_ms: 0,
                },
                RecoveryStrategy::ReRunOCR {
                    reason: reason.to_string(),
                },
            )),
            RecoveryReason::Timeout => Some((
                RecoveryDecision::Retry {
                    attempt: ctx.retry_count + 1,
                    delay_ms: 0,
                },
                RecoveryStrategy::ReCaptureScreen {
                    reason: reason.to_string(),
                },
            )),
            _ => None,
        }
    }

    /// Decide what to do when retries are exhausted.
    fn decide_post_retry(
        &self,
        ctx: &RecoveryContext,
        reason: &str,
        suggestion: &str,
    ) -> (RecoveryDecision, RecoveryStrategy) {
        // Optional step: skip.
        if ctx.step.step.continue_on_failure {
            return (
                RecoveryDecision::Skip {
                    reason: format!("retries exhausted: {}", reason),
                },
                RecoveryStrategy::SkipOptional {
                    reason: format!("retries exhausted: {}", reason),
                },
            );
        }

        // Replan enabled and environment changed → replan.
        if self.config.enable_replan {
            let has_env_change = ctx
                .world_diff
                .as_ref()
                .map(|d| d.has_any_change)
                .unwrap_or(false);
            if has_env_change {
                return (
                    RecoveryDecision::Replan {
                        from_step_index: ctx.step.step_index,
                        reason: format!("environment changed after retry exhaustion: {}", reason),
                    },
                    RecoveryStrategy::ReplanRemainingSteps {
                        reason: format!("environment changed: {}", reason),
                    },
                );
            }
        }

        // Abort on failures that suggest irrecoverable state.
        let msg = format!("retries exhausted, step required: {}", reason);
        if self.config.enable_escalation {
            (
                RecoveryDecision::Escalate {
                    reason: msg.clone(),
                    suggestion: suggestion.to_string(),
                },
                RecoveryStrategy::RequestUserIntervention {
                    reason: msg,
                    suggestion: suggestion.to_string(),
                },
            )
        } else {
            (
                RecoveryDecision::Abort {
                    reason: msg.clone(),
                },
                RecoveryStrategy::AbortExecution { reason: msg },
            )
        }
    }
}

impl Default for RecoveryOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionType;
    use crate::outcome_verifier::VerificationEvidence;
    use crate::pipeline_step::{ExpectedOutcome, VerificationStrategy};

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_step(
        id: &str,
        action: ActionType,
        retry_policy: RetryPolicy,
        continue_on_failure: bool,
        index: usize,
    ) -> PipelineStep {
        use crate::planner::ExecutionStep;
        let step = ExecutionStep {
            id: id.to_string(),
            description: format!("step {}", id),
            action,
            dependencies: vec![],
            required_capabilities: vec![],
            timeout_ms: 5000,
            retry_count: match &retry_policy {
                RetryPolicy::Fixed(n) => *n,
                RetryPolicy::ExponentialBackoff { max_retries, .. } => *max_retries,
                RetryPolicy::NoRetry => 0,
            },
            continue_on_failure,
        };
        PipelineStep::new(
            step,
            index,
            vec![],
            VerificationStrategy::NoVerification,
            ExpectedOutcome::NoChange,
            retry_policy,
        )
    }

    fn make_context(
        step: PipelineStep,
        verification: VerificationResult,
        retry_count: u32,
        world_diff: Option<WorldDiff>,
        failure_reason: String,
    ) -> RecoveryContext {
        RecoveryContext {
            step,
            verification,
            evidence: VerificationEvidence::new(),
            world_diff,
            retry_count,
            failure_reason,
        }
    }

    fn failed_result(reason: &str, suggestion: &str) -> VerificationResult {
        VerificationResult::Failed {
            reason: reason.to_string(),
            suggestion: suggestion.to_string(),
        }
    }

    fn uncertain_result(reason: &str) -> VerificationResult {
        VerificationResult::Uncertain {
            reason: reason.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // Construction tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_orchestrator_new() {
        let orch = RecoveryOrchestrator::new();
        assert!(orch.history.read().is_empty());
        assert_eq!(orch.config.max_backoff_delay_ms, 30_000);
        assert!(orch.config.enable_replan);
        assert!(orch.config.enable_escalation);
    }

    #[test]
    fn test_orchestrator_with_config() {
        let config = RecoveryConfig {
            max_backoff_delay_ms: 10_000,
            enable_replan: false,
            enable_escalation: false,
        };
        let orch = RecoveryOrchestrator::with_config(config);
        assert_eq!(orch.config.max_backoff_delay_ms, 10_000);
        assert!(!orch.config.enable_replan);
        assert!(!orch.config.enable_escalation);
    }

    #[test]
    fn test_orchestrator_builder_methods() {
        let orch = RecoveryOrchestrator::new()
            .with_max_backoff_delay(60_000)
            .with_replan(false)
            .with_escalation(false);
        assert_eq!(orch.config.max_backoff_delay_ms, 60_000);
        assert!(!orch.config.enable_replan);
        assert!(!orch.config.enable_escalation);
    }

    // -----------------------------------------------------------------------
    // RecoveryDecision tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_recovery_decision_label() {
        assert_eq!(
            RecoveryDecision::Retry {
                attempt: 1,
                delay_ms: 0
            }
            .label(),
            "retry"
        );
        assert_eq!(
            RecoveryDecision::Replan {
                from_step_index: 0,
                reason: "".into()
            }
            .label(),
            "replan"
        );
        assert_eq!(RecoveryDecision::Skip { reason: "".into() }.label(), "skip");
        assert_eq!(
            RecoveryDecision::Abort { reason: "".into() }.label(),
            "abort"
        );
        assert_eq!(
            RecoveryDecision::Escalate {
                reason: "".into(),
                suggestion: "".into()
            }
            .label(),
            "escalate"
        );
    }

    #[test]
    fn test_recovery_decision_is_continuable() {
        assert!(RecoveryDecision::Retry {
            attempt: 1,
            delay_ms: 0
        }
        .is_continuable());
        assert!(!RecoveryDecision::Abort { reason: "".into() }.is_continuable());
        assert!(!RecoveryDecision::Escalate {
            reason: "".into(),
            suggestion: "".into()
        }
        .is_continuable());
        assert!(RecoveryDecision::Skip { reason: "".into() }.is_continuable());
    }

    #[test]
    fn test_recovery_decision_is_terminal() {
        assert!(RecoveryDecision::Abort { reason: "".into() }.is_terminal());
        assert!(RecoveryDecision::Escalate {
            reason: "".into(),
            suggestion: "".into()
        }
        .is_terminal());
        assert!(!RecoveryDecision::Retry {
            attempt: 1,
            delay_ms: 0
        }
        .is_terminal());
        assert!(!RecoveryDecision::Skip { reason: "".into() }.is_terminal());
    }

    // -----------------------------------------------------------------------
    // RecoveryReason classification tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_recovery_reason_classification() {
        let failed = failed_result("permission denied: no access", "ask admin");
        assert_eq!(
            RecoveryReason::from_failure("permission denied: no access", &failed),
            RecoveryReason::PermissionDenied
        );

        let timeout_res = failed_result("step timeout exceeded", "retry later");
        assert_eq!(
            RecoveryReason::from_failure("step timeout exceeded", &timeout_res),
            RecoveryReason::Timeout
        );

        let nf = failed_result("element not found", "try again");
        assert_eq!(
            RecoveryReason::from_failure("element not found", &nf),
            RecoveryReason::ElementNotFound
        );
    }

    // -----------------------------------------------------------------------
    // Recovery strategy kind
    // -----------------------------------------------------------------------

    #[test]
    fn test_recovery_strategy_kind() {
        assert_eq!(
            RecoveryStrategy::ImmediateRetry {
                attempt: 1,
                max_attempts: 3
            }
            .kind(),
            "immediate_retry"
        );
        assert_eq!(
            RecoveryStrategy::RequestUserIntervention {
                reason: "".into(),
                suggestion: "".into()
            }
            .kind(),
            "escalate"
        );
    }

    // -----------------------------------------------------------------------
    // Decide: retry success
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_retry_success_immediate() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::Fixed(3),
            false,
            0,
        );
        let ctx = make_context(
            step,
            failed_result("element not found", "retry"),
            0, // first failure
            None,
            "element not found".into(),
        );
        let (decision, strategy, report) = orch.decide(&ctx);

        assert_eq!(
            decision,
            RecoveryDecision::Retry {
                attempt: 1,
                delay_ms: 0
            }
        );
        assert!(matches!(strategy, RecoveryStrategy::ImmediateRetry { .. }));
        assert!(report.success);
        assert_eq!(report.retry_count, 0);
    }

    // -----------------------------------------------------------------------
    // Decide: retry exhausted
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_retry_exhausted_required_step_escalates() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::Fixed(3),
            false,
            0,
        );
        let ctx = make_context(
            step,
            failed_result("verification failed", "try a different approach"),
            3, // exhausted: 0,1,2,3 = 3 retries already done (count starts at 0)
            None,
            "verification failed".into(),
        );
        let (decision, strategy, _) = orch.decide(&ctx);

        assert!(matches!(decision, RecoveryDecision::Escalate { .. }));
        assert!(matches!(
            strategy,
            RecoveryStrategy::RequestUserIntervention { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // Decide: skip optional step when retries exhausted
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_skip_optional_when_retries_exhausted() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::Fixed(2),
            true,
            0,
        );
        let ctx = make_context(
            step,
            failed_result("verification failed", "skip"),
            2,
            None,
            "verification failed".into(),
        );
        let (decision, strategy, _) = orch.decide(&ctx);

        assert!(matches!(decision, RecoveryDecision::Skip { .. }));
        assert!(matches!(strategy, RecoveryStrategy::SkipOptional { .. }));
    }

    // -----------------------------------------------------------------------
    // Decide: abort irreversible failure when escalation disabled
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_abort_when_escalation_disabled() {
        let orch = RecoveryOrchestrator::new().with_escalation(false);
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::NoRetry,
            false,
            0,
        );
        let ctx = make_context(
            step,
            failed_result("verification failed", "abort"),
            0,
            None,
            "verification failed".into(),
        );
        let (decision, strategy, _) = orch.decide(&ctx);

        assert!(matches!(decision, RecoveryDecision::Abort { .. }));
        assert!(matches!(strategy, RecoveryStrategy::AbortExecution { .. }));
    }

    // -----------------------------------------------------------------------
    // Decide: escalate permission denied
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_escalate_permission_denied() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::Fixed(5),
            false,
            0,
        );
        let ctx = make_context(
            step,
            failed_result("permission denied: no access", "ask admin"),
            0,
            None,
            "permission denied: no access".into(),
        );
        let (decision, strategy, _) = orch.decide(&ctx);

        assert!(matches!(decision, RecoveryDecision::Escalate { .. }));
        assert!(matches!(
            strategy,
            RecoveryStrategy::RequestUserIntervention { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // Decide: replan when environment changed
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_replan_when_env_changed() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::ClickScreenElement {
                query: "btn".into(),
            },
            RetryPolicy::Fixed(1),
            false,
            0,
        );
        let diff = WorldDiff {
            frame_changed: true,
            has_any_change: true,
            ..WorldDiff::new()
        };
        let ctx = make_context(
            step,
            failed_result("element not found", "replan"),
            1, // retries exhausted
            Some(diff),
            "element not found".into(),
        );
        let (decision, strategy, _) = orch.decide(&ctx);

        assert!(matches!(decision, RecoveryDecision::Replan { .. }));
        assert!(matches!(
            strategy,
            RecoveryStrategy::ReplanRemainingSteps { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // Decide: uncertain with timeout triggers recapture
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_uncertain_timeout_triggers_recapture() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::ClickScreenElement {
                query: "btn".into(),
            },
            RetryPolicy::Fixed(3),
            false,
            0,
        );
        let ctx = make_context(
            step,
            uncertain_result("Timeout: frame capture exceeded limit"),
            0,
            None,
            "Timeout: frame capture exceeded limit".into(),
        );
        let (decision, strategy, _) = orch.decide(&ctx);

        assert!(matches!(decision, RecoveryDecision::Retry { .. }));
        assert!(matches!(strategy, RecoveryStrategy::ReCaptureScreen { .. }));
    }

    // -----------------------------------------------------------------------
    // Decide: uncertain with OCR mismatch triggers rerun OCR
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_uncertain_ocr_triggers_rerun_ocr() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::ClickScreenElement {
                query: "btn".into(),
            },
            RetryPolicy::Fixed(3),
            false,
            0,
        );
        let ctx = make_context(
            step,
            uncertain_result("OCR confidence below threshold"),
            0,
            None,
            "OCR confidence below threshold".into(),
        );
        let (decision, strategy, _) = orch.decide(&ctx);

        assert!(matches!(decision, RecoveryDecision::Retry { .. }));
        assert!(matches!(strategy, RecoveryStrategy::ReRunOCR { .. }));
    }

    // -----------------------------------------------------------------------
    // Decide: uncertain with grounding failure triggers reground
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_uncertain_grounding_triggers_reground() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::ClickScreenElement {
                query: "btn".into(),
            },
            RetryPolicy::Fixed(3),
            false,
            0,
        );
        let ctx = make_context(
            step,
            uncertain_result("grounding failed: no match"),
            0,
            None,
            "grounding failed: no match".into(),
        );
        let (decision, strategy, _) = orch.decide(&ctx);

        assert!(matches!(decision, RecoveryDecision::Retry { .. }));
        assert!(matches!(strategy, RecoveryStrategy::ReGroundElement { .. }));
    }

    // -----------------------------------------------------------------------
    // Exponential backoff calculation
    // -----------------------------------------------------------------------

    #[test]
    fn test_exponential_backoff_delay() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::ExponentialBackoff {
                max_retries: 5,
                base_delay_ms: 1000,
            },
            false,
            0,
        );

        // First retry (count=0): delay = 1000 * 2^0 = 1000
        let ctx = make_context(
            step.clone(),
            failed_result("timeout", "retry"),
            0,
            None,
            "timeout".into(),
        );
        let (decision, _, _) = orch.decide(&ctx);
        assert_eq!(
            decision,
            RecoveryDecision::Retry {
                attempt: 1,
                delay_ms: 1000
            }
        );

        // Second retry (count=1): delay = 1000 * 2^1 = 2000
        let ctx = make_context(
            step.clone(),
            failed_result("timeout", "retry"),
            1,
            None,
            "timeout".into(),
        );
        let (decision, _, _) = orch.decide(&ctx);
        assert_eq!(
            decision,
            RecoveryDecision::Retry {
                attempt: 2,
                delay_ms: 2000
            }
        );

        // Third retry (count=2): delay = 1000 * 2^2 = 4000
        let ctx = make_context(
            step.clone(),
            failed_result("timeout", "retry"),
            2,
            None,
            "timeout".into(),
        );
        let (decision, _, _) = orch.decide(&ctx);
        assert_eq!(
            decision,
            RecoveryDecision::Retry {
                attempt: 3,
                delay_ms: 4000
            }
        );
    }

    #[test]
    fn test_exponential_backoff_cap() {
        let orch = RecoveryOrchestrator::new().with_max_backoff_delay(5_000);
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::ExponentialBackoff {
                max_retries: 10,
                base_delay_ms: 1000,
            },
            false,
            0,
        );

        // 4th retry (count=3): delay = 1000 * 2^3 = 8000, capped at 5000
        let ctx = make_context(
            step.clone(),
            failed_result("timeout", "retry"),
            3,
            None,
            "timeout".into(),
        );
        let (decision, _, _) = orch.decide(&ctx);
        assert_eq!(
            decision,
            RecoveryDecision::Retry {
                attempt: 4,
                delay_ms: 5000
            }
        );
    }

    // -----------------------------------------------------------------------
    // NoRetry policy
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_no_retry_policy_skips_retry() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::NoRetry,
            false,
            0,
        );
        let ctx = make_context(
            step,
            failed_result("verification failed", "try again"),
            0,
            None,
            "verification failed".into(),
        );
        let (decision, _, _) = orch.decide(&ctx);

        // No retries available → escalate
        assert!(matches!(decision, RecoveryDecision::Escalate { .. }));
    }

    // -----------------------------------------------------------------------
    // History recording
    // -----------------------------------------------------------------------

    #[test]
    fn test_history_recording() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::Fixed(3),
            false,
            0,
        );
        let ctx = make_context(
            step,
            failed_result("element not found", "retry"),
            0,
            None,
            "element not found".into(),
        );
        let (_, _, report) = orch.decide(&ctx);

        assert!(orch.history.read().is_empty());
        orch.record_outcome(report);
        assert_eq!(orch.history.read().len(), 1);
    }

    // -----------------------------------------------------------------------
    // Statistics
    // -----------------------------------------------------------------------

    #[test]
    fn test_statistics_update() {
        let orch = RecoveryOrchestrator::new();

        // Record a success (retry)
        orch.record_outcome(RecoveryReport {
            decision: RecoveryDecision::Retry {
                attempt: 1,
                delay_ms: 0,
            },
            strategy: RecoveryStrategy::ImmediateRetry {
                attempt: 1,
                max_attempts: 3,
            },
            recovery_reason: RecoveryReason::ElementNotFound,
            timestamp: 0,
            step_index: 0,
            step_id: "s1".into(),
            failure_reason: "not found".into(),
            retry_count: 0,
            success: true,
        });

        // Record a failure (abort)
        orch.record_outcome(RecoveryReport {
            decision: RecoveryDecision::Abort {
                reason: "exhausted".into(),
            },
            strategy: RecoveryStrategy::AbortExecution {
                reason: "exhausted".into(),
            },
            recovery_reason: RecoveryReason::VerificationFailed,
            timestamp: 1,
            step_index: 0,
            step_id: "s1".into(),
            failure_reason: "exhausted".into(),
            retry_count: 3,
            success: false,
        });

        let stats = orch.statistics();
        assert_eq!(stats.total_attempts, 2);
        assert_eq!(stats.successful_recoveries, 1);
        assert_eq!(stats.failed_recoveries, 1);
        assert_eq!(stats.aborted_count, 1);
    }

    // -----------------------------------------------------------------------
    // Clear history
    // -----------------------------------------------------------------------

    #[test]
    fn test_clear_history() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::Fixed(3),
            false,
            0,
        );
        let ctx = make_context(
            step,
            failed_result("error", "retry"),
            0,
            None,
            "error".into(),
        );
        let (_, _, report) = orch.decide(&ctx);
        orch.record_outcome(report);
        assert_eq!(orch.history.read().len(), 1);

        orch.clear_history();
        assert!(orch.history.read().is_empty());
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_decide_passed_verification_returns_skip() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::NoRetry,
            false,
            0,
        );
        let ctx = make_context(step, VerificationResult::Passed, 0, None, "passed".into());
        let (decision, _, _) = orch.decide(&ctx);

        assert!(matches!(decision, RecoveryDecision::Skip { .. }));
    }

    #[test]
    fn test_recovery_history_default() {
        let hist = RecoveryHistory::default();
        assert!(hist.is_empty());
        assert_eq!(hist.len(), 0);
    }

    #[test]
    fn test_recovery_history_with_capacity() {
        let hist = RecoveryHistory::with_capacity(100);
        assert!(hist.is_empty());
    }

    #[test]
    fn test_recovery_history_record_success_and_failure() {
        let mut hist = RecoveryHistory::new();
        hist.record_success(
            RecoveryDecision::Retry {
                attempt: 1,
                delay_ms: 0,
            },
            RecoveryStrategy::ImmediateRetry {
                attempt: 1,
                max_attempts: 3,
            },
            RecoveryReason::ElementNotFound,
            0,
            "s1".into(),
            "not found".into(),
            0,
        );
        hist.record_failure(
            RecoveryDecision::Abort {
                reason: "exhausted".into(),
            },
            RecoveryStrategy::AbortExecution {
                reason: "exhausted".into(),
            },
            RecoveryReason::VerificationFailed,
            0,
            "s1".into(),
            "exhausted".into(),
            3,
        );

        assert_eq!(hist.len(), 2);
        assert!(hist.last().is_some());
        assert!(!hist.last().unwrap().success);
    }

    #[test]
    fn test_fixed_retry_zero_acts_like_no_retry() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::Fixed(0),
            false,
            0,
        );
        let ctx = make_context(
            step,
            failed_result("error", "retry"),
            0,
            None,
            "error".into(),
        );
        let (decision, _, _) = orch.decide(&ctx);

        // Fixed(0) means zero retries allowed → escalate
        assert!(matches!(decision, RecoveryDecision::Escalate { .. }));
    }

    #[test]
    fn test_last_entry() {
        let mut hist = RecoveryHistory::new();
        assert!(hist.last().is_none());
        hist.record_success(
            RecoveryDecision::Retry {
                attempt: 1,
                delay_ms: 100,
            },
            RecoveryStrategy::ExponentialBackoff {
                attempt: 1,
                max_attempts: 3,
                delay_ms: 100,
                base_delay_ms: 100,
            },
            RecoveryReason::Timeout,
            0,
            "s1".into(),
            "timeout".into(),
            0,
        );
        assert!(hist.last().is_some());
        assert!(hist.last().unwrap().success);
    }

    #[test]
    fn test_empty_statistics() {
        let hist = RecoveryHistory::new();
        let stats = hist.statistics();
        assert_eq!(stats.total_attempts, 0);
        assert_eq!(stats.successful_recoveries, 0);
        assert_eq!(stats.failed_recoveries, 0);
    }

    #[test]
    fn test_decide_with_exponential_backoff_strategy_type() {
        let orch = RecoveryOrchestrator::new();
        let step = make_step(
            "s1",
            ActionType::Wait { duration_ms: 1 },
            RetryPolicy::ExponentialBackoff {
                max_retries: 5,
                base_delay_ms: 500,
            },
            false,
            0,
        );
        let ctx = make_context(
            step,
            failed_result("timeout", "retry"),
            0,
            None,
            "timeout".into(),
        );
        let (_, strategy, _) = orch.decide(&ctx);

        assert!(matches!(
            strategy,
            RecoveryStrategy::ExponentialBackoff { .. }
        ));
        if let RecoveryStrategy::ExponentialBackoff {
            delay_ms,
            base_delay_ms,
            ..
        } = &strategy
        {
            assert_eq!(*delay_ms, 500); // 500 * 2^0 = 500
            assert_eq!(*base_delay_ms, 500);
        }
    }

    // -----------------------------------------------------------------------
    // Concurrent access (basic smoke test)
    // -----------------------------------------------------------------------

    #[test]
    fn test_concurrent_history_access() {
        use std::thread;
        let orch = Arc::new(RecoveryOrchestrator::new());
        let mut handles = Vec::new();

        for i in 0..5 {
            let orch_clone = Arc::clone(&orch);
            handles.push(thread::spawn(move || {
                let step = make_step(
                    &format!("s{}", i),
                    ActionType::Wait { duration_ms: 1 },
                    RetryPolicy::Fixed(3),
                    false,
                    i,
                );
                let ctx = make_context(
                    step,
                    failed_result("error", "retry"),
                    0,
                    None,
                    "error".into(),
                );
                let (_, _, report) = orch_clone.decide(&ctx);
                orch_clone.record_outcome(report);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(orch.history.read().len(), 5);
    }

    // -----------------------------------------------------------------------
    // RecoveryReport serde roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn test_recovery_report_serialization() {
        let report = RecoveryReport {
            decision: RecoveryDecision::Retry {
                attempt: 2,
                delay_ms: 1000,
            },
            strategy: RecoveryStrategy::ExponentialBackoff {
                attempt: 2,
                max_attempts: 5,
                delay_ms: 1000,
                base_delay_ms: 500,
            },
            recovery_reason: RecoveryReason::Timeout,
            timestamp: 12345,
            step_index: 0,
            step_id: "s1".into(),
            failure_reason: "timeout".into(),
            retry_count: 1,
            success: true,
        };

        let json = serde_json::to_string(&report).unwrap();
        let deserialized: RecoveryReport = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.step_id, "s1");
        assert_eq!(deserialized.retry_count, 1);
        assert!(deserialized.success);
        assert!(matches!(
            deserialized.decision,
            RecoveryDecision::Retry {
                attempt: 2,
                delay_ms: 1000
            }
        ));
    }
}
