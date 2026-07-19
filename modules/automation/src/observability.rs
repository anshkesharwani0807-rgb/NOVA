use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub total_goals: u64,
    pub successful_goals: u64,
    pub failed_goals: u64,
    pub pipelines_executed: u64,
    pub total_steps_attempted: u64,
    pub completed_steps: u64,
    pub failed_steps: u64,
    pub skipped_steps: u64,
    pub retries: u64,
    pub replans: u64,
    pub recoveries: u64,
    pub verification_count: u64,
    pub total_execution_duration_ms: u64,
    pub total_verification_duration_ms: u64,
    pub total_recovery_duration_ms: u64,
}

impl ExecutionMetrics {
    pub fn record_goal_start(&mut self) {
        self.total_goals += 1;
    }

    pub fn record_goal_success(&mut self) {
        self.successful_goals += 1;
    }

    pub fn record_goal_failure(&mut self) {
        self.failed_goals += 1;
    }

    pub fn record_pipeline(&mut self) {
        self.pipelines_executed += 1;
    }

    pub fn record_step_completed(&mut self) {
        self.completed_steps += 1;
    }

    pub fn record_step_failed(&mut self) {
        self.failed_steps += 1;
    }

    pub fn record_step_skipped(&mut self) {
        self.skipped_steps += 1;
    }

    pub fn record_retry(&mut self) {
        self.retries += 1;
    }

    pub fn record_replan(&mut self) {
        self.replans += 1;
    }

    pub fn record_recovery(&mut self) {
        self.recoveries += 1;
    }

    pub fn record_verification(&mut self) {
        self.verification_count += 1;
    }

    pub fn record_execution_duration(&mut self, duration: Duration) {
        self.total_execution_duration_ms += duration.as_millis() as u64;
    }

    pub fn record_verification_duration(&mut self, duration: Duration) {
        self.total_verification_duration_ms += duration.as_millis() as u64;
    }

    pub fn record_recovery_duration(&mut self, duration: Duration) {
        self.total_recovery_duration_ms += duration.as_millis() as u64;
    }

    pub fn reset(&mut self) {
        self.total_goals = 0;
        self.successful_goals = 0;
        self.failed_goals = 0;
        self.pipelines_executed = 0;
        self.total_steps_attempted = 0;
        self.completed_steps = 0;
        self.failed_steps = 0;
        self.skipped_steps = 0;
        self.retries = 0;
        self.replans = 0;
        self.recoveries = 0;
        self.verification_count = 0;
        self.total_execution_duration_ms = 0;
        self.total_verification_duration_ms = 0;
        self.total_recovery_duration_ms = 0;
    }

    pub fn snapshot(&self) -> ExecutionMetrics {
        self.clone()
    }

    pub fn merge(&mut self, other: &ExecutionMetrics) {
        self.total_goals += other.total_goals;
        self.successful_goals += other.successful_goals;
        self.failed_goals += other.failed_goals;
        self.pipelines_executed += other.pipelines_executed;
        self.total_steps_attempted += other.total_steps_attempted;
        self.completed_steps += other.completed_steps;
        self.failed_steps += other.failed_steps;
        self.skipped_steps += other.skipped_steps;
        self.retries += other.retries;
        self.replans += other.replans;
        self.recoveries += other.recoveries;
        self.verification_count += other.verification_count;
        self.total_execution_duration_ms += other.total_execution_duration_ms;
        self.total_verification_duration_ms += other.total_verification_duration_ms;
        self.total_recovery_duration_ms += other.total_recovery_duration_ms;
    }

    pub fn average_execution_duration_ms(&self) -> f64 {
        if self.total_goals == 0 {
            0.0
        } else {
            self.total_execution_duration_ms as f64 / self.total_goals as f64
        }
    }

    pub fn average_verification_duration_ms(&self) -> f64 {
        if self.verification_count == 0 {
            0.0
        } else {
            self.total_verification_duration_ms as f64 / self.verification_count as f64
        }
    }

