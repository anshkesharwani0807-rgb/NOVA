use crate::events::{DeviceEvent, DeviceEventPayload};
use crate::permission::{DeviceCapability, PermissionManager};
use crate::providers::DeviceProvider;
use async_trait::async_trait;
use nova_ai::tool::{Tool, ToolSpec};
use nova_kernel::{ErrorCategory, NovaError, Result};
use parking_lot::RwLock;
use std::sync::Arc;
use uuid::Uuid;

pub trait DeviceTool: Tool {
    fn capability(&self) -> DeviceCapability;
}

struct ToolContext {
    provider: Arc<dyn DeviceProvider>,
    permissions: Arc<PermissionManager>,
    audit: Arc<RwLock<Vec<DeviceEvent>>>,
}

impl ToolContext {
    fn new(
        provider: Arc<dyn DeviceProvider>,
        permissions: Arc<PermissionManager>,
        audit: Arc<RwLock<Vec<DeviceEvent>>>,
    ) -> Self {
        Self {
            provider,
            permissions,
            audit,
        }
    }

    async fn check_permission(&self, cap: &DeviceCapability) -> Result<()> {
        if !self.permissions.is_granted(cap) {
            return Err(NovaError::new(
                ErrorCategory::ConsentRequired,
                "ERR_DEVICE_PERMISSION_DENIED",
                &format!("Permission denied for '{}'", cap.name()),
            ));
        }
        Ok(())
    }

    fn log(&self, payload: DeviceEventPayload) {
        let event = DeviceEvent::new(Uuid::new_v4(), payload);
        let action = event.action_name();
        let desc = event.description();
        nova_kernel::log_activity("device", action, &desc, Some(event.correlation_id));
        self.audit.write().push(event);
    }
}

macro_rules! device_tool {
    ($name:ident, $spec_name:expr, $desc:expr, $params:expr, $cap:ident) => {
        pub struct $name {
            ctx: ToolContext,
        }
        impl $name {
            pub fn new(
                provider: Arc<dyn DeviceProvider>,
                permissions: Arc<PermissionManager>,
                audit: Arc<RwLock<Vec<DeviceEvent>>>,
            ) -> Self {
                Self {
                    ctx: ToolContext::new(provider, permissions, audit),
                }
            }
        }
        #[async_trait]
        impl Tool for $name {
            fn spec(&self) -> ToolSpec {
                ToolSpec::new($spec_name, $desc, $params)
            }
            async fn invoke(&self, arguments: &str) -> Result<String> {
                self.ctx.check_permission(&DeviceCapability::$cap).await?;
                let start = std::time::Instant::now();
                let result = self.execute(arguments).await;
                let duration_ms = start.elapsed().as_millis() as u64;
                let success = result.is_ok();
                self.ctx.log(DeviceEventPayload::DeviceToolInvoked {
                    tool: $spec_name.to_string(),
                    duration_ms,
                    success,
                });
                result
            }
        }
        #[async_trait]
        impl DeviceTool for $name {
            fn capability(&self) -> DeviceCapability {
                DeviceCapability::$cap
            }
        }
    };
}

device_tool!(
    CameraTool,
    "camera",
    "Capture a photo using the device camera",
    r#"{"type":"object","properties":{}}"#,
    Camera
);

impl CameraTool {
    async fn execute(&self, _arguments: &str) -> Result<String> {
        let result = self.ctx.provider.capture_photo().await?;
        Ok(serde_json::json!({
            "path": result.path,
            "width": result.width,
            "height": result.height,
            "size_bytes": result.size_bytes,
        })
        .to_string())
    }
}

device_tool!(
    GalleryTool,
    "gallery",
    "Browse or select files from the device gallery",
    r#"{"type":"object","properties":{"mime_types":{"type":"array","items":{"type":"string"}}}}"#,
    GalleryRead
);

impl GalleryTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value =
            serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));
        let mime_types: Vec<String> = args
            .get("mime_types")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let files = self.ctx.provider.pick_from_gallery(&mime_types).await?;
        let json: serde_json::Value = serde_json::to_value(&files).unwrap_or(serde_json::json!([]));
        Ok(json.to_string())
    }
}

device_tool!(
    ClipboardReadTool,
    "clipboard_read",
    "Read the current clipboard contents",
    r#"{"type":"object","properties":{}}"#,
    ClipboardRead
);

