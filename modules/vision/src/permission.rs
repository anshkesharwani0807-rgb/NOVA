use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisionCapability {
    Camera,
    GalleryRead,
    MediaPicker,
    Storage,
    CameraFrame,
    FaceRecognition,
    VisualSearch,
}

impl VisionCapability {
    pub fn name(&self) -> &'static str {
        match self {
            VisionCapability::Camera => "vision_camera",
            VisionCapability::GalleryRead => "vision_gallery_read",
            VisionCapability::MediaPicker => "vision_media_picker",
            VisionCapability::Storage => "vision_storage",
            VisionCapability::CameraFrame => "vision_camera_frame",
            VisionCapability::FaceRecognition => "vision_face_recognition",
            VisionCapability::VisualSearch => "vision_visual_search",
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
    pub capability: VisionCapability,
    pub state: PermissionState,
    pub granted_at: Option<String>,
    pub expires_at: Option<String>,
}

pub struct VisionPermissionManager {
    inner: RwLock<HashMap<VisionCapability, PermissionEntry>>,
}

impl VisionPermissionManager {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for VisionPermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl VisionPermissionManager {
    pub fn grant(&self, capability: &VisionCapability) {
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

    pub fn deny(&self, capability: &VisionCapability, _reason: &str) {
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

    pub fn revoke(&self, capability: &VisionCapability) {
        self.inner.write().remove(capability);
    }

    pub fn is_granted(&self, capability: &VisionCapability) -> bool {
        self.inner
            .read()
            .get(capability)
            .map(|e| e.state == PermissionState::Granted)
            .unwrap_or(false)
    }

    pub fn check(&self, capability: &VisionCapability) -> PermissionState {
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
