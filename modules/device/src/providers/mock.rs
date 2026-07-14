use super::*;
use async_trait::async_trait;
use nova_kernel::{ErrorCategory, NovaError, Result};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct MockDeviceProvider {
    clipboard: RwLock<Vec<ClipboardEntry>>,
    contacts: RwLock<Vec<Contact>>,
    events: RwLock<Vec<CalendarEvent>>,
    battery: RwLock<BatteryStatus>,
    storage: RwLock<StorageInfo>,
    location: RwLock<Location>,
    sensors: RwLock<Vec<SensorReading>>,
    photos: RwLock<Vec<PhotoCapture>>,
    next_id: AtomicU64,
}

impl Default for MockDeviceProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockDeviceProvider {
    pub fn new() -> Self {
        Self {
            clipboard: RwLock::new(vec![]),
            contacts: RwLock::new(vec![]),
            events: RwLock::new(vec![]),
            battery: RwLock::new(BatteryStatus {
                level: 85,
                is_charging: true,
                temperature: 32.0,
            }),
            storage: RwLock::new(StorageInfo {
                total_bytes: 128_000_000_000,
                free_bytes: 64_000_000_000,
                used_bytes: 64_000_000_000,
            }),
            location: RwLock::new(Location {
                latitude: 37.7749,
                longitude: -122.4194,
                accuracy: 10.0,
            }),
            sensors: RwLock::new(vec![
                SensorReading {
                    sensor_type: "accelerometer".to_string(),
                    values: vec![0.1, -0.2, 9.8],
                    timestamp: chrono::Local::now().to_rfc3339(),
                },
                SensorReading {
                    sensor_type: "gyroscope".to_string(),
                    values: vec![0.01, -0.02, 0.005],
                    timestamp: chrono::Local::now().to_rfc3339(),
                },
            ]),
            photos: RwLock::new(vec![]),
            next_id: AtomicU64::new(1),
        }
    }

    fn next_id(&self) -> String {
        format!("mock_{}", self.next_id.fetch_add(1, Ordering::SeqCst))
    }
}

#[async_trait]
impl DeviceProvider for MockDeviceProvider {
    async fn open_camera(&self, camera_id: &str) -> Result<String> {
        Ok(format!("Camera '{camera_id}' opened (mock)"))
    }

    async fn capture_photo(&self) -> Result<PhotoCapture> {
        let photo = PhotoCapture {
            path: format!(
                "/mock/photos/IMG_{}.jpg",
                chrono::Local::now().format("%Y%m%d_%H%M%S")
            ),
            width: 4032,
            height: 3024,
            size_bytes: 2_500_000,
        };
        self.photos.write().push(photo.clone());
        Ok(photo)
    }

    async fn pick_from_gallery(&self, mime_types: &[String]) -> Result<Vec<MediaFile>> {
        Ok(vec![MediaFile {
            id: self.next_id(),
            path: "/mock/gallery/photo_2024.jpg".to_string(),
            mime_type: mime_types
                .first()
                .cloned()
                .unwrap_or_else(|| "image/jpeg".to_string()),
            size_bytes: 1_500_000,
            date_added: chrono::Local::now().to_rfc3339(),
        }])
    }

    async fn read_clipboard(&self) -> Result<ClipboardEntry> {
        let content = self
            .clipboard
            .read()
            .last()
            .cloned()
            .unwrap_or(ClipboardEntry {
                content: String::new(),
                copied_at: chrono::Local::now().to_rfc3339(),
                app_source: "mock".to_string(),
            });
        Ok(content)
    }

    async fn write_clipboard(&self, content: &str) -> Result<()> {
        self.clipboard.write().push(ClipboardEntry {
            content: content.to_string(),
            copied_at: chrono::Local::now().to_rfc3339(),
            app_source: "nova_mock".to_string(),
        });
        Ok(())
    }

    async fn get_clipboard_history(&self) -> Result<Vec<ClipboardEntry>> {
        Ok(self.clipboard.read().clone())
    }

    async fn clear_clipboard_history(&self) -> Result<()> {
        self.clipboard.write().clear();
        Ok(())
    }

    async fn post_notification(&self, title: &str, text: &str, _package: &str) -> Result<()> {
        tracing::info!("[mock] Notification: {title} — {text}");
        Ok(())
    }

    async fn get_notifications(&self) -> Result<Vec<NotificationInfo>> {
        Ok(vec![])
    }

    async fn list_calendar_events(&self, _from: &str, _to: &str) -> Result<Vec<CalendarEvent>> {
        Ok(self.events.read().clone())
    }

    async fn create_calendar_event(&self, event: &CalendarEvent) -> Result<String> {
        let id = self.next_id();
        let mut e = event.clone();
        e.id = id.clone();
        self.events.write().push(e);
        Ok(id)
    }