impl ClipboardReadTool {
    async fn execute(&self, _arguments: &str) -> Result<String> {
        let entry = self.ctx.provider.read_clipboard().await?;
        Ok(serde_json::json!({
            "content": entry.content,
            "copied_at": entry.copied_at,
            "app_source": entry.app_source,
        })
        .to_string())
    }
}

device_tool!(
    ClipboardWriteTool,
    "clipboard_write",
    "Write content to the device clipboard",
    r#"{"type":"object","properties":{"content":{"type":"string"}},"required":["content"]}"#,
    ClipboardWrite
);

impl ClipboardWriteTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value =
            serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        self.ctx.provider.write_clipboard(content).await?;
        Ok(format!("Written {} chars to clipboard", content.len()))
    }
}

device_tool!(
    CalendarTool,
    "calendar",
    "List, create, or manage calendar events",
    r#"{"type":"object","properties":{"action":{"type":"string","enum":["list","create","update","delete"]},"title":{"type":"string"}},"required":["action"]}"#,
    CalendarRead
);

impl CalendarTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value =
            serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");
        match action {
            "create" => {
                let event = crate::providers::CalendarEvent {
                    id: String::new(),
                    title: args
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    description: args
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    start_time: args
                        .get("start_time")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    end_time: args
                        .get("end_time")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    location: args
                        .get("location")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                };
                let id = self.ctx.provider.create_calendar_event(&event).await?;
                Ok(format!("Calendar event created: {id}"))
            }
            "list" => {
                let events = self.ctx.provider.list_calendar_events("", "").await?;
                let json: serde_json::Value =
                    serde_json::to_value(&events).unwrap_or(serde_json::json!([]));
                Ok(json.to_string())
            }
            "delete" => {
                let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
                self.ctx.provider.delete_calendar_event(id).await?;
                Ok(format!("Calendar event deleted: {id}"))
            }
            _ => Ok("Unknown action".to_string()),
        }
    }
}

device_tool!(
    ContactsTool,
    "contacts",
    "List, search, or manage device contacts",
    r#"{"type":"object","properties":{"action":{"type":"string","enum":["list","get","create","delete"]},"id":{"type":"string"},"name":{"type":"string"}},"required":["action"]}"#,
    ContactsRead
);

impl ContactsTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value =
            serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");
        match action {
            "list" => {
                let contacts = self.ctx.provider.list_contacts().await?;
                let json: serde_json::Value =
                    serde_json::to_value(&contacts).unwrap_or(serde_json::json!([]));
                Ok(json.to_string())
            }
            "get" => {
                let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let contact = self.ctx.provider.get_contact(id).await?;
                let json: serde_json::Value = serde_json::to_value(&contact).unwrap_or_default();
                Ok(json.to_string())
            }
            "create" => {
                let contact = crate::providers::Contact {
                    id: String::new(),
                    name: args
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    phone: args
                        .get("phone")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    email: args
                        .get("email")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                };
                let id = self.ctx.provider.create_contact(&contact).await?;
                Ok(format!("Contact created: {id}"))
            }
            "delete" => {
                let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
                self.ctx.provider.delete_contact(id).await?;
                Ok(format!("Contact deleted: {id}"))
            }
            _ => Ok("Unknown action".to_string()),
        }
    }
}

device_tool!(
    LocationTool,
    "location",
    "Get the current device location",
    r#"{"type":"object","properties":{}}"#,
    Location
);

impl LocationTool {
    async fn execute(&self, _arguments: &str) -> Result<String> {
        let loc = self.ctx.provider.get_location().await?;
        Ok(serde_json::json!({
            "latitude": loc.latitude,
            "longitude": loc.longitude,
            "accuracy": loc.accuracy,
        })
        .to_string())
    }
}

device_tool!(
    NotificationTool,
    "notification",
    "Post or list notifications",
    r#"{"type":"object","properties":{"action":{"type":"string","enum":["post","list"]},"title":{"type":"string"},"text":{"type":"string"}},"required":["action"]}"#,
    Notifications
);

impl NotificationTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value =
            serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");
        match action {
            "post" => {
                let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("NOVA");
                let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
                self.ctx
                    .provider
                    .post_notification(title, text, "com.example.nova")
                    .await?;
                Ok(format!("Notification posted: {title}"))
            }
            "list" => {
                let notifications = self.ctx.provider.get_notifications().await?;
                let json: serde_json::Value =
                    serde_json::to_value(&notifications).unwrap_or(serde_json::json!([]));
                Ok(json.to_string())
            }
            _ => Ok("Unknown action".to_string()),
        }
    }
}

