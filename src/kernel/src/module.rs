//! Kernel-managed module system (Milestone 3).
//!
//! Defines the [`KernelModule`] contract every NOVA module implements, the lifecycle
//! states, per-module health reporting, and a thread-safe [`ModuleRegistry`] with
//! dependency resolution. All current and future modules plug in through this system
//! and obtain services only through the `Kernel` (dependency injection), never by
//! constructing peers directly. Inter-module calls go through the Event Bus (ADR-0004).

use crate::error::{ErrorCategory, NovaError, Result};
use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Coarse health status of a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// A module's health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleHealth {
    pub status: HealthStatus,
    pub detail: String,
}

impl ModuleHealth {
    pub fn healthy() -> Self {
        Self {
            status: HealthStatus::Healthy,
            detail: String::new(),
        }
    }

    pub fn degraded(detail: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Degraded,
            detail: detail.into(),
        }
    }

    pub fn unhealthy(detail: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            detail: detail.into(),
        }
    }
}

/// Lifecycle state of a registered module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleState {
    /// Registered, not yet initialized.
    Boot,
    /// `initialize()` completed.
    Initialized,
    /// Initialized and ready to start.
    Ready,
    /// `start()` completed; the module is running.
    Running,
    /// `stop()` in progress.
    Stopping,
    /// `shutdown()` completed.
    Shutdown,
}

/// The contract every NOVA module implements to be managed by the kernel.
#[async_trait]
pub trait KernelModule: Send + Sync {
    /// Stable, unique identifier (e.g. `"memory"`).
    fn module_id(&self) -> &'static str;

    /// Semantic version of the module.
    fn version(&self) -> &'static str;

    /// Ids of modules that must be initialized/started before this one.
    fn dependencies(&self) -> Vec<&'static str> {
        Vec::new()
    }

    /// Prepare resources. Called once, before [`KernelModule::start`].
    async fn initialize(&self) -> Result<()> {
        Ok(())
    }

    /// Begin running (spawn listeners, register request handlers).
    async fn start(&self) -> Result<()> {
        Ok(())
    }

    /// Stop running work, releasing transient resources.
    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    /// Final cleanup before removal.
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    /// Current health of the module.
    fn health(&self) -> ModuleHealth {
        ModuleHealth::healthy()
    }
}

/// A snapshot of a module's registry entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleStatus {
    pub id: String,
    pub version: String,
    pub state: LifecycleState,
    pub health: ModuleHealth,
    pub dependencies: Vec<String>,
}

/// Thread-safe registry of kernel modules with lifecycle and dependency management.
#[derive(Default)]
pub struct ModuleRegistry {
    modules: RwLock<HashMap<&'static str, Arc<dyn KernelModule>>>,
    states: RwLock<HashMap<&'static str, LifecycleState>>,
    order: RwLock<Vec<&'static str>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a module. Fails if a module with the same id already exists.
    pub fn register(&self, module: Arc<dyn KernelModule>) -> Result<()> {
        let id = module.module_id();
        let mut modules = self.modules.write();
        if modules.contains_key(id) {
            return Err(NovaError::new(
                ErrorCategory::Kernel,
                "ERR_MODULE_DUPLICATE",
                &format!("Module '{id}' is already registered"),
            ));
        }
        modules.insert(id, module);
        self.states.write().insert(id, LifecycleState::Boot);
        self.order.write().push(id);
        Ok(())
    }

    /// Remove a module. Fails if it is not registered.
    pub fn unregister(&self, id: &str) -> Result<()> {
        let mut modules = self.modules.write();
        if modules.remove(id).is_none() {
            return Err(NovaError::new(
                ErrorCategory::Kernel,
                "ERR_MODULE_NOT_FOUND",
                &format!("Module '{id}' is not registered"),
            ));
        }
        self.states.write().remove(id);
        self.order.write().retain(|m| *m != id);
        Ok(())
    }

    /// Look up a module by id.
    pub fn lookup(&self, id: &str) -> Option<Arc<dyn KernelModule>> {
        self.modules.read().get(id).cloned()
    }

