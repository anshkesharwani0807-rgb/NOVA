//! # NOVA Kernel
//!
//! The microkernel is the foundational layer of the NOVA AI Operating Platform.
//! It owns: lifecycle, the internal Event Bus, layered Configuration, structured Logging,
//! structured Errors, and the two mandatory security gates (Consent Gate + Egress Gate).
//!
//! All inter-module communication goes through the Event Bus — modules never call each
//! other's internals directly (ADR-0004).  All network egress passes through the Egress
//! Gate and is logged (D3, ADR-0009).

pub mod config;
pub mod consent;
pub mod egress;
pub mod error;
pub mod event_bus;
pub mod kernel;
pub mod logger;
pub mod module;

// ── Error handling ────────────────────────────────────────────────────────────
pub use error::{ErrorCategory, NovaError, Result};

// ── Logging & observability ───────────────────────────────────────────────────
pub use logger::{
    get_recent_activity, get_recent_egress, init_logger, log_activity, log_egress, ActivityLog,
    EgressLog, Redacted,
};

// ── Configuration ─────────────────────────────────────────────────────────────
pub use config::{
    get_config, load_config_from_dir, update_config, AutomationConfig, MemoryConfig, NovaConfig,
    PrivacyConfig, SystemConfig,
};

// ── Event Bus ─────────────────────────────────────────────────────────────────
pub use event_bus::{EventBus, EventMetadata, NovaEvent, NovaRequest, NovaResponse};

// ── Consent & Egress (Milestone 2) ────────────────────────────────────────────
pub use consent::{
    ConsentGrant, ConsentManager, ConsentResolution, ConsentState, GrantSource, RequestKind,
};
pub use egress::{
    DestinationScope, EgressDecision, EgressGate, EgressOutcome, EgressPolicy, EgressRequest,
};

// ── Module system (Milestone 3) ───────────────────────────────────────────────
pub use module::{
    HealthStatus, KernelModule, LifecycleState, ModuleHealth, ModuleRegistry, ModuleStatus,
};

// ── Kernel lifecycle ──────────────────────────────────────────────────────────
pub use kernel::Kernel;