device_tool!(
    FileTool,
    "file",
    "Read, write, list, or delete files",
    r#"{"type":"object","properties":{"action":{"type":"string","enum":["read","write","list","delete"]},"path":{"type":"string"},"content":{"type":"string"}},"required":["action","path"]}"#,
    FileRead
);

impl FileTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value =
            serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("read");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        match action {
            "read" => {
                let data: Vec<u8> = self.ctx.provider.read_file(path).await?;
                let text = String::from_utf8_lossy(&data);
                Ok(format!("File content ({} bytes):\n{text}", data.len()))
            }
            "write" => {
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                self.ctx
                    .provider
                    .write_file(path, content.as_bytes())
                    .await?;
                Ok(format!("Written {} bytes to {path}", content.len()))
            }
            "list" => {
                let files = self.ctx.provider.list_files(path).await?;
                Ok(files.join("\n"))
            }
            "delete" => {
                self.ctx.provider.delete_file(path).await?;
                Ok(format!("Deleted {path}"))
            }
            _ => Ok("Unknown action".to_string()),
        }
    }
}

device_tool!(
    SmsTool,
    "sms",
    "Send an SMS message (requires biometric)",
    r#"{"type":"object","properties":{"recipient":{"type":"string"},"message":{"type":"string"}},"required":["recipient","message"]}"#,
    SmsSend
);

impl SmsTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value =
            serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));
        let recipient = args.get("recipient").and_then(|v| v.as_str()).unwrap_or("");
        let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let bio = self
            .ctx
            .provider
            .authenticate_biometric("Confirm sending SMS")
            .await?;
        if !bio.success {
            return Err(NovaError::new(
                ErrorCategory::ConsentRequired,
                "ERR_BIOMETRIC_FAILED",
                "Biometric authentication required for SMS",
            ));
        }
        self.ctx.provider.send_sms(recipient, message).await?;
        Ok(format!("SMS sent to {recipient}"))
    }
}

device_tool!(
    PhoneTool,
    "phone",
    "Dial a phone number (requires biometric)",
    r#"{"type":"object","properties":{"number":{"type":"string"},"required":["number"]}}"#,
    PhoneCall
);

impl PhoneTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value =
            serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));
        let number = args.get("number").and_then(|v| v.as_str()).unwrap_or("");
        let bio = self
            .ctx
            .provider
            .authenticate_biometric("Confirm phone call")
            .await?;
        if !bio.success {
            return Err(NovaError::new(
                ErrorCategory::ConsentRequired,
                "ERR_BIOMETRIC_FAILED",
                "Biometric authentication required for phone calls",
            ));
        }
        self.ctx.provider.dial_phone(number).await?;
        Ok(format!("Dialing {number}"))
    }
}

device_tool!(
    BatteryTool,
    "battery",
    "Get current battery status",
    r#"{"type":"object","properties":{}}"#,
    Battery
);

impl BatteryTool {
    async fn execute(&self, _arguments: &str) -> Result<String> {
        let status = self.ctx.provider.get_battery_status().await?;
        Ok(serde_json::json!({
            "level": status.level,
            "is_charging": status.is_charging,
            "temperature_celsius": status.temperature,
        })
        .to_string())
    }
}

device_tool!(
    StorageTool,
    "storage",
    "Get device storage information",
    r#"{"type":"object","properties":{}}"#,
    Storage
);

impl StorageTool {
    async fn execute(&self, _arguments: &str) -> Result<String> {
        let info = self.ctx.provider.get_storage_info().await?;
        Ok(serde_json::json!({
            "total_bytes": info.total_bytes,
            "free_bytes": info.free_bytes,
            "used_bytes": info.used_bytes,
        })
        .to_string())
    }
}

device_tool!(
    SensorTool,
    "sensor",
    "Read current sensor data",
    r#"{"type":"object","properties":{"sensor_type":{"type":"string"}}}"#,
    Sensors
);

impl SensorTool {
    async fn execute(&self, _arguments: &str) -> Result<String> {
        let readings = self.ctx.provider.get_sensor_readings().await?;
        let json: serde_json::Value =
            serde_json::to_value(&readings).unwrap_or(serde_json::json!([]));
        Ok(json.to_string())
    }
}

device_tool!(
    BiometricTool,
    "biometric",
    "Authenticate using device biometrics",
    r#"{"type":"object","properties":{"reason":{"type":"string"},"required":["reason"]}}"#,
    Biometric
);

