use nova_ai::tool::Tool;
use nova_device::events::{DeviceEvent, DeviceEventPayload};
use nova_device::permission::{DeviceCapability, PermissionManager, PermissionState};
use nova_device::providers::mock::MockDeviceProvider;
use nova_device::providers::DeviceProvider;
use nova_device::tools::*;
use std::sync::Arc;

// ── Permission System Tests ───────────────────────────────────────────────────

#[test]
fn test_permission_grant_and_check() {
    let pm = PermissionManager::new();
    pm.grant(&DeviceCapability::Camera);
    assert!(pm.is_granted(&DeviceCapability::Camera));
    assert_eq!(
        pm.check(&DeviceCapability::Camera),
        PermissionState::Granted
    );
}

#[test]
fn test_permission_deny() {
    let pm = PermissionManager::new();
    pm.deny(&DeviceCapability::Location, "User privacy");
    assert_eq!(
        pm.check(&DeviceCapability::Location),
        PermissionState::Denied
    );
}

#[test]
fn test_permission_revoke() {
    let pm = PermissionManager::new();
    pm.grant(&DeviceCapability::Camera);
    pm.revoke(&DeviceCapability::Camera);
    assert_eq!(
        pm.check(&DeviceCapability::Camera),
        PermissionState::NotRequested
    );
}

#[test]
fn test_permission_not_requested_default() {
    let pm = PermissionManager::new();
    assert_eq!(
        pm.check(&DeviceCapability::Camera),
        PermissionState::NotRequested
    );
}

#[test]
fn test_biometric_required_capabilities() {
    assert!(DeviceCapability::SmsSend.requires_biometric());
    assert!(DeviceCapability::PhoneCall.requires_biometric());
    assert!(DeviceCapability::ContactsWrite.requires_biometric());
    assert!(DeviceCapability::CalendarWrite.requires_biometric());
    assert!(!DeviceCapability::Camera.requires_biometric());
    assert!(!DeviceCapability::ClipboardRead.requires_biometric());
}

#[test]
fn test_permission_audit_log() {
    let pm = PermissionManager::new();
    pm.grant(&DeviceCapability::Camera);
    pm.deny(&DeviceCapability::Location, "Not needed");
    pm.revoke(&DeviceCapability::ClipboardRead);
    let audit = pm.audit_log();
    assert_eq!(audit.len(), 3);
    assert!(audit[0].result.contains("Granted"));
    assert!(audit[1].result.contains("Denied"));
    assert!(audit[2].result.contains("Revoked"));
}

#[test]
fn test_list_grants() {
    let pm = PermissionManager::new();
    pm.grant(&DeviceCapability::Camera);
    pm.grant(&DeviceCapability::ClipboardRead);
    pm.grant(&DeviceCapability::Location);
    let grants = pm.list_grants();
    assert_eq!(grants.len(), 3);
}

#[test]
fn test_permission_request_flow() {
    let pm = PermissionManager::new();
    let result = pm.request(&DeviceCapability::Camera, "Need for photo");
    assert!(matches!(
        result,
        nova_device::permission::PermissionResult::Granted
    ));
    assert!(pm.is_granted(&DeviceCapability::Camera));
}

#[test]
fn test_request_biometric_requires_confirmation() {
    let pm = PermissionManager::new();
    let result = pm.request(&DeviceCapability::SmsSend, "Send SMS");
    assert!(matches!(
        result,
        nova_device::permission::PermissionResult::RequiresBiometric
    ));
}

// ── Mock Device Provider Tests ────────────────────────────────────────────────

#[tokio::test]
async fn test_mock_camera_capture() {
    let provider = MockDeviceProvider::new();
    let result = provider.capture_photo().await;
    assert!(result.is_ok());
    let photo = result.unwrap();
    assert!(photo.path.contains("IMG_"));
    assert!(photo.width > 0);
    assert!(photo.height > 0);
}

#[tokio::test]
async fn test_mock_clipboard_write_read() {
    let provider = MockDeviceProvider::new();
    provider.write_clipboard("Hello NOVA").await.unwrap();
    let entry = provider.read_clipboard().await.unwrap();
    assert_eq!(entry.content, "Hello NOVA");
}

#[tokio::test]
async fn test_mock_clipboard_history() {
    let provider = MockDeviceProvider::new();
    provider.write_clipboard("item1").await.unwrap();
    provider.write_clipboard("item2").await.unwrap();
    let history = provider.get_clipboard_history().await.unwrap();
    assert_eq!(history.len(), 2);
    provider.clear_clipboard_history().await.unwrap();
    let history = provider.get_clipboard_history().await.unwrap();
    assert_eq!(history.len(), 0);
}

