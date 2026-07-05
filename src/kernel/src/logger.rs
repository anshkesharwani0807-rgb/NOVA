use parking_lot::RwLock;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tracing::{info, Level};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

fn log_file_path() -> &'static RwLock<Option<PathBuf>> {
    static CELL: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(None))
}

fn activity_trail() -> &'static Mutex<Vec<ActivityLog>> {
    static CELL: OnceLock<Mutex<Vec<ActivityLog>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(Vec::new()))
}

fn egress_log_store() -> &'static Mutex<Vec<EgressLog>> {
    static CELL: OnceLock<Mutex<Vec<EgressLog>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(Vec::new()))
}

/// Wraps a value so it prints as `[REDACTED]` in logs — use for PII fields.
pub struct Redacted<T>(pub T);

impl<T: fmt::Display> fmt::Display for Redacted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl<T: fmt::Debug> fmt::Debug for Redacted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Redacted(...)")
    }
}

/// A single entry in the user-facing Activity Trail (Principle 5 — transparency).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActivityLog {
    pub timestamp: String,
    pub module: String,
    pub action: String,
    pub reason: String,
    pub correlation_id: Option<uuid::Uuid>,
}

/// A single entry in the Egress Log (D3 — every network egress is logged and attributable).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EgressLog {
    pub timestamp: String,
    pub destination: String,
    pub purpose: String,
    pub data_size_bytes: usize,
    pub consent_granted: bool,
    pub correlation_id: Option<uuid::Uuid>,
}

/// Initialize the local-only structured logging system (ADR-0009).
/// Three planes: diagnostic (dev), activity trail (user-facing), egress log (user-facing).
/// No data ever leaves the device through this subsystem.
pub fn init_logger(log_dir: &Path) {
    if let Err(e) = fs::create_dir_all(log_dir) {
        eprintln!("NOVA: Failed to create log directory {:?}: {}", log_dir, e);
    }

    let log_path = log_dir.join("diagnostic.log");
    *log_file_path().write() = Some(log_path.clone());

    let file = OpenOptions::new().create(true).append(true).open(&log_path);

    let filter = EnvFilter::from_default_env().add_directive(Level::INFO.into());

    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_span_events(FmtSpan::CLOSE);

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer);

    if let Ok(file_handle) = file {
        let file_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(Mutex::new(file_handle));
        // Ignore error — subscriber may already be set in tests.
        let _ = registry.with(file_layer).try_init();
    } else {
        let _ = registry.try_init();
    }

    info!("NOVA logging initialized. Diagnostic log: {:?}", log_path);
}

/// Record a user-facing activity entry (Principle 5 — transparency over magic).
/// Call this for every material action NOVA takes so the user can inspect it.
pub fn log_activity(module: &str, action: &str, reason: &str, correlation_id: Option<uuid::Uuid>) {
    let now = chrono::Local::now().to_rfc3339();
    let entry = ActivityLog {
        timestamp: now,
        module: module.to_string(),
        action: action.to_string(),
        reason: reason.to_string(),
        correlation_id,
    };

    let mut trail = activity_trail().lock().unwrap();
    trail.push(entry);
    // Keep a rolling window in memory; persistent storage goes to the DB (Ch14).
    if trail.len() > 10_000 {
        trail.remove(0);
    }

    info!(
        target: "activity_trail",
        module = %module,
        action = %action,
        reason = %reason,
        correlation_id = ?correlation_id,
        "Activity: {}", action
    );
}

/// Record a network egress event (D3 — 100% of egress must be logged and attributable).
/// Call this in the Egress Gate before allowing any outbound network call.
pub fn log_egress(
    destination: &str,
    purpose: &str,
    data_size_bytes: usize,
    consent_granted: bool,
    correlation_id: Option<uuid::Uuid>,
) {
    let now = chrono::Local::now().to_rfc3339();
    let entry = EgressLog {
        timestamp: now,
        destination: destination.to_string(),
        purpose: purpose.to_string(),
        data_size_bytes,
        consent_granted,
        correlation_id,
    };

    let mut log = egress_log_store().lock().unwrap();
    log.push(entry);
    if log.len() > 10_000 {
        log.remove(0);
    }

    info!(
        target: "egress_log",
        destination = %destination,
        purpose = %purpose,
        data_size_bytes = data_size_bytes,
        consent_granted = consent_granted,
        correlation_id = ?correlation_id,
        "Egress: {} → {} (consent={})", purpose, destination, consent_granted
    );
}

/// Return a snapshot of the recent activity trail for the user to inspect.
pub fn get_recent_activity() -> Vec<ActivityLog> {
    activity_trail().lock().unwrap().clone()
}

/// Return a snapshot of the recent egress log for the user to inspect.
pub fn get_recent_egress() -> Vec<EgressLog> {
    egress_log_store().lock().unwrap().clone()
}
