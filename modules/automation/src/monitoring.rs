use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::resource_manager::ResourceMetrics;

#[derive(Debug, Clone, Default)]
pub struct SchedulerStats {
    pub total_ticks: u64,
    pub failed_ticks: u64,
    pub successful_ticks: u64,
    pub last_tick_duration_ms: u64,
    pub avg_tick_duration_ms: f64,
    pub max_tick_duration_ms: u64,
    pub min_tick_duration_ms: u64,
    pub total_sessions_scheduled: u64,
    pub total_sessions_completed: u64,
    pub total_sessions_failed: u64,
    pub queue_depth: usize,
    pub is_running: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeHealth {
    Healthy,
    Degraded,
    Busy,
    Recovering,
    Failed,
}

impl std::fmt::Display for RuntimeHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeHealth::Healthy => write!(f, "healthy"),
            RuntimeHealth::Degraded => write!(f, "degraded"),
            RuntimeHealth::Busy => write!(f, "busy"),
            RuntimeHealth::Recovering => write!(f, "recovering"),
            RuntimeHealth::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeDiagnostics {
    pub uptime: Duration,
    pub health: RuntimeHealth,
    pub total_sessions_created: u64,
    pub active_sessions: usize,
    pub total_errors: u64,
    pub session_completion_rate: f64,
    pub avg_session_duration_ms: f64,
    pub resource_utilization: f64,
    pub event_counts: EventCounts,
    pub queue_depth: usize,
}

#[derive(Debug, Clone, Default)]
pub struct EventCounts {
    pub total_events_published: u64,
    pub events_by_type: Vec<(String, u64)>,
}

#[derive(Debug)]
pub struct RuntimeMonitor {
    started_at: Instant,
    total_sessions_created: AtomicU64,
    total_errors: AtomicU64,
    total_events_published: AtomicU64,
    event_type_counts: std::sync::Mutex<std::collections::HashMap<String, u64>>,
}

impl RuntimeMonitor {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            total_sessions_created: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            total_events_published: AtomicU64::new(0),
            event_type_counts: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn record_session_created(&self) {
        self.total_sessions_created.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_event(&self, variant_name: &str) {
        self.total_events_published.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut counts) = self.event_type_counts.lock() {
            *counts.entry(variant_name.to_string()).or_insert(0) += 1;
        }
    }

    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn total_sessions_created(&self) -> u64 {
        self.total_sessions_created.load(Ordering::Relaxed)
    }

    pub fn total_errors(&self) -> u64 {
        self.total_errors.load(Ordering::Relaxed)
    }

    pub fn total_events_published(&self) -> u64 {
        self.total_events_published.load(Ordering::Relaxed)
    }

    pub fn event_counts(&self) -> EventCounts {
        let events_by_type = if let Ok(counts) = self.event_type_counts.lock() {
            let mut v: Vec<(String, u64)> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
            v.sort_by_key(|b| std::cmp::Reverse(b.1));
            v
        } else {
            Vec::new()
        };
        EventCounts {
            total_events_published: self.total_events_published(),
            events_by_type,
        }
    }

    pub fn diagnostics(
        &self,
        active_sessions: usize,
        completed_sessions: u64,
        total_session_duration_ms: f64,
        resource_utilization: f64,
        queue_depth: usize,
    ) -> RuntimeDiagnostics {
        let total_sessions = self.total_sessions_created();
        let total_created = total_sessions.max(1);
        let session_completion_rate = (completed_sessions as f64) / (total_created as f64) * 100.0;
        let avg_session_duration_ms = if completed_sessions > 0 {
            total_session_duration_ms / completed_sessions as f64
        } else {
            0.0
        };

        let error_rate = self.total_errors();
        let health = if error_rate > 100 {
            RuntimeHealth::Failed
        } else if error_rate > 50 {
            RuntimeHealth::Degraded
        } else if active_sessions > 20 {
            RuntimeHealth::Busy
        } else {
            RuntimeHealth::Healthy
        };

        RuntimeDiagnostics {
            uptime: self.uptime(),
            health,
            total_sessions_created: total_sessions,
            active_sessions,
            total_errors: error_rate,
            session_completion_rate,
            avg_session_duration_ms,
            resource_utilization,
            event_counts: self.event_counts(),
            queue_depth,
        }
    }
}

impl Default for RuntimeMonitor {
    fn default() -> Self {
        Self::new()
    }
}

pub fn assess_health(
    active_sessions: usize,
    error_count: u64,
    resource_metrics: Option<&ResourceMetrics>,
    scheduler_stats: Option<&SchedulerStats>,
) -> RuntimeHealth {
    if error_count > 100 {
        return RuntimeHealth::Failed;
    }
    if error_count > 50 {
        return RuntimeHealth::Degraded;
    }
    if active_sessions > 20 {
        return RuntimeHealth::Busy;
    }
    if let Some(rm) = resource_metrics {
        if rm.total_acquisitions > 0
            && rm.acquisition_failures as f64 / rm.total_acquisitions as f64 > 0.3
        {
            return RuntimeHealth::Degraded;
        }
    }
    if let Some(ss) = scheduler_stats {
        if ss.total_ticks > 10 && ss.failed_ticks as f64 / ss.total_ticks as f64 > 0.3 {
            return RuntimeHealth::Degraded;
        }
    }
    RuntimeHealth::Healthy
}

pub fn format_duration(d: &Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m {}s", secs / 3600, (secs % 3600) / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    pub fn make_resource_metrics(acquires: u64, failures: u64) -> ResourceMetrics {
        ResourceMetrics {
            total_acquisitions: acquires,
            total_releases: 0,
            acquisition_failures: failures,
            current_locks_held: 0,
            peak_locks_held: 0,
            total_wait_time_ms: 0,
            total_contentions: 0,
            peak_contention: 0,
            forced_releases: 0,
            timeouts: 0,
            deadlock_detections: 0,
        }
    }

