use nova_kernel::{log_activity, Result};

use crate::error::plugin_error;
use crate::permissions::SharedPermissionManager;

pub trait Sandbox: Send + Sync {
    fn validate_action(&self, plugin_id: &str, action: &str, permission: &str) -> Result<()>;
    fn validate_storage_access(&self, plugin_id: &str, path: &str) -> Result<()>;
    fn validate_network_access(&self, plugin_id: &str) -> Result<()>;
}

pub struct PluginSandbox {
    permissions: SharedPermissionManager,
}

impl PluginSandbox {
    pub fn new(permissions: SharedPermissionManager) -> Self {
        Self { permissions }
    }
}

impl Sandbox for PluginSandbox {
    fn validate_action(&self, plugin_id: &str, action: &str, permission: &str) -> Result<()> {
        if self.permissions.check(plugin_id, permission) {
            log_activity(
                "plugin_sdk",
                "sandbox_action_allowed",
                &format!(
                    "Plugin '{}' action '{}' granted via '{}'",
                    plugin_id, action, permission
                ),
                None,
            );
            Ok(())
        } else {
            log_activity(
                "plugin_sdk",
                "sandbox_action_denied",
                &format!(
                    "Plugin '{}' action '{}' denied — missing '{}'",
                    plugin_id, action, permission
                ),
                None,
            );
            Err(plugin_error(
                "ERR_PLUGIN_SANDBOX",
                &format!(
                    "Plugin '{}' action '{}' blocked by sandbox — requires '{}'",
                    plugin_id, action, permission
                ),
            ))
        }
    }

    fn validate_storage_access(&self, plugin_id: &str, path: &str) -> Result<()> {
        let store_path = format!("plugins/{}/", plugin_id);
        if !path.contains(&store_path) && !path.contains("plugin_data") {
            log_activity(
                "plugin_sdk",
                "sandbox_storage_denied",
                &format!("Plugin '{}' blocked from accessing '{}'", plugin_id, path),
                None,
            );
            return Err(plugin_error(
                "ERR_PLUGIN_SANDBOX_STORAGE",
                &format!(
                    "Plugin '{}' cannot access path '{}' outside its storage",
                    plugin_id, path
                ),
            ));
        }
        Ok(())
    }

    fn validate_network_access(&self, plugin_id: &str) -> Result<()> {
        if self.permissions.check(plugin_id, "internet.access") {
            log_activity(
                "plugin_sdk",
                "sandbox_network_allowed",
                &format!("Plugin '{}' network access granted", plugin_id),
                None,
            );
            Ok(())
        } else {
            log_activity(
                "plugin_sdk",
                "sandbox_network_denied",
                &format!("Plugin '{}' network access denied", plugin_id),
                None,
            );
            Err(plugin_error(
                "ERR_PLUGIN_SANDBOX_NETWORK",
                &format!("Plugin '{}' has no internet.access permission", plugin_id),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::PluginPermissionManager;
    use std::sync::Arc;

    fn make_sandbox() -> PluginSandbox {
        let pm = Arc::new(PluginPermissionManager::new());
        PluginSandbox::new(pm)
    }

    #[test]
    fn test_validate_action_granted() {
        let sb = make_sandbox();
        sb.permissions
            .declare("p1", &["memory.read".to_string()])
            .unwrap();
        sb.permissions.grant_all("p1").unwrap();
        assert!(sb.validate_action("p1", "read", "memory.read").is_ok());
    }

    #[test]
    fn test_validate_action_denied() {
        let sb = make_sandbox();
        sb.permissions
            .declare("p1", &["memory.read".to_string()])
            .unwrap();
        assert!(sb.validate_action("p1", "write", "memory.write").is_err());
    }

    #[test]
    fn test_validate_network_no_permission() {
        let sb = make_sandbox();
        sb.permissions.declare("p1", &[]).unwrap();
        assert!(sb.validate_network_access("p1").is_err());
    }

    #[test]
    fn test_validate_network_with_permission() {
        let sb = make_sandbox();
        sb.permissions
            .declare("p1", &["internet.access".to_string()])
            .unwrap();
        sb.permissions.grant_all("p1").unwrap();
        assert!(sb.validate_network_access("p1").is_ok());
    }

    #[test]
    fn test_validate_storage_outside_bound() {
        let sb = make_sandbox();
        assert!(sb
            .validate_storage_access("p1", "C:\\Windows\\system32")
            .is_err());
    }
}
