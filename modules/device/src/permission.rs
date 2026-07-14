use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Capabilities that require permission.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceCapability {
    Camera,
    GalleryRead,
    GalleryWrite,
    FileRead,
    FileWrite,
    ClipboardRead,
    ClipboardWrite,
    Notifications,
    CalendarRead,
    CalendarWrite,
    ContactsRead,
    ContactsWrite,
    SmsRead,
    SmsSend,
    PhoneCall,
    Location,
    Sensors,
    Battery,
    Storage,
    InstalledApps,
    ShareSheet,
    Downloads,
    MediaPicker,
    Biometric,
    AudioRecord,
}

impl DeviceCapability {
    pub fn name(&self) -> &'static str {
        match self {
            DeviceCapability::Camera => "camera",
            DeviceCapability::GalleryRead => "gallery_read",
            DeviceCapability::GalleryWrite => "gallery_write",
            DeviceCapability::FileRead => "file_read",
            DeviceCapability::FileWrite => "file_write",
            DeviceCapability::ClipboardRead => "clipboard_read",
            DeviceCapability::ClipboardWrite => "clipboard_write",
            DeviceCapability::Notifications => "notifications",
            DeviceCapability::CalendarRead => "calendar_read",
            DeviceCapability::CalendarWrite => "calendar_write",
            DeviceCapability::ContactsRead => "contacts_read",
            DeviceCapability::ContactsWrite => "contacts_write",
            DeviceCapability::SmsRead => "sms_read",
            DeviceCapability::SmsSend => "sms_send",
            DeviceCapability::PhoneCall => "phone_call",
            DeviceCapability::Location => "location",
            DeviceCapability::Sensors => "sensors",
            DeviceCapability::Battery => "battery",
            DeviceCapability::Storage => "storage",
            DeviceCapability::InstalledApps => "installed_apps",
            DeviceCapability::ShareSheet => "share_sheet",
            DeviceCapability::Downloads => "downloads",
            DeviceCapability::MediaPicker => "media_picker",
            DeviceCapability::Biometric => "biometric",
            DeviceCapability::AudioRecord => "audio_record",
        }
    }

    pub fn requires_biometric(&self) -> bool {
        matches!(
            self,
            DeviceCapability::SmsSend
                | DeviceCapability::PhoneCall
                | DeviceCapability::ContactsWrite
                | DeviceCapability::CalendarWrite
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionState {
    Granted,
    Denied,
    NotRequested,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGrant {
    pub capability: DeviceCapability,
    pub state: PermissionState,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionEntry {
    pub capability: DeviceCapability,
    pub state: PermissionState,
    pub granted_at: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub capability: DeviceCapability,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PermissionResult {
    Granted,
    Denied(String),
    RequiresBiometric,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionAuditEntry {
    pub capability: String,
    pub action: String,
    pub result: String,
    pub timestamp: String,
    pub correlation_id: String,
}

#[derive(Default)]
pub struct PermissionManager {
    grants: RwLock<HashMap<DeviceCapability, PermissionEntry>>,
    audit: RwLock<Vec<PermissionAuditEntry>>,
}

impl PermissionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn request(&self, capability: &DeviceCapability, _reason: &str) -> PermissionResult {
        if capability.requires_biometric() {
            return PermissionResult::RequiresBiometric;
        }
        let mut grants = self.grants.write();
        let entry = grants.get(capability);
        match entry {
            Some(e) if e.state == PermissionState::Granted => PermissionResult::Granted,
            Some(e) if e.state == PermissionState::Denied => {
                PermissionResult::Denied(format!("Permission denied for {}", capability.name()))
            }
            _ => {
                grants.insert(
                    capability.clone(),
                    PermissionEntry {
                        capability: capability.clone(),
                        state: PermissionState::Granted,
                        granted_at: Some(chrono::Local::now().to_rfc3339()),
                        expires_at: None,
                    },
                );
                self.audit.write().push(PermissionAuditEntry {
                    capability: capability.name().to_string(),
                    action: "request".to_string(),
                    result: "Granted".to_string(),
                    timestamp: chrono::Local::now().to_rfc3339(),
                    correlation_id: String::new(),
                });
                PermissionResult::Granted
            }
        }
    }

    pub fn grant(&self, capability: &DeviceCapability) {
        let mut grants = self.grants.write();
        grants.insert(
            capability.clone(),
            PermissionEntry {
                capability: capability.clone(),
                state: PermissionState::Granted,
                granted_at: Some(chrono::Local::now().to_rfc3339()),
                expires_at: None,
            },
        );
        self.audit.write().push(PermissionAuditEntry {
            capability: capability.name().to_string(),
            action: "grant".to_string(),
            result: "Granted".to_string(),
            timestamp: chrono::Local::now().to_rfc3339(),
            correlation_id: String::new(),
        });
    }

    pub fn deny(&self, capability: &DeviceCapability, reason: &str) {
        let mut grants = self.grants.write();
        grants.insert(
            capability.clone(),
            PermissionEntry {
                capability: capability.clone(),
                state: PermissionState::Denied,
                granted_at: None,
                expires_at: None,
            },
        );
        self.audit.write().push(PermissionAuditEntry {
            capability: capability.name().to_string(),
            action: "deny".to_string(),
            result: format!("Denied: {reason}"),
            timestamp: chrono::Local::now().to_rfc3339(),
            correlation_id: String::new(),
        });
    }

    pub fn revoke(&self, capability: &DeviceCapability) {
        let mut grants = self.grants.write();
        grants.remove(capability);
        self.audit.write().push(PermissionAuditEntry {
            capability: capability.name().to_string(),
            action: "revoke".to_string(),
            result: "Revoked".to_string(),
            timestamp: chrono::Local::now().to_rfc3339(),
            correlation_id: String::new(),
        });
    }

    pub fn check(&self, capability: &DeviceCapability) -> PermissionState {
        self.grants
            .read()
            .get(capability)
            .map(|e| e.state)
            .unwrap_or(PermissionState::NotRequested)
    }

    pub fn is_granted(&self, capability: &DeviceCapability) -> bool {
        self.check(capability) == PermissionState::Granted
    }

    pub fn list_grants(&self) -> Vec<PermissionEntry> {
        self.grants.read().values().cloned().collect()
    }

    pub fn audit_log(&self) -> Vec<PermissionAuditEntry> {
        self.audit.read().clone()
    }

    pub fn count(&self) -> usize {
        self.grants.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_granted() {
        let pm = PermissionManager::new();
        assert_eq!(
            pm.request(&DeviceCapability::Camera, "Need camera"),
            PermissionResult::Granted
        );
        assert_eq!(
            pm.check(&DeviceCapability::Camera),
            PermissionState::Granted
        );
    }

    #[test]
    fn test_deny_and_revoke() {
        let pm = PermissionManager::new();
        pm.deny(&DeviceCapability::Location, "User denied");
        assert_eq!(
            pm.check(&DeviceCapability::Location),
            PermissionState::Denied
        );
        pm.revoke(&DeviceCapability::Location);
        assert_eq!(
            pm.check(&DeviceCapability::Location),
            PermissionState::NotRequested
        );
    }

    #[test]
    fn test_biometric_required() {
        let pm = PermissionManager::new();
        assert!(matches!(
            pm.request(&DeviceCapability::SmsSend, "Send SMS"),
            PermissionResult::RequiresBiometric
        ));
    }

    #[test]
    fn test_audit_logging() {
        let pm = PermissionManager::new();
        pm.grant(&DeviceCapability::Camera);
        pm.deny(&DeviceCapability::Location, "Privacy concern");
        assert_eq!(pm.audit_log().len(), 2);
    }

    #[test]
    fn test_list_grants() {
        let pm = PermissionManager::new();
        pm.grant(&DeviceCapability::Camera);
        pm.grant(&DeviceCapability::ClipboardRead);
        let grants = pm.list_grants();
        assert_eq!(grants.len(), 2);
    }
}
