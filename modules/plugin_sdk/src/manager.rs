use nova_kernel::Result;
use std::sync::Arc;

use crate::context::PluginContext;
use crate::error::plugin_error;
use crate::events::{publish_plugin_event, PluginEventType};
use crate::lifecycle::PluginLifecycleManager;
use crate::loader::PluginLoader;
use crate::permissions::{PluginPermissionManager, SharedPermissionManager};
use crate::plugin::Plugin;
use crate::registry::{PluginEntry, PluginRegistry};
use crate::sandbox::{PluginSandbox, Sandbox};
use crate::storage::PluginStorage;

pub struct PluginManager {
    pub registry: Arc<PluginRegistry>,
    pub loader: Arc<PluginLoader>,
    pub lifecycle: Arc<PluginLifecycleManager>,
    pub permissions: SharedPermissionManager,
    pub sandbox: Arc<dyn Sandbox>,
    pub event_bus: Option<Arc<nova_kernel::EventBus>>,
}

impl PluginManager {
    pub fn new(event_bus: Option<Arc<nova_kernel::EventBus>>) -> Self {
        let registry = Arc::new(PluginRegistry::new());
        let permissions = Arc::new(PluginPermissionManager::new());
        let sandbox = Arc::new(PluginSandbox::new(permissions.clone()));
        let lifecycle = Arc::new(PluginLifecycleManager::new(registry.clone()));
        Self {
            registry,
            loader: Arc::new(PluginLoader::new()),
            lifecycle,
            permissions,
            sandbox,
            event_bus,
        }
    }

    pub fn register_plugin(&self, plugin: Arc<dyn Plugin>) -> Result<()> {
        let manifest = plugin.manifest();
        let id = &manifest.plugin_id;
        self.registry.register(id, plugin.clone())?;
        self.loader.register(id, plugin.clone())?;
        let perms: Vec<String> = manifest.required_permissions.clone();
        if !perms.is_empty() {
            self.permissions.declare(id, &perms)?;
            self.permissions.grant_all(id)?;
        }
        if let Some(ref eb) = self.event_bus {
            publish_plugin_event(
                eb,
                PluginEventType::PluginInstalled,
                id,
                &manifest.name,
                &manifest.version,
                "Plugin registered",
            );
        }
        Ok(())
    }

    pub async fn install_plugin(&self, id: &str) -> Result<()> {
        self.lifecycle.install(id).await?;
        if let Some(ref eb) = self.event_bus {
            let entry = self.registry.get(id).unwrap();
            publish_plugin_event(
                eb,
                PluginEventType::PluginLoaded,
                id,
                &entry.manifest.name,
                &entry.manifest.version,
                "Plugin loaded",
            );
        }
        Ok(())
    }

    pub async fn enable_plugin(&self, id: &str) -> Result<()> {
        self.lifecycle.enable(id, self.event_bus.as_ref()).await
    }

    pub async fn disable_plugin(&self, id: &str) -> Result<()> {
        self.lifecycle.disable(id, self.event_bus.as_ref()).await
    }

    pub async fn unload_plugin(&self, id: &str) -> Result<()> {
        self.lifecycle.unload(id, self.event_bus.as_ref()).await
    }

    pub async fn uninstall_plugin(&self, id: &str) -> Result<()> {
        self.lifecycle
            .uninstall(id, self.event_bus.as_ref())
            .await?;
        self.loader.unload(id)?;
        self.permissions.revoke_all(id);
        Ok(())
    }

    pub async fn update_plugin(&self, id: &str) -> Result<()> {
        self.lifecycle.update(id, self.event_bus.as_ref()).await
    }

    pub async fn reload_plugin(&self, id: &str) -> Result<()> {
        self.lifecycle.reload(id).await
    }

    pub fn check_health(&self, id: &str) -> Result<String> {
        self.lifecycle.check_health(id)
    }

    pub fn get_plugin(&self, id: &str) -> Option<PluginEntry> {
        self.registry.get(id)
    }

    pub fn list_plugins(&self) -> Vec<PluginEntry> {
        self.registry.list()
    }