#[tokio::test]
async fn test_mock_location() {
    let provider = MockDeviceProvider::new();
    let loc = provider.get_location().await.unwrap();
    assert!(loc.latitude != 0.0);
    assert!(loc.longitude != 0.0);
    assert!(loc.accuracy > 0.0);
}

#[tokio::test]
async fn test_mock_battery() {
    let provider = MockDeviceProvider::new();
    let status = provider.get_battery_status().await.unwrap();
    assert!(status.level > 0);
    assert!(status.temperature > 0.0);
}

#[tokio::test]
async fn test_mock_storage() {
    let provider = MockDeviceProvider::new();
    let info = provider.get_storage_info().await.unwrap();
    assert!(info.total_bytes > 0);
    assert!(info.free_bytes > 0);
    assert_eq!(info.total_bytes, info.free_bytes + info.used_bytes);
}

#[tokio::test]
async fn test_mock_sensors() {
    let provider = MockDeviceProvider::new();
    let readings = provider.get_sensor_readings().await.unwrap();
    assert!(!readings.is_empty());
    assert_eq!(readings[0].sensor_type, "accelerometer");
    assert_eq!(readings[0].values.len(), 3);
}

#[tokio::test]
async fn test_mock_contacts_crud() {
    let provider = MockDeviceProvider::new();
    let id = provider
        .create_contact(&nova_device::providers::Contact {
            id: String::new(),
            name: "Alice".to_string(),
            phone: vec!["+1234567890".to_string()],
            email: vec!["alice@example.com".to_string()],
        })
        .await
        .unwrap();
    assert!(!id.is_empty());

    let contact = provider.get_contact(&id).await.unwrap();
    assert_eq!(contact.name, "Alice");

    provider.delete_contact(&id).await.unwrap();
    assert!(provider.get_contact(&id).await.is_err());
}

#[tokio::test]
async fn test_mock_calendar_crud() {
    let provider = MockDeviceProvider::new();
    let id = provider
        .create_calendar_event(&nova_device::providers::CalendarEvent {
            id: String::new(),
            title: "Meeting".to_string(),
            description: "Project sync".to_string(),
            start_time: "2026-07-14T10:00:00Z".to_string(),
            end_time: "2026-07-14T11:00:00Z".to_string(),
            location: "Office".to_string(),
        })
        .await
        .unwrap();
    assert!(!id.is_empty());

    let events = provider.list_calendar_events("", "").await.unwrap();
    assert_eq!(events.len(), 1);

    provider.delete_calendar_event(&id).await.unwrap();
    let events = provider.list_calendar_events("", "").await.unwrap();
    assert_eq!(events.len(), 0);
}

#[tokio::test]
async fn test_mock_sms() {
    let provider = MockDeviceProvider::new();
    let result = provider.send_sms("+1234567890", "Hello").await;
    assert!(result.is_ok());

    let sms_list = provider.read_sms().await.unwrap();
    assert!(!sms_list.is_empty());
    assert_eq!(sms_list[0].0, "+1234567890");
}

#[tokio::test]
async fn test_mock_biometric() {
    let provider = MockDeviceProvider::new();
    let result = provider.authenticate_biometric("Test").await.unwrap();
    assert!(result.success);
    assert_eq!(result.method, "mock_fingerprint");
}

#[tokio::test]
async fn test_mock_connectivity() {
    let provider = MockDeviceProvider::new();
    let (online, net_type) = provider.get_connectivity_status().await.unwrap();
    assert!(online);
    assert_eq!(net_type, "wifi");
}

#[tokio::test]
async fn test_mock_installed_apps() {
    let provider = MockDeviceProvider::new();
    let apps = provider.list_installed_apps().await.unwrap();
    assert!(apps.len() >= 3);
    assert_eq!(apps[0].package_name, "com.example.nova");
}

#[tokio::test]
async fn test_mock_files() {
    let provider = MockDeviceProvider::new();
    let files = provider.list_files("/mock/docs").await.unwrap();
    assert_eq!(files.len(), 3);

    let data = provider
        .read_file("/mock/docs/document1.pdf")
        .await
        .unwrap();
    assert!(!data.is_empty());
}

#[tokio::test]
async fn test_mock_gallery() {
    let provider = MockDeviceProvider::new();
    let mime_types = vec!["image/jpeg".to_string()];
    let files = provider.pick_from_gallery(&mime_types).await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].mime_type, "image/jpeg");
}

#[tokio::test]
async fn test_mock_media_picker() {
    let provider = MockDeviceProvider::new();
    let mime_types = vec!["image/*".to_string()];
    let media = provider.pick_media(&mime_types).await.unwrap();
    assert_eq!(media.len(), 1);
}

#[tokio::test]
async fn test_mock_downloads() {
    let provider = MockDeviceProvider::new();
    let downloads = provider.list_downloads().await.unwrap();
    assert_eq!(downloads.len(), 1);
}

