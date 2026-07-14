use nova_kernel::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::plugin_error;
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    Installed,
    Enabled,
    Disabled,
    Unloaded,
    Error,
}

#[derive(Clone)]
pub struct PluginEntry {
    pub manifest: PluginManifest,
    pub state: PluginState,
    pub health: String,
}

pub struct PluginRegistry {
    entries: RwLock<HashMap<String, PluginEntry>>,
    instances: RwLock<HashMap<String, Arc<dyn Plugin>>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            instances: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, id: &str, plugin: Arc<dyn Plugin>) -> Result<()> {
        let mut entries = self.entries.write();
        if entries.contains_key(id) {
            return Err(plugin_error(
                "ERR_PLUGIN_DUPLICATE",
                &format!("Plugin '{}' is already registered", id),
            ));
        }
        let manifest = plugin.manifest().clone();
        manifest.validate()?;
        entries.insert(
            id.to_string(),
            PluginEntry {
                manifest,
                state: PluginState::Installed,
                health: "healthy".to_string(),
            },
        );
        self.instances.write().insert(id.to_string(), plugin);
        Ok(())
    }

    pub fn unregister(&self, id: &str) -> Result<()> {
        let mut entries = self.entries.write();
        if entries.remove(id).is_none() {
            return Err(plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' is not registered", id),
            ));
        }
        self.instances.write().remove(id);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<PluginEntry> {
        self.entries.read().get(id).cloned()
    }

    pub fn instance(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        self.instances.read().get(id).cloned()
    }

    pub fn list(&self) -> Vec<PluginEntry> {
        self.entries.read().values().cloned().collect()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.entries.read().contains_key(id)
    }

    pub fn count(&self) -> usize {
        self.entries.read().len()
    }

    pub fn set_state(&self, id: &str, state: PluginState) -> Result<()> {
        let mut entries = self.entries.write();
        let entry = entries.get_mut(id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' not found", id),
            )
        })?;
        entry.state = state;
        Ok(())
    }

    pub fn set_health(&self, id: &str, health: String) -> Result<()> {
        let mut entries = self.entries.write();
        let entry = entries.get_mut(id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' not found", id),
            )
        })?;
        entry.health = health;
        Ok(())
    }

    pub fn state(&self, id: &str) -> Option<PluginState> {
        self.entries.read().get(id).map(|e| e.state.clone())
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;
    use async_trait::async_trait;

    struct DummyPlugin {
        manifest: PluginManifest,
    }

    #[async_trait]
    impl Plugin for DummyPlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }
    }

    fn dummy(id: &str) -> Arc<dyn Plugin> {
        Arc::new(DummyPlugin {
            manifest: PluginManifest::new(id, id, "1.0", "NOVA", "dummy"),
        })
    }

    #[test]
    fn test_register_lookup() {
        let reg = PluginRegistry::new();
        reg.register("p1", dummy("p1")).unwrap();
        assert!(reg.contains("p1"));
        assert_eq!(reg.count(), 1);
        let entry = reg.get("p1").unwrap();
        assert_eq!(entry.state, PluginState::Installed);
    }

    #[test]
    fn test_duplicate_registration_fails() {
        let reg = PluginRegistry::new();
        reg.register("p1", dummy("p1")).unwrap();
        assert!(reg.register("p1", dummy("p1")).is_err());
    }

    #[test]
    fn test_unregister() {
        let reg = PluginRegistry::new();
        reg.register("p1", dummy("p1")).unwrap();
        reg.unregister("p1").unwrap();
        assert!(!reg.contains("p1"));
    }

    #[test]
    fn test_unregister_missing() {
        let reg = PluginRegistry::new();
        assert!(reg.unregister("nope").is_err());
    }

    #[test]
    fn test_set_state() {
        let reg = PluginRegistry::new();
        reg.register("p1", dummy("p1")).unwrap();
        reg.set_state("p1", PluginState::Enabled).unwrap();
        assert_eq!(reg.state("p1"), Some(PluginState::Enabled));
    }
}
