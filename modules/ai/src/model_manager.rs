//! Model manager for the AI Runtime (Milestone 6).
//!
//! Holds the registered [`InferenceProvider`]s, tracks the active one, and provides a
//! full lifecycle (load/unload/reload) over each provider. Lazy loading is the default
//! behavior, but explicit load/unload enables offline-first control and deterministic
//! cleanup (Principle 7 - longevity). Each provider manages its own resources.
//!
//! Providers are registered by the composition root, so no backend is hard-coded.

use crate::provider::{InferenceProvider, ModelDescriptor};
use nova_kernel::{ErrorCategory, NovaError, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Registers providers and resolves the active model.
#[derive(Default)]
pub struct ModelManager {
    providers: RwLock<HashMap<String, Arc<dyn InferenceProvider>>>,
    active: RwLock<Option<String>>,
}

impl ModelManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a provider. The first registered provider becomes active by default.
    pub fn register(&self, provider: Arc<dyn InferenceProvider>) -> Result<()> {
        let id = provider.id().to_string();
        let mut providers = self.providers.write();
        if providers.contains_key(&id) {
            return Err(NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_MODEL_DUPLICATE",
                &format!("Model provider '{id}' is already registered"),
            ));
        }
        providers.insert(id.clone(), provider);
        let mut active = self.active.write();
        if active.is_none() {
            *active = Some(id);
        }
        Ok(())
    }

    /// Select the active model by id. Errors if the id is not registered.
    pub fn set_active(&self, id: &str) -> Result<()> {
        if !self.providers.read().contains_key(id) {
            return Err(NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_MODEL_NOT_FOUND",
                &format!("Model provider '{id}' is not registered"),
            ));
        }
        *self.active.write() = Some(id.to_string());
        Ok(())
    }

    pub fn active_id(&self) -> Option<String> {
        self.active.read().clone()
    }

    pub fn count(&self) -> usize {
        self.providers.read().len()
    }

    /// Descriptors for all registered models (sorted by id for determinism).
    pub fn list(&self) -> Vec<ModelDescriptor> {
        let mut list: Vec<ModelDescriptor> = self
            .providers
            .read()
            .values()
            .map(|p| p.describe())
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// Resolve the active provider, lazily loading it if not already in memory.
    pub async fn active(&self) -> Result<Arc<dyn InferenceProvider>> {
        let id = self.active_id().ok_or_else(|| {
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_NO_ACTIVE_MODEL",
                "No active model is set (register a provider first)",
            )
        })?;
        let provider = self.providers.read().get(&id).cloned().ok_or_else(|| {
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_MODEL_NOT_FOUND",
                &format!("Active model '{id}' is no longer registered"),
            )
        })?;
        if !provider.is_loaded() {
            nova_kernel::log_activity(
                "ai",
                "ai.model_loading",
                &format!("loading model '{id}'"),
                None,
            );
            provider.load().await?;
        }
        Ok(provider)
    }

    /// Explicitly load a model provider by ID (overrides lazy loading).
    ///
    /// Useful for UI scenarios where the user wants to prepare a model ahead of time
    /// or to report load status. Will error if the model is already loaded.
    pub async fn load_provider(&self, id: &str) -> Result<()> {
        let provider = self.providers.read().get(id).cloned().ok_or_else(|| {
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_MODEL_NOT_FOUND",
                &format!("Model provider '{id}' is not registered"),
            )
        })?;
        if provider.is_loaded() {
            return Ok(()); // Already loaded
        }
        nova_kernel::log_activity(
            "ai",
            "ai.model_load_explicit",
            &format!("explicit load of model '{id}'"),
            None,
        );
        provider.load().await
    }

    /// Explicitly unload a model provider by ID, freeing associated resources.
    ///
    /// This is a safety operation that allows the system to reclaim memory
    /// and supports the principle of optional cloud acceleration (offline-first).
    pub async fn unload_provider(&self, id: &str) -> Result<()> {
        let provider = self.providers.read().get(id).cloned().ok_or_else(|| {
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_MODEL_NOT_FOUND",
                &format!("Model provider '{id}' is not registered"),
            )
        })?;
        if !provider.is_loaded() {
            return Ok(()); // Already unloaded
        }
        provider.unload().await
    }

    /// Reload a model provider, forcing re-initialization.
    ///
    /// Used when configuration changes or to refresh a model (e.g., newer weights).
    pub async fn reload_provider(&self, id: &str) -> Result<()> {
        if !self.providers.read().contains_key(id) {
            return Err(NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_MODEL_NOT_FOUND",
                &format!("Model provider '{id}' is not registered"),
            ));
        }
        // Temporarily unload, then load again.
        self.unload_provider(id).await?;
        self.load_provider(id).await
    }

    /// Wait for a provider to reach a specific state (loaded/unloaded) with timeout.
    ///
    /// This is useful for UI feedback during model loading.
    pub async fn wait_for_provider_state(
        &self,
        id: &str,
        target_loaded: bool,
        timeout_ms: u64,
    ) -> Result<()> {
        let start = std::time::Instant::now();
        loop {
            let current_loaded = {
                let guard = self.providers.read();
                match guard.get(id) {
                    Some(p) => p.is_loaded(),
                    None => {
                        return Err(NovaError::new(
                            ErrorCategory::Internal,
                            "ERR_AI_MODEL_NOT_FOUND",
                            &format!("Model provider '{id}' is not registered"),
                        ))
                    }
                }
            };
            if current_loaded == target_loaded {
                return Ok(());
            }
            if start.elapsed().as_millis() > timeout_ms as u128 {
                return Err(NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_MODEL_LOAD_TIMEOUT",
                    &format!(
                        "Timeout waiting for model '{id}' to become {}",
                        if target_loaded { "loaded" } else { "unloaded" }
                    ),
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    /// Obtain the current load state of a model provider.
    ///
    /// Returns `Some(true)` if loaded, `Some(false)` if unloaded, `None` if not registered.
    pub fn provider_state(&self, id: &str) -> Option<bool> {
        self.providers.read().get(id).map(|p| p.is_loaded())
    }

    /// List all models with their current load state.
    ///
    /// Useful for UI model selection screens and diagnostics.
    pub fn list_with_state(&self) -> Vec<(ModelDescriptor, bool)> {
        self.providers
            .read()
            .values()
            .map(|p| {
                let desc = p.describe();
                (desc, p.is_loaded())
            })
            .collect()
    }
}
