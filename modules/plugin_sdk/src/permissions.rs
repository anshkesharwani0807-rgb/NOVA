use nova_kernel::{log_activity, Result};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::error::plugin_error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginPermissionState {
    Granted,
    Denied,
    Pending,
}

pub struct PluginPermissionManager {
    declared: RwLock<HashMap<String, Vec<String>>>,
    granted: RwLock<HashMap<String, HashSet<String>>>,
}

impl PluginPermissionManager {
    pub fn new() -> Self {
        Self {
            declared: RwLock::new(HashMap::new()),
            granted: RwLock::new(HashMap::new()),
        }
    }

    pub fn declare(&self, plugin_id: &str, permissions: &[String]) -> Result<()> {
        let mut declared = self.declared.write();
        if declared.contains_key(plugin_id) {
            return Err(plugin_error(
                "ERR_PLUGIN_PERM_DUPLICATE",
                &format!("Permissions already declared for plugin '{}'", plugin_id),
            ));
        }
        declared.insert(plugin_id.to_string(), permissions.to_vec());
        log_activity(
            "plugin_sdk",
            "permissions_declared",
            &format!(
                "Plugin '{}' declared {} permission(s)",
                plugin_id,
                permissions.len()
            ),
            None,
        );
        Ok(())
    }

    pub fn grant(&self, plugin_id: &str, permissions: &[String]) {
        let mut granted = self.granted.write();
        let entry = granted.entry(plugin_id.to_string()).or_default();
        for p in permissions {
            entry.insert(p.clone());
        }
        log_activity(
            "plugin_sdk",
            "permissions_granted",
            &format!(
                "Plugin '{}' granted {} permission(s)",
                plugin_id,
                permissions.len()
            ),
            None,
        );
    }

    pub fn grant_all(&self, plugin_id: &str) -> Result<()> {
        let declared = self.declared.read();
        let perms = declared.get(plugin_id).cloned().unwrap_or_default();
        drop(declared);
        self.grant(plugin_id, &perms);
        Ok(())
    }

    pub fn revoke_all(&self, plugin_id: &str) {
        let mut granted = self.granted.write();
        granted.remove(plugin_id);
    }

    pub fn check(&self, plugin_id: &str, permission: &str) -> bool {
        let granted = self.granted.read();
        granted
            .get(plugin_id)
            .map(|set| set.contains(permission))
            .unwrap_or(false)
    }

    pub fn check_or_fail(&self, plugin_id: &str, permission: &str) -> Result<()> {
        if self.check(plugin_id, permission) {
            Ok(())
        } else {
            Err(plugin_error(
                "ERR_PLUGIN_PERM_DENIED",
                &format!(
                    "Plugin '{}' is missing required permission '{}'",
                    plugin_id, permission
                ),
            ))
        }
    }

    pub fn granted_permissions(&self, plugin_id: &str) -> Vec<String> {
        let granted = self.granted.read();
        granted
            .get(plugin_id)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn declared_permissions(&self, plugin_id: &str) -> Vec<String> {
        let declared = self.declared.read();
        declared.get(plugin_id).cloned().unwrap_or_default()
    }
}

impl Default for PluginPermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedPermissionManager = Arc<PluginPermissionManager>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_declare_and_grant() {
        let pm = PluginPermissionManager::new();
        pm.declare(
            "test_plugin",
            &["memory.read".to_string(), "memory.write".to_string()],
        )
        .unwrap();
        assert!(!pm.check("test_plugin", "memory.read"));
        pm.grant_all("test_plugin").unwrap();
        assert!(pm.check("test_plugin", "memory.read"));
        assert!(pm.check("test_plugin", "memory.write"));
        assert!(!pm.check("test_plugin", "voice.listen"));
    }

    #[test]
    fn test_duplicate_declare_fails() {
        let pm = PluginPermissionManager::new();
        pm.declare("p1", &["a".to_string()]).unwrap();
        assert!(pm.declare("p1", &["b".to_string()]).is_err());
    }

    #[test]
    fn test_check_or_fail() {
        let pm = PluginPermissionManager::new();
        pm.declare("p1", &["memory.read".to_string()]).unwrap();
        pm.grant_all("p1").unwrap();
        assert!(pm.check_or_fail("p1", "memory.read").is_ok());
        assert!(pm.check_or_fail("p1", "memory.write").is_err());
    }

    #[test]
    fn test_revoke_all() {
        let pm = PluginPermissionManager::new();
        pm.declare("p1", &["a".to_string()]).unwrap();
        pm.grant_all("p1").unwrap();
        assert!(pm.check("p1", "a"));
        pm.revoke_all("p1");
        assert!(!pm.check("p1", "a"));
    }

    #[test]
    fn test_granted_permissions() {
        let pm = PluginPermissionManager::new();
        pm.declare("p1", &["a".to_string(), "b".to_string()])
            .unwrap();
        pm.grant("p1", &["a".to_string()]);
        let granted = pm.granted_permissions("p1");
        assert_eq!(granted, vec!["a"]);
    }
}
