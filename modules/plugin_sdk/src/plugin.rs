use async_trait::async_trait;
use nova_kernel::Result;

use crate::context::PluginContext;
use crate::manifest::PluginManifest;

#[async_trait]
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;

    async fn on_install(&self, ctx: &PluginContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    async fn on_enable(&self, ctx: &PluginContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    async fn on_disable(&self, ctx: &PluginContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    async fn on_update(&self, ctx: &PluginContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    async fn on_reload(&self, ctx: &PluginContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    async fn on_unload(&self, ctx: &PluginContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    fn health(&self) -> String {
        "healthy".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::PluginContext;
    use crate::manifest::PluginManifest;
    use crate::permissions::PluginPermissionManager;
    use crate::storage::PluginStorage;
    use std::sync::Arc;

    struct TestPlugin {
        manifest: PluginManifest,
    }

    #[async_trait]
    impl Plugin for TestPlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }
    }

    #[tokio::test]
    async fn test_plugin_lifecycle_defaults() {
        let plugin = TestPlugin {
            manifest: PluginManifest::new("test", "Test", "1.0", "NOVA", "desc"),
        };
        let storage = Arc::new(PluginStorage::new_in_memory("test"));
        let perms = Arc::new(PluginPermissionManager::new());
        let ctx = PluginContext::new("test", storage, perms);
        assert!(plugin.on_install(&ctx).await.is_ok());
        assert!(plugin.on_enable(&ctx).await.is_ok());
        assert!(plugin.on_disable(&ctx).await.is_ok());
        assert_eq!(plugin.health(), "healthy");
    }
}