impl BiometricTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value =
            serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));
        let reason = args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Authentication");
        let result = self.ctx.provider.authenticate_biometric(reason).await?;
        if result.success {
            Ok(serde_json::json!({"status":"authenticated","method":result.method}).to_string())
        } else {
            Ok(serde_json::json!({"status":"failed","error":result.error}).to_string())
        }
    }
}

device_tool!(
    AppTool,
    "installed_apps",
    "List installed applications",
    r#"{"type":"object","properties":{}}"#,
    InstalledApps
);

impl AppTool {
    async fn execute(&self, _arguments: &str) -> Result<String> {
        let apps = self.ctx.provider.list_installed_apps().await?;
        let json: serde_json::Value = serde_json::to_value(&apps).unwrap_or(serde_json::json!([]));
        Ok(json.to_string())
    }
}

device_tool!(
    MediaPickerTool,
    "media_picker",
    "Pick media files from device",
    r#"{"type":"object","properties":{"mime_types":{"type":"array","items":{"type":"string"}}}}"#,
    MediaPicker
);

impl MediaPickerTool {
    async fn execute(&self, arguments: &str) -> Result<String> {
        let args: serde_json::Value =
            serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));
        let mime_types: Vec<String> = args
            .get("mime_types")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let media = self.ctx.provider.pick_media(&mime_types).await?;
        let json: serde_json::Value = serde_json::to_value(&media).unwrap_or(serde_json::json!([]));
        Ok(json.to_string())
    }
}

pub struct DeviceToolkit {
    pub tools: Vec<Arc<dyn DeviceTool>>,
}

impl DeviceToolkit {
    pub fn new(
        provider: Arc<dyn DeviceProvider>,
        permissions: Arc<PermissionManager>,
        audit: Arc<RwLock<Vec<DeviceEvent>>>,
    ) -> Self {
        let tools: Vec<Arc<dyn DeviceTool>> = vec![
            Arc::new(CameraTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(GalleryTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(ClipboardReadTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(ClipboardWriteTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(CalendarTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(ContactsTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(LocationTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(NotificationTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(FileTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(SmsTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(PhoneTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(BatteryTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(StorageTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(SensorTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(BiometricTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(AppTool::new(
                provider.clone(),
                permissions.clone(),
                audit.clone(),
            )),
            Arc::new(MediaPickerTool::new(provider, permissions, audit)),
        ];
        Self { tools }
    }

    pub fn count(&self) -> usize {
        self.tools.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mock::MockDeviceProvider;

    #[allow(clippy::type_complexity)]
    fn setup() -> (
        Arc<PermissionManager>,
        Arc<RwLock<Vec<DeviceEvent>>>,
        Arc<dyn DeviceProvider>,
    ) {
        (
            Arc::new(PermissionManager::new()),
            Arc::new(RwLock::new(vec![])),
            Arc::new(MockDeviceProvider::new()),
        )
    }

    #[tokio::test]
    async fn test_camera_permission_denied() {
        let (p, a, prov) = setup();
        let tool = CameraTool::new(prov, p, a);
        assert!(tool.invoke("{}").await.is_err());
    }

    #[tokio::test]
    async fn test_camera_success() {
        let (p, a, prov) = setup();
        p.grant(&DeviceCapability::Camera);
        let tool = CameraTool::new(prov, p, a);
        let r = tool.invoke("{}").await.unwrap();
        assert!(r.contains("path"));
    }

    #[tokio::test]
    async fn test_clipboard_write_read() {
        let (p, a, prov) = setup();
        p.grant(&DeviceCapability::ClipboardWrite);
        p.grant(&DeviceCapability::ClipboardRead);
        let w = ClipboardWriteTool::new(prov.clone(), p.clone(), a.clone());
        let r = ClipboardReadTool::new(prov, p, a);
        w.invoke(r#"{"content":"hello"}"#).await.unwrap();
        let out = r.invoke("{}").await.unwrap();
        assert!(out.contains("hello"));
    }

    #[tokio::test]
    async fn test_battery_tool() {
        let (p, a, prov) = setup();
        p.grant(&DeviceCapability::Battery);
        let tool = BatteryTool::new(prov, p, a);
        let r = tool.invoke("{}").await.unwrap();
        assert!(r.contains("level"));
    }

    #[tokio::test]
    async fn test_toolkit_count() {
        let (p, a, prov) = setup();
        let tk = DeviceToolkit::new(prov, p, a);
        assert_eq!(tk.count(), 17);
    }
}
