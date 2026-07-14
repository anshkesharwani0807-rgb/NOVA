use nova_kernel::Result;
use std::sync::Arc;

use crate::context::PluginContext;
use crate::error::plugin_error;
use crate::events::{publish_plugin_event, PluginEventType};
use crate::permissions::SharedPermissionManager;
use crate::registry::{PluginRegistry, PluginState};
use crate::storage::PluginStorage;

pub struct PluginLifecycleManager {
    registry: Arc<PluginRegistry>,
}

impl PluginLifecycleManager {
    pub fn new(registry: Arc<PluginRegistry>) -> Self {
        Self { registry }
    }

    pub async fn install(&self, plugin_id: &str) -> Result<()> {
        let plugin = self.registry.instance(plugin_id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' not found", plugin_id),
            )
        })?;
        let storage = Arc::new(PluginStorage::new_in_memory(plugin_id));
        let ctx = PluginContext::new(plugin_id, storage, self.make_permissions(plugin_id));
        plugin.on_install(&ctx).await?;
        self.registry.set_state(plugin_id, PluginState::Installed)?;
        Ok(())
    }

    pub async fn enable(
        &self,
        plugin_id: &str,
        event_bus: Option<&Arc<nova_kernel::EventBus>>,
    ) -> Result<()> {
        let plugin = self.registry.instance(plugin_id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' not found", plugin_id),
            )
        })?;
        let entry = self.registry.get(plugin_id).unwrap();
        let storage = Arc::new(PluginStorage::new_in_memory(plugin_id));
        let ctx = PluginContext::new(plugin_id, storage, self.make_permissions(plugin_id));
        plugin.on_enable(&ctx).await?;
        self.registry.set_state(plugin_id, PluginState::Enabled)?;
        if let Some(eb) = event_bus {
            publish_plugin_event(
                eb,
                PluginEventType::PluginEnabled,
                plugin_id,
                &entry.manifest.name,
                &entry.manifest.version,
                "Plugin enabled",
            );
        }
        Ok(())
    }

    pub async fn disable(
        &self,
        plugin_id: &str,
        event_bus: Option<&Arc<nova_kernel::EventBus>>,
    ) -> Result<()> {
        let plugin = self.registry.instance(plugin_id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' not found", plugin_id),
            )
        })?;
        let entry = self.registry.get(plugin_id).unwrap();
        let storage = Arc::new(PluginStorage::new_in_memory(plugin_id));
        let ctx = PluginContext::new(plugin_id, storage, self.make_permissions(plugin_id));
        plugin.on_disable(&ctx).await?;
        self.registry.set_state(plugin_id, PluginState::Disabled)?;
        if let Some(eb) = event_bus {
            publish_plugin_event(
                eb,
                PluginEventType::PluginDisabled,
                plugin_id,
                &entry.manifest.name,
                &entry.manifest.version,
                "Plugin disabled",
            );
        }
        Ok(())
    }

    pub async fn update(
        &self,
        plugin_id: &str,
        event_bus: Option<&Arc<nova_kernel::EventBus>>,
    ) -> Result<()> {
        let plugin = self.registry.instance(plugin_id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' not found", plugin_id),
            )
        })?;
        let entry = self.registry.get(plugin_id).unwrap();
        let storage = Arc::new(PluginStorage::new_in_memory(plugin_id));
        let ctx = PluginContext::new(plugin_id, storage, self.make_permissions(plugin_id));
        plugin.on_update(&ctx).await?;
        if let Some(eb) = event_bus {
            publish_plugin_event(
                eb,
                PluginEventType::PluginUpdated,
                plugin_id,
                &entry.manifest.name,
                &entry.manifest.version,
                "Plugin updated",
            );
        }
        Ok(())
    }

    pub async fn reload(&self, plugin_id: &str) -> Result<()> {
        let plugin = self.registry.instance(plugin_id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' not found", plugin_id),
            )
        })?;
        let storage = Arc::new(PluginStorage::new_in_memory(plugin_id));
        let ctx = PluginContext::new(plugin_id, storage, self.make_permissions(plugin_id));
        plugin.on_reload(&ctx).await?;
        Ok(())
    }

    pub async fn unload(
        &self,
        plugin_id: &str,
        event_bus: Option<&Arc<nova_kernel::EventBus>>,
    ) -> Result<()> {
        let plugin = self.registry.instance(plugin_id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' not found", plugin_id),
            )
        })?;
        let entry = self.registry.get(plugin_id).unwrap();
        let storage = Arc::new(PluginStorage::new_in_memory(plugin_id));
        let ctx = PluginContext::new(plugin_id, storage, self.make_permissions(plugin_id));
        plugin.on_unload(&ctx).await?;
        self.registry.set_state(plugin_id, PluginState::Unloaded)?;
        if let Some(eb) = event_bus {
            publish_plugin_event(
                eb,
                PluginEventType::PluginUnloaded,
                plugin_id,
                &entry.manifest.name,
                &entry.manifest.version,
                "Plugin unloaded",
            );
        }
        Ok(())
    }

    pub async fn uninstall(
        &self,
        plugin_id: &str,
        event_bus: Option<&Arc<nova_kernel::EventBus>>,
    ) -> Result<()> {
        let entry = self.registry.get(plugin_id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' not found", plugin_id),
            )
        })?;
        self.registry.set_state(plugin_id, PluginState::Unloaded)?;
        self.registry.unregister(plugin_id)?;
        if let Some(eb) = event_bus {
            publish_plugin_event(
                eb,
                PluginEventType::PluginRemoved,
                plugin_id,
                &entry.manifest.name,
                &entry.manifest.version,
                "Plugin uninstalled",
            );
        }
        Ok(())
    }

    pub fn check_health(&self, plugin_id: &str) -> Result<String> {
        let plugin = self.registry.instance(plugin_id).ok_or_else(|| {
            plugin_error(
                "ERR_PLUGIN_NOT_FOUND",
                &format!("Plugin '{}' not found", plugin_id),
            )
        })?;
        let health = plugin.health();
        self.registry.set_health(plugin_id, health.clone())?;
        Ok(health)
    }

    fn make_permissions(&self, plugin_id: &str) -> SharedPermissionManager {
        let pm = Arc::new(crate::permissions::PluginPermissionManager::new());
        if let Some(entry) = self.registry.get(plugin_id) {
            let perms: Vec<String> = entry.manifest.required_permissions.clone();
            pm.declare(plugin_id, &perms).ok();
            pm.grant_all(plugin_id).ok();
        }
        pm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::PluginContext;
    use crate::manifest::PluginManifest;
    use crate::plugin::Plugin;
    use crate::registry::PluginRegistry;
    use async_trait::async_trait;
    use std::sync::Arc;

    struct LifecycleTestPlugin {
        manifest: PluginManifest,
        enable_count: parking_lot::RwLock<u32>,
    }

    #[async_trait]
    impl Plugin for LifecycleTestPlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }

        async fn on_enable(&self, _ctx: &PluginContext) -> Result<()> {
            *self.enable_count.write() += 1;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_install_and_enable() {
        let reg = Arc::new(PluginRegistry::new());
        let plugin = Arc::new(LifecycleTestPlugin {
            manifest: PluginManifest::new(
                "lifecycle_test",
                "Lifecycle Test",
                "1.0",
                "NOVA",
                "test",
            ),
            enable_count: parking_lot::RwLock::new(0),
        });
        reg.register("lifecycle_test", plugin).unwrap();
        let lm = PluginLifecycleManager::new(reg.clone());
        lm.install("lifecycle_test").await.unwrap();
        assert_eq!(reg.state("lifecycle_test"), Some(PluginState::Installed));
        lm.enable("lifecycle_test", None).await.unwrap();
        assert_eq!(reg.state("lifecycle_test"), Some(PluginState::Enabled));
    }

    #[tokio::test]
    async fn test_disable_and_unload() {
        let reg = Arc::new(PluginRegistry::new());
        let plugin = Arc::new(LifecycleTestPlugin {
            manifest: PluginManifest::new("test2", "Test2", "1.0", "NOVA", "test"),
            enable_count: parking_lot::RwLock::new(0),
        });
        reg.register("test2", plugin).unwrap();
        let lm = PluginLifecycleManager::new(reg.clone());
        lm.install("test2").await.unwrap();
        lm.enable("test2", None).await.unwrap();
        lm.disable("test2", None).await.unwrap();
        assert_eq!(reg.state("test2"), Some(PluginState::Disabled));
        lm.unload("test2", None).await.unwrap();
        assert_eq!(reg.state("test2"), Some(PluginState::Unloaded));
    }

    #[tokio::test]
    async fn test_uninstall_removes_from_registry() {
        let reg = Arc::new(PluginRegistry::new());
        let plugin = Arc::new(LifecycleTestPlugin {
            manifest: PluginManifest::new("test3", "Test3", "1.0", "NOVA", "test"),
            enable_count: parking_lot::RwLock::new(0),
        });
        reg.register("test3", plugin).unwrap();
        let lm = PluginLifecycleManager::new(reg.clone());
        lm.install("test3").await.unwrap();
        lm.uninstall("test3", None).await.unwrap();
        assert!(!reg.contains("test3"));
    }

    #[tokio::test]
    async fn test_health_check() {
        let reg = Arc::new(PluginRegistry::new());
        let plugin = Arc::new(LifecycleTestPlugin {
            manifest: PluginManifest::new("test4", "Test4", "1.0", "NOVA", "test"),
            enable_count: parking_lot::RwLock::new(0),
        });
        reg.register("test4", plugin).unwrap();
        let lm = PluginLifecycleManager::new(reg.clone());
        let health = lm.check_health("test4").unwrap();
        assert_eq!(health, "healthy");
    }
}
