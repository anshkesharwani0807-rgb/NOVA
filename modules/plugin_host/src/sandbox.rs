use nova_kernel::{log_activity, Result};
use std::collections::HashSet;

pub trait PluginSandbox: Send + Sync {
    fn validate(&self, plugin_id: &str, permissions: &[&str]) -> Result<bool>;
    fn sandbox_id(&self) -> &'static str;
}

pub struct NullSandbox {
    id: &'static str,
    allowed: HashSet<String>,
}

impl NullSandbox {
    pub fn new(id: &'static str, permissions: &[&str]) -> Self {
        Self {
            id,
            allowed: permissions.iter().map(|p| p.to_string()).collect(),
        }
    }
}

impl PluginSandbox for NullSandbox {
    fn validate(&self, plugin_id: &str, permissions: &[&str]) -> Result<bool> {
        let granted = permissions.iter().all(|p| self.allowed.contains(*p));
        if granted {
            log_activity(
                "plugin_host",
                "sandbox_validate",
                &format!("Plugin '{}' granted permissions", plugin_id),
                None,
            );
        } else {
            log_activity(
                "plugin_host",
                "sandbox_denied",
                &format!("Plugin '{}' denied — insufficient permissions", plugin_id),
                None,
            );
        }
        Ok(granted)
    }

    fn sandbox_id(&self) -> &'static str {
        self.id
    }
}
