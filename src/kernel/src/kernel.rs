use crate::error::{ErrorCategory, NovaError, Result};
use crate::event_bus::EventBus;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;

pub struct Kernel {
    pub event_bus: Arc<EventBus>,
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
        let _config = crate::config::load_config_from_dir(config_dir)?;

        // 3. Create Event Bus
        let event_bus = Arc::new(EventBus::new(1024));

        let kernel = Arc::new(Self {
            event_bus,
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