    #[test]
    fn test_runtime_health_display() {
        assert_eq!(format!("{}", RuntimeHealth::Healthy), "healthy");
        assert_eq!(format!("{}", RuntimeHealth::Degraded), "degraded");
        assert_eq!(format!("{}", RuntimeHealth::Busy), "busy");
        assert_eq!(format!("{}", RuntimeHealth::Recovering), "recovering");
        assert_eq!(format!("{}", RuntimeHealth::Failed), "failed");
    }

    #[test]
    fn test_health_assess_healthy() {
        let health = assess_health(1, 0, None, None);
        assert_eq!(health, RuntimeHealth::Healthy);
    }

    #[test]
    fn test_health_assess_busy() {
        let health = assess_health(25, 0, None, None);
        assert_eq!(health, RuntimeHealth::Busy);
    }

    #[test]
    fn test_health_assess_degraded_errors() {
        let health = assess_health(1, 60, None, None);
        assert_eq!(health, RuntimeHealth::Degraded);
    }

    #[test]
    fn test_health_assess_failed() {
        let health = assess_health(1, 150, None, None);
        assert_eq!(health, RuntimeHealth::Failed);
    }

    #[test]
    fn test_health_assess_resource_degraded() {
        let rm = make_resource_metrics(100, 40);
        let health = assess_health(1, 0, Some(&rm), None);
        assert_eq!(health, RuntimeHealth::Degraded);
    }

    #[test]
    fn test_health_assess_resource_ok() {
        let rm = make_resource_metrics(100, 10);
        let health = assess_health(1, 0, Some(&rm), None);
        assert_eq!(health, RuntimeHealth::Healthy);
    }

    #[test]
    fn test_health_assess_no_resource_metrics() {
        let rm = make_resource_metrics(0, 0);
        let health = assess_health(1, 0, Some(&rm), None);
        assert_eq!(health, RuntimeHealth::Healthy);
    }

    #[test]
    fn test_runtime_monitor_new() {
        let m = RuntimeMonitor::new();
        assert_eq!(m.total_sessions_created(), 0);
        assert_eq!(m.total_errors(), 0);
        assert_eq!(m.total_events_published(), 0);
    }

    #[test]
    fn test_runtime_monitor_record_session() {
        let m = RuntimeMonitor::new();
        m.record_session_created();
        assert_eq!(m.total_sessions_created(), 1);
        m.record_session_created();
        assert_eq!(m.total_sessions_created(), 2);
    }

    #[test]
    fn test_runtime_monitor_record_error() {
        let m = RuntimeMonitor::new();
        m.record_error();
        assert_eq!(m.total_errors(), 1);
    }

    #[test]
    fn test_runtime_monitor_record_event() {
        let m = RuntimeMonitor::new();
        m.record_event("SessionCreated");
        m.record_event("SessionCreated");
        m.record_event("RuntimeStarted");
        assert_eq!(m.total_events_published(), 3);
        let counts = m.event_counts();
        assert_eq!(counts.total_events_published, 3);
        assert!(counts.events_by_type.len() >= 2);
    }

    #[test]
    fn test_runtime_monitor_event_counts_empty() {
        let m = RuntimeMonitor::new();
        let counts = m.event_counts();
        assert_eq!(counts.total_events_published, 0);
        assert!(counts.events_by_type.is_empty());
    }

    #[test]
    fn test_runtime_monitor_event_counts_ordering() {
        let m = RuntimeMonitor::new();
        m.record_event("A");
        m.record_event("A");
        m.record_event("B");
        let counts = m.event_counts();
        assert_eq!(counts.events_by_type[0].1, 2);
    }

    #[test]
    fn test_runtime_monitor_uptime() {
        let m = RuntimeMonitor::new();
        thread::sleep(Duration::from_millis(10));
        let uptime = m.uptime();
        assert!(uptime >= Duration::from_millis(10));
    }

