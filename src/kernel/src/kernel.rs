use crate::consent::ConsentManager;
use crate::egress::{EgressGate, EgressPolicy};
use crate::error::{ErrorCategory, NovaError, Result};
use crate::event_bus::EventBus;
use crate::module::ModuleRegistry;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;

pub struct Kernel {
    pub event_bus: Arc<EventBus>,
    /// Consent Manager — records and evaluates user consent (Milestone 2, D8).
    pub consent: Arc<ConsentManager>,
    /// Egress Gate — the single chokepoint for all outbound interactions (Milestone 2, D3).
    pub egress_gate: Arc<EgressGate>,
    /// Module Registry — kernel-managed module system with lifecycle + DI (Milestone 3).
    pub registry: Arc<ModuleRegistry>,
    pub config_dir: PathBuf,
    pub log_dir: PathBuf,
}

static KERNEL_INSTANCE: OnceLock<Arc<Kernel>> = OnceLock::new();

impl Kernel {
    /// Bootstraps the NOVA Kernel, initializing logging, configuration, and the event bus.
    pub fn bootstrap(config_dir: &Path, log_dir: &Path) -> Result<Arc<Self>> {
        // 1. Initialize Logger
        crate::logger::init_logger(log_dir);

        // 2. Load Configuration
        let config = crate::config::load_config_from_dir(config_dir)?;

        // 3. Create Event Bus
        let event_bus = Arc::new(EventBus::new(1024));

        // 4. Create the Consent Manager and Egress Gate (Milestone 2).
        //    The initial egress policy honours the privacy-first config default: unless
        //    the user has enabled remote acceleration, the device starts fully offline.
        let consent = Arc::new(ConsentManager::new());
        let initial_policy = if config.privacy.allow_remote_acceleration {
            EgressPolicy::InternetAllowed
        } else {
            EgressPolicy::OfflineOnly
        };
        let egress_gate = Arc::new(EgressGate::new(consent.clone(), initial_policy));

        // 5. Create the Module Registry (Milestone 3). Modules are registered by the
        //    composition root (the app / FFI) after bootstrap, then driven through the
        //    registry's lifecycle. The kernel crate cannot depend on module crates, so
        //    it owns the registry but not the concrete modules.
        let registry = Arc::new(ModuleRegistry::new());

        let kernel = Arc::new(Self {
            event_bus,
            consent,
            egress_gate,
            registry,
            config_dir: config_dir.to_path_buf(),
            log_dir: log_dir.to_path_buf(),
        });

        if KERNEL_INSTANCE.set(kernel.clone()).is_err() {
            return Err(NovaError::new(
                ErrorCategory::Kernel,
                "ERR_KERNEL_ALREADY_BOOTSTRAPPED",
                "Kernel bootstrap was called multiple times",
            ));
        }

        tracing::info!("NOVA Kernel bootstrapped successfully.");
        Ok(kernel)
    }

    /// Retrieve the singleton instance of the bootstrapped Kernel
    pub fn instance() -> Result<Arc<Self>> {
        KERNEL_INSTANCE.get().cloned().ok_or_else(|| {
            NovaError::new(
                ErrorCategory::Kernel,
                "ERR_KERNEL_NOT_BOOTSTRAPPED",
                "Kernel is not bootstrapped. Call Kernel::bootstrap() first.",
            )
        })
    }

    /// Triggers shutdown sequence for the kernel and all registered modules
    pub fn shutdown(&self) {
        tracing::info!("NOVA Kernel shutdown requested. Cleaning up resources...");
        // Here we would signal any cancel tokens, shut down background worker threads, etc.
    }
}
