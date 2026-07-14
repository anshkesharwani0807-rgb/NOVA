use nova_kernel::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::permissions::SharedPermissionManager;
use crate::storage::PluginStorage;

pub struct PluginContext {
    pub plugin_id: String,
    pub storage: Arc<PluginStorage>,
    pub permissions: SharedPermissionManager,
    pub config: RwLock<HashMap<String, String>>,
    pub logger: PluginLogger,
}

impl PluginContext {
    pub fn new(
        plugin_id: &str,
        storage: Arc<PluginStorage>,
        permissions: SharedPermissionManager,
    ) -> Self {
        Self {
            plugin_id: plugin_id.to_string(),
            storage,
            permissions,
            config: RwLock::new(HashMap::new()),
            logger: PluginLogger::new(plugin_id),
        }
    }

    pub fn check_permission(&self, permission: &str) -> Result<()> {
        self.permissions.check_or_fail(&self.plugin_id, permission)
    }

    pub fn set_config(&self, key: &str, value: &str) {
        self.config
            .write()
            .insert(key.to_string(), value.to_string());
    }

    pub fn get_config(&self, key: &str) -> Option<String> {
        self.config.read().get(key).cloned()
    }

    pub fn log(&self, message: &str) {
        self.logger.info(message);
    }
}

pub struct PluginLogger {
    plugin_id: String,
}

impl PluginLogger {
    pub fn new(plugin_id: &str) -> Self {
        Self {
            plugin_id: plugin_id.to_string(),
        }
    }

    pub fn info(&self, message: &str) {
        tracing::info!(plugin = %self.plugin_id, "{}", message);
    }

    pub fn warn(&self, message: &str) {
        tracing::warn!(plugin = %self.plugin_id, "{}", message);
    }

    pub fn error(&self, message: &str) {
        tracing::error!(plugin = %self.plugin_id, "{}", message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::PluginPermissionManager;
    use crate::storage::PluginStorage;

    fn make_context(id: &str) -> PluginContext {
        let storage = Arc::new(PluginStorage::new_in_memory(id));
        let permissions = Arc::new(PluginPermissionManager::new());
        PluginContext::new(id, storage, permissions)
    }

    #[test]
    fn test_context_config() {
        let ctx = make_context("test");
        ctx.set_config("key", "value");
        assert_eq!(ctx.get_config("key"), Some("value".to_string()));
        assert_eq!(ctx.get_config("missing"), None);
    }

    #[test]
    fn test_context_check_permission_missing() {
        let ctx = make_context("test");
        assert!(ctx.check_permission("memory.read").is_err());
    }

    #[test]
    fn test_context_check_permission_granted() {
        let ctx = make_context("test");
        ctx.permissions
            .declare("test", &["memory.read".to_string()])
            .unwrap();
        ctx.permissions.grant_all("test").unwrap();
        assert!(ctx.check_permission("memory.read").is_ok());
    }
}
