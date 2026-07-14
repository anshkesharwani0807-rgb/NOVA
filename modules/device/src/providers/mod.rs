pub mod mock;

use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub phone: Vec<String>,
    pub email: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub description: String,
    pub start_time: String,
    pub end_time: String,
    pub location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReading {
    pub sensor_type: String,
    pub values: Vec<f64>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryStatus {
    pub level: u8,
    pub is_charging: bool,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInfo {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub package_name: String,
    pub label: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationInfo {
    pub package_name: String,
    pub title: String,
    pub text: String,
    pub posted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub content: String,
    pub copied_at: String,
    pub app_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFile {
    pub id: String,
    pub path: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub date_added: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoCapture {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricResult {
    pub success: bool,
    pub method: String,
    pub error: Option<String>,
}

/// Platform-agnostic device capability provider.
/// Mock implementation exists for testing/demo; real Android provider
/// will use JNI to call Android framework APIs.
#[async_trait]
pub trait DeviceProvider: Send + Sync {
    // Camera
    async fn open_camera(&self, camera_id: &str) -> Result<String>;
    async fn capture_photo(&self) -> Result<PhotoCapture>;

    // Gallery
    async fn pick_from_gallery(&self, mime_types: &[String]) -> Result<Vec<MediaFile>>;

    // Clipboard
    async fn read_clipboard(&self) -> Result<ClipboardEntry>;
    async fn write_clipboard(&self, content: &str) -> Result<()>;
    async fn get_clipboard_history(&self) -> Result<Vec<ClipboardEntry>>;
    async fn clear_clipboard_history(&self) -> Result<()>;

    // Notifications
    async fn post_notification(&self, title: &str, text: &str, package: &str) -> Result<()>;
    async fn get_notifications(&self) -> Result<Vec<NotificationInfo>>;

    // Calendar
    async fn list_calendar_events(&self, from: &str, to: &str) -> Result<Vec<CalendarEvent>>;
    async fn create_calendar_event(&self, event: &CalendarEvent) -> Result<String>;
    async fn update_calendar_event(&self, event: &CalendarEvent) -> Result<()>;
    async fn delete_calendar_event(&self, id: &str) -> Result<()>;

    // Contacts
    async fn list_contacts(&self) -> Result<Vec<Contact>>;
    async fn get_contact(&self, id: &str) -> Result<Contact>;
    async fn create_contact(&self, contact: &Contact) -> Result<String>;
    async fn update_contact(&self, contact: &Contact) -> Result<()>;
    async fn delete_contact(&self, id: &str) -> Result<()>;

    // SMS
    async fn send_sms(&self, recipient: &str, message: &str) -> Result<()>;
    async fn read_sms(&self) -> Result<Vec<(String, String, String)>>;

    // Phone
    async fn dial_phone(&self, number: &str) -> Result<()>;

    // Location
    async fn get_location(&self) -> Result<Location>;

    // Sensors
    async fn get_sensor_readings(&self) -> Result<Vec<SensorReading>>;

    // Battery
    async fn get_battery_status(&self) -> Result<BatteryStatus>;

    // Storage
    async fn get_storage_info(&self) -> Result<StorageInfo>;

    // Installed Apps
    async fn list_installed_apps(&self) -> Result<Vec<AppInfo>>;

    // Files
    async fn read_file(&self, path: &str) -> Result<Vec<u8>>;
    async fn write_file(&self, path: &str, data: &[u8]) -> Result<()>;
    async fn list_files(&self, directory: &str) -> Result<Vec<String>>;
    async fn delete_file(&self, path: &str) -> Result<()>;

    // Downloads
    async fn list_downloads(&self) -> Result<Vec<MediaFile>>;

    // Media Picker
    async fn pick_media(&self, mime_types: &[String]) -> Result<Vec<MediaFile>>;

    // Biometric
    async fn authenticate_biometric(&self, reason: &str) -> Result<BiometricResult>;

    // Connectivity
    async fn get_connectivity_status(&self) -> Result<(bool, String)>;

    // Share Sheet
    async fn open_share_sheet(&self, text: &str, mime_type: &str) -> Result<()>;
}