    async fn update_calendar_event(&self, event: &CalendarEvent) -> Result<()> {
        let mut events = self.events.write();
        if let Some(e) = events.iter_mut().find(|e| e.id == event.id) {
            *e = event.clone();
        }
        Ok(())
    }

    async fn delete_calendar_event(&self, id: &str) -> Result<()> {
        self.events.write().retain(|e| e.id != id);
        Ok(())
    }

    async fn list_contacts(&self) -> Result<Vec<Contact>> {
        Ok(self.contacts.read().clone())
    }

    async fn get_contact(&self, id: &str) -> Result<Contact> {
        self.contacts
            .read()
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| {
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_DEVICE_CONTACT_NOT_FOUND",
                    "Contact not found",
                )
            })
    }

    async fn create_contact(&self, contact: &Contact) -> Result<String> {
        let id = self.next_id();
        let mut c = contact.clone();
        c.id = id.clone();
        self.contacts.write().push(c);
        Ok(id)
    }

    async fn update_contact(&self, contact: &Contact) -> Result<()> {
        let mut contacts = self.contacts.write();
        if let Some(c) = contacts.iter_mut().find(|c| c.id == contact.id) {
            *c = contact.clone();
        }
        Ok(())
    }

    async fn delete_contact(&self, id: &str) -> Result<()> {
        self.contacts.write().retain(|c| c.id != id);
        Ok(())
    }

    async fn send_sms(&self, recipient: &str, _message: &str) -> Result<()> {
        tracing::info!("[mock] SMS sent to {recipient}");
        Ok(())
    }

    async fn read_sms(&self) -> Result<Vec<(String, String, String)>> {
        Ok(vec![(
            "+1234567890".to_string(),
            "Hello from NOVA mock".to_string(),
            chrono::Local::now().to_rfc3339(),
        )])
    }

    async fn dial_phone(&self, number: &str) -> Result<()> {
        tracing::info!("[mock] Dialing {number}");
        Ok(())
    }

    async fn get_location(&self) -> Result<Location> {
        Ok(self.location.read().clone())
    }

    async fn get_sensor_readings(&self) -> Result<Vec<SensorReading>> {
        Ok(self.sensors.read().clone())
    }

    async fn get_battery_status(&self) -> Result<BatteryStatus> {
        Ok(self.battery.read().clone())
    }

    async fn get_storage_info(&self) -> Result<StorageInfo> {
        Ok(self.storage.read().clone())
    }

    async fn list_installed_apps(&self) -> Result<Vec<AppInfo>> {
        Ok(vec![
            AppInfo {
                package_name: "com.example.nova".to_string(),
                label: "NOVA".to_string(),
                version: "1.0.0".to_string(),
            },
            AppInfo {
                package_name: "com.android.chrome".to_string(),
                label: "Chrome".to_string(),
                version: "120.0.0".to_string(),
            },
            AppInfo {
                package_name: "org.telegram.messenger".to_string(),
                label: "Telegram".to_string(),
                version: "10.0.0".to_string(),
            },
        ])
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        Ok(format!("[mock content of '{path}']").into_bytes())
    }

    async fn write_file(&self, path: &str, _data: &[u8]) -> Result<()> {
        tracing::info!("[mock] Written to {path}");
        Ok(())
    }

    async fn list_files(&self, directory: &str) -> Result<Vec<String>> {
        Ok(vec![
            format!("{directory}/document1.pdf"),
            format!("{directory}/photo.jpg"),
            format!("{directory}/notes.txt"),
        ])
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        tracing::info!("[mock] Deleted {path}");
        Ok(())
    }

    async fn list_downloads(&self) -> Result<Vec<MediaFile>> {
        Ok(vec![MediaFile {
            id: self.next_id(),
            path: "/mock/downloads/report.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            size_bytes: 500_000,
            date_added: chrono::Local::now().to_rfc3339(),
        }])
    }

    async fn pick_media(&self, mime_types: &[String]) -> Result<Vec<MediaFile>> {
        Ok(vec![MediaFile {
            id: self.next_id(),
            path: "/mock/media/selected_file.jpg".to_string(),
            mime_type: mime_types
                .first()
                .cloned()
                .unwrap_or_else(|| "image/jpeg".to_string()),
            size_bytes: 2_000_000,
            date_added: chrono::Local::now().to_rfc3339(),
        }])
    }

    async fn authenticate_biometric(&self, _reason: &str) -> Result<BiometricResult> {
        Ok(BiometricResult {
            success: true,
            method: "mock_fingerprint".to_string(),
            error: None,
        })
    }

    async fn get_connectivity_status(&self) -> Result<(bool, String)> {
        Ok((true, "wifi".to_string()))
    }

    async fn open_share_sheet(&self, _text: &str, _mime_type: &str) -> Result<()> {
        tracing::info!("[mock] Share sheet opened");
        Ok(())
    }
}
