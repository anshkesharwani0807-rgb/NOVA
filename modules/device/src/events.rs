use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceEventPayload {
    CameraOpened {
        camera_id: String,
    },
    PhotoCaptured {
        path: String,
    },
    GallerySelected {
        paths: Vec<String>,
    },
    ClipboardRead {
        content_len: usize,
    },
    ClipboardWritten {
        content_len: usize,
    },
    ClipboardHistoryCleared,
    NotificationPosted {
        package: String,
        title: String,
    },
    NotificationClicked {
        package: String,
        action: String,
    },
    CalendarEventCreated {
        title: String,
    },
    CalendarEventUpdated {
        title: String,
    },
    CalendarEventDeleted {
        title: String,
    },
    ContactAccessed {
        contact_id: String,
    },
    ContactCreated {
        contact_id: String,
    },
    ContactUpdated {
        contact_id: String,
    },
    ContactDeleted {
        contact_id: String,
    },
    SmsSent {
        recipient: String,
    },
    SmsReceived {
        sender: String,
    },
    PhoneCallDialed {
        number: String,
    },
    PhoneCallReceived {
        caller: String,
    },
    LocationRequested {
        precision: String,
    },
    LocationUpdated {
        lat: f64,
        lng: f64,
        accuracy: f64,
    },
    SensorUpdated {
        sensor_type: String,
        values: Vec<f64>,
    },
    BatteryLow {
        level: u8,
    },
    BatteryCharging,
    BatteryDischarging,
    BatteryFull,
    StorageLow {
        free_bytes: u64,
    },
    StorageCritical {
        free_bytes: u64,
    },
    PermissionGranted {
        capability: String,
    },
    PermissionDenied {
        capability: String,
        reason: String,
    },
    BiometricSucceeded,
    BiometricFailed {
        reason: String,
    },
    DeviceToolInvoked {
        tool: String,
        duration_ms: u64,
        success: bool,
    },
    ConnectivityChanged {
        online: bool,
        network_type: String,
    },
    InstalledAppsChanged,
    FileOpened {
        path: String,
    },
    MediaPicked {
        mime_type: String,
    },
    DownloadCompleted {
        path: String,
    },
    ShareSheetOpened,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEvent {
    pub id: Uuid,
    pub correlation_id: Uuid,
    pub timestamp: DateTime<Local>,
    pub payload: DeviceEventPayload,
}

impl DeviceEvent {
    pub fn new(correlation_id: Uuid, payload: DeviceEventPayload) -> Self {
        Self {
            id: Uuid::new_v4(),
            correlation_id,
            timestamp: Local::now(),
            payload,
        }
    }

    pub fn action_name(&self) -> &'static str {
        match self.payload {
            DeviceEventPayload::CameraOpened { .. } => "device.camera_opened",
            DeviceEventPayload::PhotoCaptured { .. } => "device.photo_captured",
            DeviceEventPayload::GallerySelected { .. } => "device.gallery_selected",
            DeviceEventPayload::ClipboardRead { .. } => "device.clipboard_read",
            DeviceEventPayload::ClipboardWritten { .. } => "device.clipboard_written",
            DeviceEventPayload::ClipboardHistoryCleared => "device.clipboard_history_cleared",
            DeviceEventPayload::NotificationPosted { .. } => "device.notification_posted",
            DeviceEventPayload::NotificationClicked { .. } => "device.notification_clicked",
            DeviceEventPayload::CalendarEventCreated { .. } => "device.calendar_event_created",
            DeviceEventPayload::CalendarEventUpdated { .. } => "device.calendar_event_updated",
            DeviceEventPayload::CalendarEventDeleted { .. } => "device.calendar_event_deleted",
            DeviceEventPayload::ContactAccessed { .. } => "device.contact_accessed",
            DeviceEventPayload::ContactCreated { .. } => "device.contact_created",
            DeviceEventPayload::ContactUpdated { .. } => "device.contact_updated",
            DeviceEventPayload::ContactDeleted { .. } => "device.contact_deleted",
            DeviceEventPayload::SmsSent { .. } => "device.sms_sent",
            DeviceEventPayload::SmsReceived { .. } => "device.sms_received",
            DeviceEventPayload::PhoneCallDialed { .. } => "device.phone_call_dialed",
            DeviceEventPayload::PhoneCallReceived { .. } => "device.phone_call_received",
            DeviceEventPayload::LocationRequested { .. } => "device.location_requested",
            DeviceEventPayload::LocationUpdated { .. } => "device.location_updated",
            DeviceEventPayload::SensorUpdated { .. } => "device.sensor_updated",
            DeviceEventPayload::BatteryLow { .. } => "device.battery_low",
            DeviceEventPayload::BatteryCharging => "device.battery_charging",
            DeviceEventPayload::BatteryDischarging => "device.battery_discharging",
            DeviceEventPayload::BatteryFull => "device.battery_full",
            DeviceEventPayload::StorageLow { .. } => "device.storage_low",
            DeviceEventPayload::StorageCritical { .. } => "device.storage_critical",
            DeviceEventPayload::PermissionGranted { .. } => "device.permission_granted",
            DeviceEventPayload::PermissionDenied { .. } => "device.permission_denied",
            DeviceEventPayload::BiometricSucceeded => "device.biometric_succeeded",
            DeviceEventPayload::BiometricFailed { .. } => "device.biometric_failed",
            DeviceEventPayload::DeviceToolInvoked { .. } => "device.tool_invoked",
            DeviceEventPayload::ConnectivityChanged { .. } => "device.connectivity_changed",
            DeviceEventPayload::InstalledAppsChanged => "device.installed_apps_changed",
            DeviceEventPayload::FileOpened { .. } => "device.file_opened",
            DeviceEventPayload::MediaPicked { .. } => "device.media_picked",
            DeviceEventPayload::DownloadCompleted { .. } => "device.download_completed",
            DeviceEventPayload::ShareSheetOpened => "device.share_sheet_opened",
        }
    }

    pub fn description(&self) -> String {
        match &self.payload {
            DeviceEventPayload::CameraOpened { camera_id } => {
                format!("Camera '{camera_id}' opened")
            }
            DeviceEventPayload::PhotoCaptured { path } => format!("Photo captured: {path}"),
            DeviceEventPayload::GallerySelected { paths } => {
                format!("{} file(s) selected from gallery", paths.len())
            }
            DeviceEventPayload::ClipboardRead { content_len } => {
                format!("Clipboard read ({content_len} chars)")
            }
            DeviceEventPayload::ClipboardWritten { content_len } => {
                format!("Clipboard written ({content_len} chars)")
            }
            DeviceEventPayload::ClipboardHistoryCleared => "Clipboard history cleared".to_string(),
            DeviceEventPayload::NotificationPosted { package, title } => {
                format!("Notification from {package}: {title}")
            }
            DeviceEventPayload::NotificationClicked { package, action } => {
                format!("Notification {package} clicked: {action}")
            }
            DeviceEventPayload::CalendarEventCreated { title } => {
                format!("Calendar event created: {title}")
            }
            DeviceEventPayload::CalendarEventUpdated { title } => {
                format!("Calendar event updated: {title}")
            }
            DeviceEventPayload::CalendarEventDeleted { title } => {
                format!("Calendar event deleted: {title}")
            }
            DeviceEventPayload::ContactAccessed { contact_id } => {
                format!("Contact accessed: {contact_id}")
            }
            DeviceEventPayload::ContactCreated { contact_id } => {
                format!("Contact created: {contact_id}")
            }
            DeviceEventPayload::ContactUpdated { contact_id } => {
                format!("Contact updated: {contact_id}")
            }
            DeviceEventPayload::ContactDeleted { contact_id } => {
                format!("Contact deleted: {contact_id}")
            }
            DeviceEventPayload::SmsSent { recipient } => format!("SMS sent to {recipient}"),
            DeviceEventPayload::SmsReceived { sender } => format!("SMS received from {sender}"),
            DeviceEventPayload::PhoneCallDialed { number } => {
                format!("Phone call dialed: {number}")
            }
            DeviceEventPayload::PhoneCallReceived { caller } => format!("Phone call from {caller}"),
            DeviceEventPayload::LocationRequested { precision } => {
                format!("Location requested ({precision})")
            }
            DeviceEventPayload::LocationUpdated { lat, lng, accuracy } => {
                format!("Location: {lat:.4},{lng:.4} ±{accuracy:.1}m")
            }
            DeviceEventPayload::SensorUpdated {
                sensor_type,
                values,
            } => format!("Sensor '{sensor_type}': {values:?}"),
            DeviceEventPayload::BatteryLow { level } => format!("Battery low: {level}%"),
            DeviceEventPayload::BatteryCharging => "Battery charging".to_string(),
            DeviceEventPayload::BatteryDischarging => "Battery discharging".to_string(),
            DeviceEventPayload::BatteryFull => "Battery full".to_string(),
            DeviceEventPayload::StorageLow { free_bytes } => {
                format!("Storage low: {free_bytes} bytes free")
            }
            DeviceEventPayload::StorageCritical { free_bytes } => {
                format!("Storage critical: {free_bytes} bytes free")
            }
            DeviceEventPayload::PermissionGranted { capability } => {
                format!("Permission granted: {capability}")
            }
            DeviceEventPayload::PermissionDenied { capability, reason } => {
                format!("Permission denied: {capability} ({reason})")
            }
            DeviceEventPayload::BiometricSucceeded => "Biometric auth succeeded".to_string(),
            DeviceEventPayload::BiometricFailed { reason } => {
                format!("Biometric auth failed: {reason}")
            }
            DeviceEventPayload::DeviceToolInvoked {
                tool,
                duration_ms,
                success,
            } => format!("Tool '{tool}' invoked: {duration_ms}ms, success={success}"),
            DeviceEventPayload::ConnectivityChanged {
                online,
                network_type,
            } => format!("Connectivity: {network_type} online={online}"),
            DeviceEventPayload::InstalledAppsChanged => "Installed apps changed".to_string(),
            DeviceEventPayload::FileOpened { path } => format!("File opened: {path}"),
            DeviceEventPayload::MediaPicked { mime_type } => format!("Media picked: {mime_type}"),
            DeviceEventPayload::DownloadCompleted { path } => format!("Download completed: {path}"),
            DeviceEventPayload::ShareSheetOpened => "Share sheet opened".to_string(),
        }
    }
}