    #[test]
    fn test_diagnostics_healthy() {
        let m = RuntimeMonitor::new();
        let d = m.diagnostics(1, 0, 0.0, 0.5, 0);
        assert_eq!(d.health, RuntimeHealth::Healthy);
        assert_eq!(d.active_sessions, 1);
        assert_eq!(d.total_sessions_created, 0);
        assert_eq!(d.session_completion_rate, 0.0);
    }

    #[test]
    fn test_diagnostics_with_sessions() {
        let m = RuntimeMonitor::new();
        m.record_session_created();
        m.record_session_created();
        let d = m.diagnostics(1, 1, 2000.0, 0.5, 2);
        assert_eq!(d.total_sessions_created, 2);
        assert!((d.session_completion_rate - 50.0).abs() < 0.01);
        assert!((d.avg_session_duration_ms - 2000.0).abs() < 0.01);
        assert!((d.resource_utilization - 0.5).abs() < 0.01);
        assert_eq!(d.queue_depth, 2);
    }

    #[test]
    fn test_diagnostics_no_completed_sessions() {
        let m = RuntimeMonitor::new();
        let d = m.diagnostics(0, 0, 0.0, 0.0, 0);
        assert_eq!(d.avg_session_duration_ms, 0.0);
        assert_eq!(d.active_sessions, 0);
    }

    #[test]
    fn test_diagnostics_failed_health() {
        let m = RuntimeMonitor::new();
        for _ in 0..101 {
            m.record_error();
        }
        let d = m.diagnostics(0, 0, 0.0, 0.0, 0);
        assert_eq!(d.health, RuntimeHealth::Failed);
    }

    #[test]
    fn test_diagnostics_degraded_health() {
        let m = RuntimeMonitor::new();
        for _ in 0..51 {
            m.record_error();
        }
        let d = m.diagnostics(0, 0, 0.0, 0.0, 0);
        assert_eq!(d.health, RuntimeHealth::Degraded);
    }

    #[test]
    fn test_diagnostics_busy_health() {
        let m = RuntimeMonitor::new();
        let d = m.diagnostics(21, 0, 0.0, 0.0, 0);
        assert_eq!(d.health, RuntimeHealth::Busy);
    }

    #[test]
    fn test_diagnostics_includes_event_counts() {
        let m = RuntimeMonitor::new();
        m.record_event("RuntimeStarted");
        let d = m.diagnostics(0, 0, 0.0, 0.0, 0);
        assert_eq!(d.event_counts.total_events_published, 1);
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(&Duration::from_secs(30)), "30s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(&Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(&Duration::from_secs(3725)), "1h 2m 5s");
    }

    #[test]
    fn test_format_duration_zero() {
        assert_eq!(format_duration(&Duration::from_secs(0)), "0s");
    }

    #[test]
    fn test_runtime_monitor_default() {
        let m = RuntimeMonitor::default();
        assert_eq!(m.total_sessions_created(), 0);
    }

    #[test]
    fn test_health_assess_scheduler_degraded() {
        let ss = SchedulerStats {
            total_ticks: 20,
            failed_ticks: 10,
            successful_ticks: 10,
            last_tick_duration_ms: 0,
            avg_tick_duration_ms: 0.0,
            max_tick_duration_ms: 0,
            min_tick_duration_ms: 0,
            total_sessions_scheduled: 0,
            total_sessions_completed: 0,
            total_sessions_failed: 0,
            queue_depth: 0,
            is_running: true,
        };
        let health = assess_health(1, 0, None, Some(&ss));
        assert_eq!(health, RuntimeHealth::Degraded);
    }

    #[test]
    fn test_health_assess_scheduler_ok() {
        let ss = SchedulerStats {
            total_ticks: 20,
            failed_ticks: 2,
            successful_ticks: 18,
            last_tick_duration_ms: 0,
            avg_tick_duration_ms: 0.0,
            max_tick_duration_ms: 0,
            min_tick_duration_ms: 0,
            total_sessions_scheduled: 0,
            total_sessions_completed: 0,
            total_sessions_failed: 0,
            queue_depth: 0,
            is_running: true,
        };
        let health = assess_health(1, 0, None, Some(&ss));
        assert_eq!(health, RuntimeHealth::Healthy);
    }

    #[test]
    fn test_health_assess_scheduler_no_ticks() {
        let ss = SchedulerStats {
            total_ticks: 5,
            failed_ticks: 3,
            successful_ticks: 2,
            last_tick_duration_ms: 0,
            avg_tick_duration_ms: 0.0,
            max_tick_duration_ms: 0,
            min_tick_duration_ms: 0,
            total_sessions_scheduled: 0,
            total_sessions_completed: 0,
            total_sessions_failed: 0,
            queue_depth: 0,
            is_running: true,
        };
        let health = assess_health(1, 0, None, Some(&ss));
        assert_eq!(health, RuntimeHealth::Healthy);
    }
}