    /// Whether a module id is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.modules.read().contains_key(id)
    }

    /// Number of registered modules.
    pub fn count(&self) -> usize {
        self.modules.read().len()
    }

    /// Current lifecycle state of a module.
    pub fn state(&self, id: &str) -> Option<LifecycleState> {
        self.states.read().get(id).copied()
    }

    /// List modules in registration order with status + health.
    pub fn list(&self) -> Vec<ModuleStatus> {
        let modules = self.modules.read();
        let states = self.states.read();
        self.order
            .read()
            .iter()
            .filter_map(|id| {
                modules.get(id).map(|m| ModuleStatus {
                    id: id.to_string(),
                    version: m.version().to_string(),
                    state: states.get(id).copied().unwrap_or(LifecycleState::Boot),
                    health: m.health(),
                    dependencies: m.dependencies().iter().map(|d| d.to_string()).collect(),
                })
            })
            .collect()
    }

    /// Health report for every registered module, in registration order.
    pub fn health_report(&self) -> Vec<(String, ModuleHealth)> {
        let modules = self.modules.read();
        self.order
            .read()
            .iter()
            .filter_map(|id| modules.get(id).map(|m| (id.to_string(), m.health())))
            .collect()
    }

    /// Resolve a start order honoring dependencies (topological sort).
    ///
    /// Errors on a missing dependency or a dependency cycle. Ties are broken by
    /// registration order for determinism.
    pub fn resolve_order(&self) -> Result<Vec<&'static str>> {
        let modules = self.modules.read();
        let ids: Vec<&'static str> = self.order.read().clone();

        // Collect (id, deps), validating that each dependency is registered.
        let mut remaining: Vec<(&'static str, Vec<&'static str>)> = Vec::with_capacity(ids.len());
        for &id in &ids {
            let deps = modules
                .get(id)
                .map(|m| m.dependencies())
                .unwrap_or_default();
            for &dep in &deps {
                if !modules.contains_key(dep) {
                    return Err(NovaError::new(
                        ErrorCategory::Kernel,
                        "ERR_MODULE_DEP_MISSING",
                        &format!("Module '{id}' depends on unregistered module '{dep}'"),
                    ));
                }
            }
            remaining.push((id, deps));
        }

        let mut resolved: Vec<&'static str> = Vec::new();
        while !remaining.is_empty() {
            let mut progressed = false;
            let mut still: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
            for (id, deps) in remaining.into_iter() {
                if deps.iter().all(|d| resolved.contains(d)) {
                    resolved.push(id);
                    progressed = true;
                } else {
                    still.push((id, deps));
                }
            }
            remaining = still;
            if !progressed {
                return Err(NovaError::new(
                    ErrorCategory::Kernel,
                    "ERR_MODULE_DEP_CYCLE",
                    "Module dependency cycle detected",
                ));
            }
        }
        Ok(resolved)
    }

    fn set_state(&self, id: &'static str, state: LifecycleState) {
        self.states.write().insert(id, state);
    }

    /// Clone the module `Arc`s for the given ids (releases the lock before any await).
    fn snapshot(&self, order: &[&'static str]) -> Vec<(&'static str, Arc<dyn KernelModule>)> {
        let modules = self.modules.read();
        order
            .iter()
            .filter_map(|id| modules.get(id).map(|m| (*id, m.clone())))
            .collect()
    }

    /// Initialize all modules in dependency order (Boot → Initialized → Ready).
    pub async fn initialize_all(&self) -> Result<()> {
        let order = self.resolve_order()?;
        for (id, module) in self.snapshot(&order) {
            module.initialize().await?;
            self.set_state(id, LifecycleState::Initialized);
        }
        for &id in &order {
            self.set_state(id, LifecycleState::Ready);
        }
        Ok(())
    }

    /// Start all modules in dependency order (Ready → Running).
    pub async fn start_all(&self) -> Result<()> {
        let order = self.resolve_order()?;
        for (id, module) in self.snapshot(&order) {
            module.start().await?;
            self.set_state(id, LifecycleState::Running);
        }
        Ok(())
    }

    /// Convenience: initialize then start (used at bootstrap).
    pub async fn bring_up(&self) -> Result<()> {
        self.initialize_all().await?;
        self.start_all().await
    }

    /// Stop all modules in reverse dependency order (Running → Stopping → Ready).
    pub async fn stop_all(&self) -> Result<()> {
        let order = self.resolve_order()?;
        for (id, module) in self.snapshot(&order).into_iter().rev() {
            self.set_state(id, LifecycleState::Stopping);
            module.stop().await?;
            self.set_state(id, LifecycleState::Ready);
        }
        Ok(())
    }

    /// Shut down all modules in reverse dependency order (→ Shutdown).
    pub async fn shutdown_all(&self) -> Result<()> {
        let order = self.resolve_order()?;
        for (id, module) in self.snapshot(&order).into_iter().rev() {
            module.shutdown().await?;
            self.set_state(id, LifecycleState::Shutdown);
        }
        Ok(())
    }

    /// Convenience: stop then shut down (reverse order).
    pub async fn tear_down(&self) -> Result<()> {
        self.stop_all().await?;
        self.shutdown_all().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy {
        id: &'static str,
        deps: Vec<&'static str>,
    }

    #[async_trait]
    impl KernelModule for Dummy {
        fn module_id(&self) -> &'static str {
            self.id
        }
        fn version(&self) -> &'static str {
            "1.0.0"
        }
        fn dependencies(&self) -> Vec<&'static str> {
            self.deps.clone()
        }
    }

    fn dummy(id: &'static str, deps: &[&'static str]) -> Arc<dyn KernelModule> {
        Arc::new(Dummy {
            id,
            deps: deps.to_vec(),
        })
    }

    #[test]
    fn register_lookup_and_count() {
        let reg = ModuleRegistry::new();
        reg.register(dummy("a", &[])).unwrap();
        assert_eq!(reg.count(), 1);
        assert!(reg.contains("a"));
        assert!(reg.lookup("a").is_some());
        assert_eq!(reg.state("a"), Some(LifecycleState::Boot));
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let reg = ModuleRegistry::new();
        reg.register(dummy("a", &[])).unwrap();
        assert!(reg.register(dummy("a", &[])).is_err());
    }

    #[test]
    fn unregister_missing_is_error() {
        let reg = ModuleRegistry::new();
        assert!(reg.unregister("nope").is_err());
    }

    #[test]
    fn resolve_order_respects_dependencies() {
        let reg = ModuleRegistry::new();
        // c depends on b, b depends on a → order must be a, b, c.
        reg.register(dummy("c", &["b"])).unwrap();
        reg.register(dummy("a", &[])).unwrap();
        reg.register(dummy("b", &["a"])).unwrap();
        let order = reg.resolve_order().unwrap();
        let pos = |x: &str| order.iter().position(|i| *i == x).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("b") < pos("c"));
    }

    #[test]
    fn missing_dependency_is_error() {
        let reg = ModuleRegistry::new();
        reg.register(dummy("a", &["ghost"])).unwrap();
        assert!(reg.resolve_order().is_err());
    }

    #[test]
    fn dependency_cycle_is_error() {
        let reg = ModuleRegistry::new();
        reg.register(dummy("a", &["b"])).unwrap();
        reg.register(dummy("b", &["a"])).unwrap();
        assert!(reg.resolve_order().is_err());
    }
}
