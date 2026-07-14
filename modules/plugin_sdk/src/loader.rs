use nova_kernel::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::plugin_error;
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;

pub struct PluginLoader {
    loaded: RwLock<HashMap<String, Arc<dyn Plugin>>>,
}

impl PluginLoader {
    pub fn new() -> Self {
        Self {
            loaded: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, id: &str, plugin: Arc<dyn Plugin>) -> Result<()> {
        let mut loaded = self.loaded.write();
        if loaded.contains_key(id) {
            return Err(plugin_error(
                "ERR_PLUGIN_ALREADY_LOADED",
                &format!("Plugin '{}' is already loaded", id),
            ));
        }
        plugin.manifest().validate()?;
        loaded.insert(id.to_string(), plugin);
        Ok(())
    }

    pub fn unload(&self, id: &str) -> Result<()> {
        let mut loaded = self.loaded.write();
        loaded.remove(id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_LOADED",
                &format!("Plugin '{}' is not loaded", id),
            )
        })?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        self.loaded.read().get(id).cloned()
    }

    pub fn list(&self) -> Vec<(String, PluginManifest)> {
        self.loaded
            .read()
            .iter()
            .map(|(id, p)| (id.clone(), p.manifest().clone()))
            .collect()
    }

    pub fn count(&self) -> usize {
        self.loaded.read().len()
    }

    pub fn hot_reload(&self, id: &str, new_plugin: Arc<dyn Plugin>) -> Result<()> {
        let mut loaded = self.loaded.write();
        if !loaded.contains_key(id) {
            return Err(plugin_error(
                "ERR_PLUGIN_NOT_LOADED",
                &format!("Cannot hot-reload '{}' — not loaded", id),
            ));
        }
        new_plugin.manifest().validate()?;
        loaded.insert(id.to_string(), new_plugin);
        Ok(())
    }

    pub fn resolve_dependencies(&self, id: &str) -> Result<Vec<String>> {
        let loaded = self.loaded.read();
        let plugin = loaded.get(id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_LOADED",
                &format!("Plugin '{}' not loaded", id),
            )
        })?;
        let deps = plugin.manifest().dependencies.clone();
        for dep in &deps {
            if !loaded.contains_key(dep.as_str()) {
                return Err(plugin_error(
                    "ERR_PLUGIN_DEP_MISSING",
                    &format!("Plugin '{}' depends on '{}' which is not loaded", id, dep),
                ));
            }
        }
        Ok(deps)
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;
    use crate::plugin::Plugin;
    use async_trait::async_trait;

    struct LoadablePlugin {
        manifest: PluginManifest,
    }

    #[async_trait]
    impl Plugin for LoadablePlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }
    }

    fn make_plugin(id: &str) -> Arc<dyn Plugin> {
        Arc::new(LoadablePlugin {
            manifest: PluginManifest::new(id, id, "1.0", "NOVA", "desc"),
        })
    }

    #[test]
    fn test_register_and_get() {
        let loader = PluginLoader::new();
        loader.register("p1", make_plugin("p1")).unwrap();
        assert!(loader.get("p1").is_some());
        assert_eq!(loader.count(), 1);
    }

    #[test]
    fn test_duplicate_register_fails() {
        let loader = PluginLoader::new();
        loader.register("p1", make_plugin("p1")).unwrap();
        assert!(loader.register("p1", make_plugin("p1")).is_err());
    }

    #[test]
    fn test_unload_removes() {
        let loader = PluginLoader::new();
        loader.register("p1", make_plugin("p1")).unwrap();
        loader.unload("p1").unwrap();
        assert_eq!(loader.count(), 0);
    }

    #[test]
    fn test_unload_missing_fails() {
        let loader = PluginLoader::new();
        assert!(loader.unload("nope").is_err());
    }

    #[test]
    fn test_list() {
        let loader = PluginLoader::new();
        loader.register("a", make_plugin("a")).unwrap();
        loader.register("b", make_plugin("b")).unwrap();
        let list = loader.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_hot_reload() {
        let loader = PluginLoader::new();
        loader.register("p1", make_plugin("p1")).unwrap();
        loader.hot_reload("p1", make_plugin("p1")).unwrap();
        assert_eq!(loader.count(), 1);
    }

    #[test]
    fn test_hot_reload_not_loaded_fails() {
        let loader = PluginLoader::new();
        assert!(loader.hot_reload("missing", make_plugin("p1")).is_err());
    }

    #[test]
    fn test_resolve_dependencies() {
        let loader = PluginLoader::new();
        let mut m = PluginManifest::new("parent", "Parent", "1.0", "NOVA", "desc");
        m.dependencies.push("child".to_string());
        let parent = Arc::new(LoadablePlugin { manifest: m });
        loader.register("child", make_plugin("child")).unwrap();
        loader.register("parent", parent).unwrap();
        let deps = loader.resolve_dependencies("parent").unwrap();
        assert_eq!(deps, vec!["child"]);
    }

    #[test]
    fn test_resolve_missing_dep_fails() {
        let loader = PluginLoader::new();
        let mut m = PluginManifest::new("orphan", "Orphan", "1.0", "NOVA", "desc");
        m.dependencies.push("ghost".to_string());
        loader
            .register("orphan", Arc::new(LoadablePlugin { manifest: m }))
            .unwrap();
        assert!(loader.resolve_dependencies("orphan").is_err());
    }
}