    pub fn average_recovery_duration_ms(&self) -> f64 {
        if self.recoveries == 0 {
            0.0
        } else {
            self.total_recovery_duration_ms as f64 / self.recoveries as f64
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub execution_id: String,
    pub goal: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub success: bool,
    pub pipeline_traces: Vec<PipelineTrace>,
    pub metrics: ExecutionMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTrace {
    pub pipeline_id: String,
    pub goal: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub success: bool,
    pub step_traces: Vec<StepTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTrace {
    pub step_id: String,
    pub step_index: usize,
    pub description: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub success: bool,
    pub attempts: u32,
    pub error: Option<String>,
    pub verification_trace: Option<VerificationTrace>,
    pub recovery_trace: Option<RecoveryTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationTrace {
    pub step_id: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub passed: bool,
    pub strategy: String,
    pub reason: Option<String>,
    pub evidence_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryTrace {
    pub step_id: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub decision: String,
    pub strategy: String,
    pub reason: String,
    pub retry_count: u32,
    pub success: bool,
}

pub struct SharedMetrics {
    inner: Arc<AtomicMetrics>,
}

struct AtomicMetrics {
    total_goals: AtomicU64,
    successful_goals: AtomicU64,
    failed_goals: AtomicU64,
    pipelines_executed: AtomicU64,
    total_steps_attempted: AtomicU64,
    completed_steps: AtomicU64,
    failed_steps: AtomicU64,
    skipped_steps: AtomicU64,
    retries: AtomicU64,
    replans: AtomicU64,
    recoveries: AtomicU64,
    verification_count: AtomicU64,
    total_execution_duration_ms: AtomicU64,
    total_verification_duration_ms: AtomicU64,
    total_recovery_duration_ms: AtomicU64,
}

impl AtomicMetrics {
    fn new() -> Self {
        Self {
            total_goals: AtomicU64::new(0),
            successful_goals: AtomicU64::new(0),
            failed_goals: AtomicU64::new(0),
            pipelines_executed: AtomicU64::new(0),
            total_steps_attempted: AtomicU64::new(0),
            completed_steps: AtomicU64::new(0),
            failed_steps: AtomicU64::new(0),
            skipped_steps: AtomicU64::new(0),
            retries: AtomicU64::new(0),
            replans: AtomicU64::new(0),
            recoveries: AtomicU64::new(0),
            verification_count: AtomicU64::new(0),
            total_execution_duration_ms: AtomicU64::new(0),
            total_verification_duration_ms: AtomicU64::new(0),
            total_recovery_duration_ms: AtomicU64::new(0),
        }
    }
}

impl SharedMetrics {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicMetrics::new()),
        }
    }

    pub fn record_goal_start(&self) {
        self.inner.total_goals.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_goal_success(&self) {
        self.inner.successful_goals.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_goal_failure(&self) {
        self.inner.failed_goals.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pipeline(&self) {
        self.inner
            .pipelines_executed
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_step_attempted(&self) {
        self.inner
            .total_steps_attempted
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_step_completed(&self) {
        self.inner.completed_steps.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_step_failed(&self) {
        self.inner.failed_steps.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_step_skipped(&self) {
        self.inner.skipped_steps.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_retry(&self) {
        self.inner.retries.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_replan(&self) {
        self.inner.replans.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_recovery(&self) {
        self.inner.recoveries.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_verification(&self) {
        self.inner
            .verification_count
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_execution_duration(&self, duration: Duration) {
        self.inner
            .total_execution_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
    }

    pub fn record_verification_duration(&self, duration: Duration) {
        self.inner
            .total_verification_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
    }

    pub fn record_recovery_duration(&self, duration: Duration) {
        self.inner
            .total_recovery_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> ExecutionMetrics {
        ExecutionMetrics {
            total_goals: self.inner.total_goals.load(Ordering::Relaxed),
            successful_goals: self.inner.successful_goals.load(Ordering::Relaxed),
            failed_goals: self.inner.failed_goals.load(Ordering::Relaxed),
            pipelines_executed: self.inner.pipelines_executed.load(Ordering::Relaxed),
            total_steps_attempted: self.inner.total_steps_attempted.load(Ordering::Relaxed),
            completed_steps: self.inner.completed_steps.load(Ordering::Relaxed),
            failed_steps: self.inner.failed_steps.load(Ordering::Relaxed),
            skipped_steps: self.inner.skipped_steps.load(Ordering::Relaxed),
            retries: self.inner.retries.load(Ordering::Relaxed),
            replans: self.inner.replans.load(Ordering::Relaxed),
            recoveries: self.inner.recoveries.load(Ordering::Relaxed),
            verification_count: self.inner.verification_count.load(Ordering::Relaxed),
            total_execution_duration_ms: self
                .inner
                .total_execution_duration_ms
                .load(Ordering::Relaxed),
            total_verification_duration_ms: self
                .inner
                .total_verification_duration_ms
                .load(Ordering::Relaxed),
            total_recovery_duration_ms: self
                .inner
                .total_recovery_duration_ms
                .load(Ordering::Relaxed),
        }
    }

    pub fn reset(&self) {
        self.inner.total_goals.store(0, Ordering::Relaxed);
        self.inner.successful_goals.store(0, Ordering::Relaxed);
        self.inner.failed_goals.store(0, Ordering::Relaxed);
        self.inner.pipelines_executed.store(0, Ordering::Relaxed);
        self.inner.total_steps_attempted.store(0, Ordering::Relaxed);
        self.inner.completed_steps.store(0, Ordering::Relaxed);
        self.inner.failed_steps.store(0, Ordering::Relaxed);
        self.inner.skipped_steps.store(0, Ordering::Relaxed);
        self.inner.retries.store(0, Ordering::Relaxed);
        self.inner.replans.store(0, Ordering::Relaxed);
        self.inner.recoveries.store(0, Ordering::Relaxed);
        self.inner.verification_count.store(0, Ordering::Relaxed);
        self.inner
            .total_execution_duration_ms
            .store(0, Ordering::Relaxed);
        self.inner
            .total_verification_duration_ms
            .store(0, Ordering::Relaxed);
        self.inner
            .total_recovery_duration_ms
            .store(0, Ordering::Relaxed);
    }
}

impl Default for SharedMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SharedMetrics {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[allow(dead_code)]
pub(crate) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_default() {
        let m = ExecutionMetrics::default();
        assert_eq!(m.total_goals, 0);
        assert_eq!(m.successful_goals, 0);
        assert_eq!(m.failed_goals, 0);
        assert_eq!(m.pipelines_executed, 0);
    }

    #[test]
    fn test_metrics_record_goal() {
        let mut m = ExecutionMetrics::default();
        m.record_goal_start();
        assert_eq!(m.total_goals, 1);
        m.record_goal_success();
        assert_eq!(m.successful_goals, 1);
    }

    #[test]
    fn test_metrics_record_goal_failure() {
        let mut m = ExecutionMetrics::default();
        m.record_goal_failure();
        assert_eq!(m.failed_goals, 1);
    }

    #[test]
    fn test_metrics_record_pipeline() {
        let mut m = ExecutionMetrics::default();
        m.record_pipeline();
        assert_eq!(m.pipelines_executed, 1);
    }

    #[test]
    fn test_metrics_record_step() {
        let mut m = ExecutionMetrics::default();
        m.record_step_completed();
        m.record_step_failed();
        m.record_step_skipped();
        assert_eq!(m.completed_steps, 1);
        assert_eq!(m.failed_steps, 1);
        assert_eq!(m.skipped_steps, 1);
    }

    #[test]
    fn test_metrics_record_retry_replan_recovery() {
        let mut m = ExecutionMetrics::default();
        m.record_retry();
        m.record_replan();
        m.record_recovery();
        assert_eq!(m.retries, 1);
        assert_eq!(m.replans, 1);
        assert_eq!(m.recoveries, 1);
    }

    #[test]
    fn test_metrics_record_verification() {
        let mut m = ExecutionMetrics::default();
        m.record_verification();
        assert_eq!(m.verification_count, 1);
    }

    #[test]
    fn test_metrics_record_durations() {
        let mut m = ExecutionMetrics::default();
        m.record_execution_duration(Duration::from_millis(1500));
        m.record_verification_duration(Duration::from_millis(200));
        m.record_recovery_duration(Duration::from_millis(300));
        assert_eq!(m.total_execution_duration_ms, 1500);
        assert_eq!(m.total_verification_duration_ms, 200);
        assert_eq!(m.total_recovery_duration_ms, 300);
    }

    #[test]
    fn test_metrics_reset() {
        let mut m = ExecutionMetrics::default();
        m.record_goal_start();
        m.record_goal_success();
        m.reset();
        assert_eq!(m.total_goals, 0);
        assert_eq!(m.successful_goals, 0);
    }

    #[test]
    fn test_metrics_snapshot() {
        let mut m = ExecutionMetrics::default();
        m.record_goal_start();
        m.record_step_completed();
        let snap = m.snapshot();
        assert_eq!(snap.total_goals, 1);
        assert_eq!(snap.completed_steps, 1);
        m.record_goal_success();
        assert_eq!(snap.total_goals, 1);
    }

    #[test]
    fn test_metrics_merge() {
        let mut a = ExecutionMetrics::default();
        a.record_goal_start();
        a.record_step_completed();

        let mut b = ExecutionMetrics::default();
        b.record_goal_start();
        b.record_step_completed();
        b.record_step_completed();

        a.merge(&b);
        assert_eq!(a.total_goals, 2);
        assert_eq!(a.completed_steps, 3);
    }

    #[test]
    fn test_metrics_averages_empty() {
        let m = ExecutionMetrics::default();
        assert_eq!(m.average_execution_duration_ms(), 0.0);
        assert_eq!(m.average_verification_duration_ms(), 0.0);
        assert_eq!(m.average_recovery_duration_ms(), 0.0);
    }

    #[test]
    fn test_metrics_averages() {
        let m = ExecutionMetrics {
            total_goals: 2,
            total_execution_duration_ms: 2000,
            verification_count: 4,
            total_verification_duration_ms: 400,
            recoveries: 2,
            total_recovery_duration_ms: 600,
            ..ExecutionMetrics::default()
        };
        assert!((m.average_execution_duration_ms() - 1000.0).abs() < 0.001);
        assert!((m.average_verification_duration_ms() - 100.0).abs() < 0.001);
        assert!((m.average_recovery_duration_ms() - 300.0).abs() < 0.001);
    }

    #[test]
    fn test_shared_metrics_record() {
        let sm = SharedMetrics::new();
        sm.record_goal_start();
        sm.record_goal_success();
        sm.record_step_completed();
        sm.record_retry();
        sm.record_verification();
        let snap = sm.snapshot();
        assert_eq!(snap.total_goals, 1);
        assert_eq!(snap.successful_goals, 1);
        assert_eq!(snap.completed_steps, 1);
        assert_eq!(snap.retries, 1);
        assert_eq!(snap.verification_count, 1);
    }

    #[test]
    fn test_shared_metrics_reset() {
        let sm = SharedMetrics::new();
        sm.record_goal_start();
        sm.record_pipeline();
        sm.reset();
        let snap = sm.snapshot();
        assert_eq!(snap.total_goals, 0);
        assert_eq!(snap.pipelines_executed, 0);
    }

    #[test]
    fn test_shared_metrics_durations() {
        let sm = SharedMetrics::new();
        sm.record_execution_duration(Duration::from_millis(500));
        sm.record_verification_duration(Duration::from_millis(50));
        sm.record_recovery_duration(Duration::from_millis(75));
        let snap = sm.snapshot();
        assert_eq!(snap.total_execution_duration_ms, 500);
        assert_eq!(snap.total_verification_duration_ms, 50);
        assert_eq!(snap.total_recovery_duration_ms, 75);
    }

    #[test]
    fn test_shared_metrics_concurrent() {
        let sm = Arc::new(SharedMetrics::new());
        let mut handles = Vec::new();
        for _ in 0..10 {
            let s = sm.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    s.record_goal_start();
                    s.record_step_completed();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let snap = sm.snapshot();
        assert_eq!(snap.total_goals, 1000);
        assert_eq!(snap.completed_steps, 1000);
    }

    #[test]
    fn test_trace_creation() {
        let trace = ExecutionTrace {
            execution_id: "e1".into(),
            goal: "test goal".into(),
            started_at: now_millis(),
            completed_at: None,
            duration_ms: None,
            success: false,
            pipeline_traces: vec![],
            metrics: ExecutionMetrics::default(),
        };
        assert_eq!(trace.execution_id, "e1");
        assert!(trace.completed_at.is_none());
    }

    #[test]
    fn test_pipeline_trace() {
        let trace = PipelineTrace {
            pipeline_id: "p1".into(),
            goal: "test".into(),
            started_at: 1000,
            completed_at: Some(2000),
            duration_ms: Some(1000),
            success: true,
            step_traces: vec![],
        };
        assert_eq!(trace.duration_ms, Some(1000));
        assert!(trace.success);
    }

    #[test]
    fn test_step_trace() {
        let trace = StepTrace {
            step_id: "s1".into(),
            step_index: 0,
            description: "step 1".into(),
            started_at: 1000,
            completed_at: Some(1100),
            duration_ms: Some(100),
            success: true,
            attempts: 1,
            error: None,
            verification_trace: None,
            recovery_trace: None,
        };
        assert_eq!(trace.attempts, 1);
        assert!(trace.error.is_none());
    }

    #[test]
    fn test_verification_trace() {
        let trace = VerificationTrace {
            step_id: "s1".into(),
            started_at: 1000,
            completed_at: Some(1050),
            duration_ms: Some(50),
            passed: true,
            strategy: "NoVerification".into(),
            reason: None,
            evidence_count: 0,
        };
        assert!(trace.passed);
    }

    #[test]
    fn test_recovery_trace() {
        let trace = RecoveryTrace {
            step_id: "s1".into(),
            started_at: 1000,
            completed_at: Some(1100),
            duration_ms: Some(100),
            decision: "retry".into(),
            strategy: "immediate_retry".into(),
            reason: "timeout".into(),
            retry_count: 1,
            success: true,
        };
        assert_eq!(trace.decision, "retry");
        assert!(trace.success);
    }

    #[test]
    fn test_execution_trace_with_pipeline() {
        let step = StepTrace {
            step_id: "s1".into(),
            step_index: 0,
            description: "click".into(),
            started_at: 100,
            completed_at: Some(200),
            duration_ms: Some(100),
            success: true,
            attempts: 1,
            error: None,
            verification_trace: None,
            recovery_trace: None,
        };
        let pipeline = PipelineTrace {
            pipeline_id: "p1".into(),
            goal: "test".into(),
            started_at: 100,
            completed_at: Some(300),
            duration_ms: Some(200),
            success: true,
            step_traces: vec![step],
        };
        let exec = ExecutionTrace {
            execution_id: "e1".into(),
            goal: "test".into(),
            started_at: 100,
            completed_at: Some(400),
            duration_ms: Some(300),
            success: true,
            pipeline_traces: vec![pipeline],
            metrics: ExecutionMetrics::default(),
        };
        assert_eq!(exec.pipeline_traces.len(), 1);
        assert_eq!(exec.pipeline_traces[0].step_traces.len(), 1);
    }

    #[test]
    fn test_metrics_serialization() {
        let mut m = ExecutionMetrics::default();
        m.record_goal_start();
        m.record_step_completed();
        let json = serde_json::to_string(&m).unwrap();
        let deserialized: ExecutionMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_goals, 1);
        assert_eq!(deserialized.completed_steps, 1);
    }

    #[test]
    fn test_trace_serialization() {
        let trace = VerificationTrace {
            step_id: "s1".into(),
            started_at: 0,
            completed_at: Some(50),
            duration_ms: Some(50),
            passed: true,
            strategy: "ocr".into(),
            reason: None,
            evidence_count: 1,
        };
        let json = serde_json::to_string(&trace).unwrap();
        let deserialized: VerificationTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.step_id, "s1");
        assert!(deserialized.passed);
    }

    #[test]
    fn test_now_millis_positive() {
        let t = now_millis();
        assert!(t > 1_700_000_000_000i64);
    }

    #[test]
    fn test_metrics_clone() {
        let mut m = ExecutionMetrics::default();
        m.record_goal_start();
        let cloned = m.clone();
        assert_eq!(cloned.total_goals, 1);
        m.record_goal_start();
        assert_eq!(m.total_goals, 2);
        assert_eq!(cloned.total_goals, 1);
    }

    #[test]
    fn test_shared_metrics_clone_is_same_inner() {
        let sm = SharedMetrics::new();
        sm.record_goal_start();
        let cloned = sm.clone();
        cloned.record_goal_success();
        let snap = sm.snapshot();
        assert_eq!(snap.total_goals, 1);
        assert_eq!(snap.successful_goals, 1);
    }

    #[test]
    fn test_metrics_merge_with_empty() {
        let mut a = ExecutionMetrics::default();
        a.record_goal_start();
        a.record_step_completed();
        let empty = ExecutionMetrics::default();
        a.merge(&empty);
        assert_eq!(a.total_goals, 1);
        assert_eq!(a.completed_steps, 1);
    }

    #[test]
    fn test_metrics_all_records() {
        let mut m = ExecutionMetrics::default();
        m.record_goal_start();
        m.record_goal_success();
        m.record_goal_failure();
        m.record_pipeline();
        m.record_step_completed();
        m.record_step_failed();
        m.record_step_skipped();
        m.record_retry();
        m.record_replan();
        m.record_recovery();
        m.record_verification();
        m.record_execution_duration(Duration::from_millis(100));
        m.record_verification_duration(Duration::from_millis(10));
        m.record_recovery_duration(Duration::from_millis(5));
        assert_eq!(m.total_goals, 1);
        assert_eq!(m.successful_goals, 1);
        assert_eq!(m.failed_goals, 1);
        assert_eq!(m.pipelines_executed, 1);
        assert_eq!(m.completed_steps, 1);
        assert_eq!(m.failed_steps, 1);
        assert_eq!(m.skipped_steps, 1);
        assert_eq!(m.retries, 1);
        assert_eq!(m.replans, 1);
        assert_eq!(m.recoveries, 1);
        assert_eq!(m.verification_count, 1);
        assert_eq!(m.total_execution_duration_ms, 100);
        assert_eq!(m.total_verification_duration_ms, 10);
        assert_eq!(m.total_recovery_duration_ms, 5);
    }
}
