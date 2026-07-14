use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub required_permissions: Vec<String>,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<String>,
    pub min_nova_version: String,
    pub max_nova_version: String,
}

impl PluginManifest {
    pub fn new(
        plugin_id: &str,
        name: &str,
        version: &str,
        author: &str,
        description: &str,
    ) -> Self {
        Self {
            plugin_id: plugin_id.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            author: author.to_string(),
            description: description.to_string(),
            required_permissions: Vec::new(),
            capabilities: Vec::new(),
            dependencies: Vec::new(),
            min_nova_version: "0.1.0".to_string(),
            max_nova_version: "99.0.0".to_string(),
        }
    }

    pub fn with_permissions(mut self, perms: &[&str]) -> Self {
        self.required_permissions = perms.iter().map(|p| p.to_string()).collect();
        self
    }

    pub fn with_capabilities(mut self, caps: &[&str]) -> Self {
        self.capabilities = caps.iter().map(|c| c.to_string()).collect();
        self
    }

    pub fn with_dependencies(mut self, deps: &[&str]) -> Self {
        self.dependencies = deps.iter().map(|d| d.to_string()).collect();
        self
    }

    pub fn with_nova_version(mut self, min: &str, max: &str) -> Self {
        self.min_nova_version = min.to_string();
        self.max_nova_version = max.to_string();
        self
    }

    pub fn validate(&self) -> Result<()> {
        use crate::error::plugin_error;
        if self.plugin_id.is_empty() {
            return Err(plugin_error(
                "ERR_PLUGIN_MANIFEST",
                "plugin_id must not be empty",
            ));
        }
        if self.name.is_empty() {
            return Err(plugin_error(
                "ERR_PLUGIN_MANIFEST",
                "name must not be empty",
            ));
        }
        if self.version.is_empty() {
            return Err(plugin_error(
                "ERR_PLUGIN_MANIFEST",
                "version must not be empty",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_default() {
        let m = PluginManifest::new("hello", "Hello Plugin", "1.0.0", "NOVA", "A test plugin");
        assert_eq!(m.plugin_id, "hello");
        assert_eq!(m.required_permissions.len(), 0);
        assert!(m.validate().is_ok());
    }

    #[test]
    fn test_manifest_empty_id_fails() {
        let m = PluginManifest::new("", "", "", "", "");
        assert!(m.validate().is_err());
    }

    #[test]
    fn test_manifest_with_permissions() {
        let m = PluginManifest::new("test", "Test", "1.0", "NOVA", "desc")
            .with_permissions(&["memory.read", "memory.write"]);
        assert_eq!(m.required_permissions.len(), 2);
        assert!(m.required_permissions.contains(&"memory.read".to_string()));
    }

    #[test]
    fn test_manifest_json_roundtrip() {
        let m = PluginManifest::new("p1", "P1", "2.0", "Author", "A plugin")
            .with_permissions(&["voice.listen"])
            .with_capabilities(&["tts"])
            .with_nova_version("0.14.0", "1.0.0");
        let json = serde_json::to_string(&m).unwrap();
        let deserialized: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.plugin_id, "p1");
        assert_eq!(deserialized.required_permissions, vec!["voice.listen"]);
    }
}