#[tokio::test]
async fn test_mock_share_sheet() {
    let provider = MockDeviceProvider::new();
    let result = provider.open_share_sheet("Test text", "text/plain").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_mock_phone() {
    let provider = MockDeviceProvider::new();
    let result = provider.dial_phone("+1234567890").await;
    assert!(result.is_ok());
}

// ── Tool Integration Tests ────────────────────────────────────────────────────

#[tokio::test]
async fn test_all_tools_require_permission() {
    let permissions = Arc::new(PermissionManager::new());
    let audit = Arc::new(parking_lot::RwLock::new(vec![]));
    let provider = Arc::new(MockDeviceProvider::new()) as Arc<dyn DeviceProvider>;

    let tools: Vec<Arc<dyn nova_ai::tool::Tool>> = vec![
        Arc::new(CameraTool::new(
            provider.clone(),
            permissions.clone(),
            audit.clone(),
        )),
        Arc::new(ClipboardReadTool::new(
            provider.clone(),
            permissions.clone(),
            audit.clone(),
        )),
        Arc::new(LocationTool::new(
            provider.clone(),
            permissions.clone(),
            audit.clone(),
        )),
        Arc::new(BatteryTool::new(
            provider.clone(),
            permissions.clone(),
            audit.clone(),
        )),
    ];

    for tool in &tools {
        let result = tool.invoke("{}").await;
        assert!(
            result.is_err(),
            "Tool '{}' should fail without permission",
            tool.spec().name
        );
    }
}

#[tokio::test]
async fn test_device_toolkit_composition() {
    let permissions = Arc::new(PermissionManager::new());
    let audit = Arc::new(parking_lot::RwLock::new(vec![]));
    let provider = Arc::new(MockDeviceProvider::new()) as Arc<dyn DeviceProvider>;
    let toolkit = DeviceToolkit::new(provider, permissions, audit);
    assert_eq!(toolkit.count(), 17);
}

#[tokio::test]
async fn test_permission_logged_in_audit() {
    let permissions = Arc::new(PermissionManager::new());
    let audit = Arc::new(parking_lot::RwLock::new(vec![]));
    let provider = Arc::new(MockDeviceProvider::new()) as Arc<dyn DeviceProvider>;

    permissions.grant(&DeviceCapability::Battery);
    let tool = BatteryTool::new(provider, permissions, audit.clone());
    let result = tool.invoke("{}").await;
    assert!(result.is_ok());

    let events = audit.read();
    let tool_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.payload, DeviceEventPayload::DeviceToolInvoked { .. }))
        .collect();
    assert_eq!(tool_events.len(), 1);
}

// ── DeviceEvent Tests ─────────────────────────────────────────────────────────

#[test]
fn test_device_event_action_names() {
    let uuid = uuid::Uuid::new_v4();

    let camera = DeviceEvent::new(
        uuid,
        DeviceEventPayload::CameraOpened {
            camera_id: "0".into(),
        },
    );
    assert_eq!(camera.action_name(), "device.camera_opened");

    let photo = DeviceEvent::new(
        uuid,
        DeviceEventPayload::PhotoCaptured {
            path: "/a.jpg".into(),
        },
    );
    assert_eq!(photo.action_name(), "device.photo_captured");

    let clip = DeviceEvent::new(uuid, DeviceEventPayload::ClipboardRead { content_len: 10 });
    assert_eq!(clip.action_name(), "device.clipboard_read");
}

#[test]
fn test_device_event_descriptions() {
    let uuid = uuid::Uuid::new_v4();
    let event = DeviceEvent::new(uuid, DeviceEventPayload::BatteryLow { level: 15 });
    assert!(event.description().contains("15%"));

    let event = DeviceEvent::new(
        uuid,
        DeviceEventPayload::LocationUpdated {
            lat: 37.7749,
            lng: -122.4194,
            accuracy: 10.0,
        },
    );
    assert!(event.description().contains("37.7749"));
}

#[test]
fn test_device_capability_names() {
    assert_eq!(DeviceCapability::Camera.name(), "camera");
    assert_eq!(DeviceCapability::ClipboardRead.name(), "clipboard_read");
    assert_eq!(DeviceCapability::SmsSend.name(), "sms_send");
    assert_eq!(DeviceCapability::Biometric.name(), "biometric");
}

// ── Config Tests ──────────────────────────────────────────────────────────────

#[test]
fn test_device_config_defaults() {
    let config = nova_device::DeviceConfig::default();
    assert_eq!(config.clipboard_history_size, 50);
    assert_eq!(config.sensor_polling_interval_ms, 5000);
    assert_eq!(config.battery_low_threshold_pct, 20);
    assert_eq!(config.battery_critical_threshold_pct, 5);
    assert!(!config.notification_filter_enabled);
    assert_eq!(config.location_precision, "coarse");
}