    pub fn plugin_instance(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        self.registry.instance(id)
    }

    pub fn create_context(&self, plugin_id: &str) -> Result<PluginContext> {
        if !self.registry.contains(plugin_id) {
            return Err(plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' not found", plugin_id),
            ));
        }
        let storage = Arc::new(PluginStorage::new_in_memory(plugin_id));
        Ok(PluginContext::new(
            plugin_id,
            storage,
            self.permissions.clone(),
        ))
    }

    pub fn check_action(&self, plugin_id: &str, action: &str, permission: &str) -> Result<()> {
        self.sandbox.validate_action(plugin_id, action, permission)
    }

    pub fn check_network(&self, plugin_id: &str) -> Result<()> {
        self.sandbox.validate_network_access(plugin_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;
    use crate::plugin::Plugin;
    use crate::registry::PluginState;
    use async_trait::async_trait;

    struct SamplePlugin {
        manifest: PluginManifest,
    }

    #[async_trait]
    impl Plugin for SamplePlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }

        async fn on_enable(&self, ctx: &PluginContext) -> Result<()> {
            ctx.log("SamplePlugin enabled");
            Ok(())
        }
    }

    fn make_sample(id: &str, perms: &[&str]) -> Arc<dyn Plugin> {
        Arc::new(SamplePlugin {
            manifest: PluginManifest::new(id, id, "1.0", "NOVA", "desc").with_permissions(perms),
        })
    }

    #[tokio::test]
    async fn test_manager_register_and_list() {
        let mgr = PluginManager::new(None);
        mgr.register_plugin(make_sample("hello", &[])).unwrap();
        assert_eq!(mgr.list_plugins().len(), 1);
    }

    #[tokio::test]
    async fn test_manager_install_enable_disable() {
        let mgr = PluginManager::new(None);
        mgr.register_plugin(make_sample("cycle", &[])).unwrap();
        mgr.install_plugin("cycle").await.unwrap();
        assert_eq!(
            mgr.get_plugin("cycle").unwrap().state,
            PluginState::Installed
        );
        mgr.enable_plugin("cycle").await.unwrap();
        assert_eq!(mgr.get_plugin("cycle").unwrap().state, PluginState::Enabled);
        mgr.disable_plugin("cycle").await.unwrap();
        assert_eq!(
            mgr.get_plugin("cycle").unwrap().state,
            PluginState::Disabled
        );
    }

    #[tokio::test]
    async fn test_manager_full_lifecycle() {
        let mgr = PluginManager::new(None);
        mgr.register_plugin(make_sample("full", &[])).unwrap();
        mgr.install_plugin("full").await.unwrap();
        mgr.enable_plugin("full").await.unwrap();
        mgr.disable_plugin("full").await.unwrap();
        mgr.unload_plugin("full").await.unwrap();
        assert_eq!(mgr.get_plugin("full").unwrap().state, PluginState::Unloaded);
    }

    #[tokio::test]
    async fn test_manager_uninstall_removes_completely() {
        let mgr = PluginManager::new(None);
        mgr.register_plugin(make_sample("gone", &[])).unwrap();
        mgr.install_plugin("gone").await.unwrap();
        mgr.uninstall_plugin("gone").await.unwrap();
        assert!(mgr.get_plugin("gone").is_none());
    }

    #[tokio::test]
    async fn test_manager_check_action() {
        let mgr = PluginManager::new(None);
        mgr.register_plugin(make_sample("secure", &["memory.read"]))
            .unwrap();
        mgr.permissions.grant_all("secure").unwrap();
        assert!(mgr.check_action("secure", "read", "memory.read").is_ok());
        assert!(mgr.check_action("secure", "write", "memory.write").is_err());
    }

    #[test]
    fn test_manager_health() {
        let mgr = PluginManager::new(None);
        mgr.register_plugin(make_sample("healthy", &[])).unwrap();
        let health = mgr.check_health("healthy").unwrap();
        assert_eq!(health, "healthy");
    }
}
