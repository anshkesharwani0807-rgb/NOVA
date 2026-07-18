use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenCapability {
    Capture,
    Ocr,
    UiTree,
    Grounding,
}

impl ScreenCapability {
    pub fn name(&self) -> &'static str {
        match self {
            ScreenCapability::Capture => "screen_capture",
            ScreenCapability::Ocr => "screen_ocr",
            ScreenCapability::UiTree => "screen_ui_tree",
            ScreenCapability::Grounding => "screen_grounding",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionState {
    Granted,
    Denied,
    NotRequested,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionEntry {
    pub capability: ScreenCapability,
    pub state: PermissionState,
    pub granted_at: Option<String>,
    pub expires_at: Option<String>,
}

pub struct ScreenPermissionManager {
    inner: RwLock<HashMap<ScreenCapability, PermissionEntry>>,
}

impl ScreenPermissionManager {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for ScreenPermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenPermissionManager {
    pub fn grant(&self, capability: &ScreenCapability) {
        let mut map = self.inner.write();
        map.insert(
            capability.clone(),
            PermissionEntry {
                capability: capability.clone(),
                state: PermissionState::Granted,
                granted_at: Some(chrono::Local::now().to_rfc3339()),
                expires_at: None,
            },
        );
    }

    pub fn deny(&self, capability: &ScreenCapability, _reason: &str) {
        let mut map = self.inner.write();
        map.insert(
            capability.clone(),
            PermissionEntry {
                capability: capability.clone(),
                state: PermissionState::Denied,
                granted_at: None,
                expires_at: None,
            },
        );
    }

    pub fn revoke(&self, capability: &ScreenCapability) {
        self.inner.write().remove(capability);
    }

    pub fn is_granted(&self, capability: &ScreenCapability) -> bool {
        self.inner
            .read()
            .get(capability)
            .map(|e| e.state == PermissionState::Granted)
            .unwrap_or(false)
    }

    pub fn check(&self, capability: &ScreenCapability) -> PermissionState {
        self.inner
            .read()
            .get(capability)
            .map(|e| e.state)
            .unwrap_or(PermissionState::NotRequested)
    }

    pub fn list_grants(&self) -> Vec<PermissionEntry> {
        self.inner.read().values().cloned().collect()
    }

    pub fn count(&self) -> usize {
        self.inner.read().len()
    }
}
