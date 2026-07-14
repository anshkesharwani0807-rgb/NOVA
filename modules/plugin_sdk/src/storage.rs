use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct PluginStorage {
    _plugin_id: String,
    base_dir: RwLock<Option<PathBuf>>,
    memory: RwLock<HashMap<String, String>>,
    config: RwLock<HashMap<String, String>>,
}

impl PluginStorage {
    pub fn new(plugin_id: &str, base_dir: &Path) -> Self {
        Self {
            _plugin_id: plugin_id.to_string(),
            base_dir: RwLock::new(Some(base_dir.join("plugin_data").join(plugin_id))),
            memory: RwLock::new(HashMap::new()),
            config: RwLock::new(HashMap::new()),
        }
    }

    pub fn new_in_memory(plugin_id: &str) -> Self {
        Self {
            _plugin_id: plugin_id.to_string(),
            base_dir: RwLock::new(None),
            memory: RwLock::new(HashMap::new()),
            config: RwLock::new(HashMap::new()),
        }
    }

    pub fn plugin_dir(&self) -> Option<PathBuf> {
        self.base_dir.read().clone()
    }

    pub fn data_dir(&self) -> Option<PathBuf> {
        self.base_dir.read().as_ref().map(|d| d.join("data"))
    }

    pub fn config_path(&self) -> Option<PathBuf> {
        self.base_dir.read().as_ref().map(|d| d.join("config.json"))
    }

    pub fn cache_dir(&self) -> Option<PathBuf> {
        self.base_dir.read().as_ref().map(|d| d.join("cache"))
    }

    pub fn store(&self, key: &str, value: &str) {
        self.memory
            .write()
            .insert(key.to_string(), value.to_string());
    }

    pub fn retrieve(&self, key: &str) -> Option<String> {
        self.memory.read().get(key).cloned()
    }

    pub fn remove(&self, key: &str) {
        self.memory.write().remove(key);
    }

    pub fn set_config(&self, key: &str, value: &str) {
        self.config
            .write()
            .insert(key.to_string(), value.to_string());
    }

    pub fn get_config(&self, key: &str) -> Option<String> {
        self.config.read().get(key).cloned()
    }

    pub fn clear(&self) {
        self.memory.write().clear();
        self.config.write().clear();
    }

    pub fn all_data(&self) -> HashMap<String, String> {
        self.memory.read().clone()
    }

    pub fn all_config(&self) -> HashMap<String, String> {
        self.config.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_storage() {
        let s = PluginStorage::new_in_memory("test");
        s.store("key", "value");
        assert_eq!(s.retrieve("key"), Some("value".to_string()));
        s.remove("key");
        assert_eq!(s.retrieve("key"), None);
    }

    #[test]
    fn test_storage_config() {
        let s = PluginStorage::new_in_memory("test");
        s.set_config("interval", "30");
        assert_eq!(s.get_config("interval"), Some("30".to_string()));
    }

    #[test]
    fn test_clear() {
        let s = PluginStorage::new_in_memory("test");
        s.store("a", "1");
        s.set_config("b", "2");
        s.clear();
        assert!(s.retrieve("a").is_none());
        assert!(s.get_config("b").is_none());
    }

    #[test]
    fn test_plugin_dir_with_base() {
        let s = PluginStorage::new("test", Path::new("/tmp/nova"));
        let dir = s.plugin_dir().unwrap();
        assert!(dir.ends_with("plugin_data/test"));
    }

    #[test]
    fn test_in_memory_no_disk_dirs() {
        let s = PluginStorage::new_in_memory("test");
        assert!(s.plugin_dir().is_none());
        assert!(s.data_dir().is_none());
        assert!(s.cache_dir().is_none());
    }
}
