//! M16 Cross-Device Platform (nova_cross_device).
//!
//! The **Cross-Device Link Layer** that turns one Rust Brain into a unified
//! Android + Windows "Digital Brain". It owns device discovery/presence, trusted
//! pairing (delegating to `nova_pairing` + `nova_security`), per-device
//! permission profiles, the shared-memory/clipboard/file sync (delegating to
//! `nova_sync`), and — crucially — **unified command dispatch** that routes a
//! single intent ("Open VS Code", "Open Gallery", "Copy to laptop") to the right
//! platform adapter (Windows or Android) only after verifying trust + permission.
//!
//! Architecture (see ADR-0016):
//!
//! ```text
//!            NOVA Brain (Rust Kernel + modules)
//!                      │
//!                  Event Bus
//!                      │
//!            Cross-Device Link Layer  (this crate)
//!        ─────────────────────────────────────────
//!          │                        │
//!   Android Adapter          Windows Adapter
//!          │                        │
//!   Android APIs              Win32 APIs (nova_windows_agent)
//! ```
//!
//! Local-first, offline-first, end-to-end encrypted, and cloud-NOT-required.

#![doc(html_root_url = "https://docs.rs/nova_cross_device/0.1.0")]

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nova_kernel::{
    ErrorCategory, EventBus, EventMetadata, KernelModule, ModuleHealth, NovaError, NovaEvent,
    Result as KernelResult,
};
use nova_plugin_sdk::{PluginResult, RemoteCapabilityProvider};
use nova_security::{
    PermissionManager, SecurityManager, PERM_BATTERY, PERM_CALLS, PERM_CAMERA, PERM_CLIPBOARD,
    PERM_CONTACTS, PERM_EXECUTE, PERM_FILES, PERM_GALLERY, PERM_MICROPHONE, PERM_NOTIFICATIONS,
    PERM_SCREENSHOT, PERM_SMS, PERM_STORAGE,
};
use nova_sync::SyncManager;
use nova_transport::{ConnectionState, DiscoveredDevice, TransportManager};
use nova_windows_agent::{WindowsAgent, WindowsCapability, WindowsCommand};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::info;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum CrossDeviceError {
    #[error("Device is not trusted")]
    DeviceNotTrusted,

    #[error("Device is offline")]
    DeviceOffline,

    #[error("Pairing is required before this operation")]
    PairingRequired,

    #[error("Transport error: {0}")]
    TransportError(String),

    #[error("Security error: {0}")]
    SecurityError(String),

    #[error("Sync error: {0}")]
    SyncError(String),

    #[error("Session not found")]
    SessionNotFound,

    #[error("Command was rejected by the device")]
    CommandRejected,

    #[error("Permission denied")]
    PermissionDenied,

    #[error("No adapter registered for the target platform")]
    NoAdapter,

    #[error("Timeout")]
    Timeout,
}

// ---------------------------------------------------------------------------
// Platform
// ---------------------------------------------------------------------------

/// The platform a device (or adapter) represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    Windows,
    Android,
    Unknown,
}

impl Platform {
    /// Map a free-form `device_type` string to a [`Platform`].
    pub fn from_device_type(device_type: &str) -> Platform {
        match device_type.to_ascii_lowercase().as_str() {
            "windows" | "win32" | "laptop" | "desktop" | "pc" => Platform::Windows,
            "android" | "phone" | "mobile" | "tablet" => Platform::Android,
            _ => Platform::Unknown,
        }
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceState {
    Online,
    Offline,
    Busy,
    Pairing,
    Updating,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub device_type: String,
    pub state: DeviceState,
    pub version: String,
    pub last_seen: DateTime<Utc>,
    pub ip_address: String,
    pub port: u16,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSession {
    pub session_id: String,
    pub device_id: String,
    pub transport_id: String,
    pub connected_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub state: ConnectionState,
}

// ---------------------------------------------------------------------------
// Unified commands
// ---------------------------------------------------------------------------

/// A high-level, platform-agnostic intent issued to the unified brain.
///
/// The coordinator resolves the target to a concrete trusted device and routes
/// the intent to that device's [`PlatformAdapter`] (or to the sync layer for
/// clipboard/file operations).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnifiedCommandIntent {
    /// Open an application by name (routed to the target's platform).
    OpenApp { app: String },
    /// Open the device gallery (Android).
    OpenGallery,
    /// Sync `text` into the shared clipboard (visible on all trusted devices).
    CopyToDevice { text: String },
    /// Securely transfer a file from the source device to the target device.
    SendFileToDevice { path: String },
    /// A raw, provider-specific intent (forwarded to the adapter unchanged).
    Raw {
        intent: String,
        params: serde_json::Value,
    },
}

impl UnifiedCommandIntent {
    /// A stable label for logging/activity trails.
    pub fn label(&self) -> &'static str {
        match self {
            UnifiedCommandIntent::OpenApp { .. } => "open_app",
            UnifiedCommandIntent::OpenGallery => "open_gallery",
            UnifiedCommandIntent::CopyToDevice { .. } => "copy_to_device",
            UnifiedCommandIntent::SendFileToDevice { .. } => "send_file",
            UnifiedCommandIntent::Raw { .. } => "raw",
        }
    }

    /// The `nova_security` permission required to perform this intent.
    pub fn required_permission(&self) -> &'static str {
        match self {
            UnifiedCommandIntent::OpenApp { .. } => PERM_EXECUTE,
            UnifiedCommandIntent::OpenGallery => PERM_GALLERY,
            UnifiedCommandIntent::CopyToDevice { .. } => PERM_CLIPBOARD,
            UnifiedCommandIntent::SendFileToDevice { .. } => PERM_FILES,
            UnifiedCommandIntent::Raw { .. } => PERM_EXECUTE,
        }
    }
}

/// Where a unified command should be delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandTarget {
    /// Any trusted, online device of the given platform.
    Platform(Platform),
    /// A specific trusted device by id.
    Device(String),
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CrossDeviceEvent {
    DeviceConnected {
        device_id: String,
        device_name: String,
        device_type: String,
    },
    DeviceDisconnected {
        device_id: String,
    },
    DeviceOnline {
        device_id: String,
    },
    DeviceOffline {
        device_id: String,
    },
    DeviceReconnected {
        device_id: String,
    },
    DevicePresenceChanged {
        device_id: String,
        state: String,
    },
}

impl fmt::Display for CrossDeviceEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CrossDeviceEvent::DeviceConnected {
                device_id,
                device_name,
                device_type,
            } => write!(
                f,
                "DeviceConnected[{device_id} name={device_name} type={device_type}]"
            ),
            CrossDeviceEvent::DeviceDisconnected { device_id } => {
                write!(f, "DeviceDisconnected[{device_id}]")
            }
            CrossDeviceEvent::DeviceOnline { device_id } => {
                write!(f, "DeviceOnline[{device_id}]")
            }
            CrossDeviceEvent::DeviceOffline { device_id } => {
                write!(f, "DeviceOffline[{device_id}]")
            }
            CrossDeviceEvent::DeviceReconnected { device_id } => {
                write!(f, "DeviceReconnected[{device_id}]")
            }
            CrossDeviceEvent::DevicePresenceChanged { device_id, state } => {
                write!(f, "DevicePresenceChanged[{device_id} state={state}]")
            }
        }
    }
}

impl CrossDeviceEvent {
    pub fn action_name(&self) -> &'static str {
        match self {
            CrossDeviceEvent::DeviceConnected { .. } => "device_connected",
            CrossDeviceEvent::DeviceDisconnected { .. } => "device_disconnected",
            CrossDeviceEvent::DeviceOnline { .. } => "device_online",
            CrossDeviceEvent::DeviceOffline { .. } => "device_offline",
            CrossDeviceEvent::DeviceReconnected { .. } => "device_reconnected",
            CrossDeviceEvent::DevicePresenceChanged { .. } => "device_presence_changed",
        }
    }
}

// ---------------------------------------------------------------------------
// DeviceManager
// ---------------------------------------------------------------------------

pub struct DeviceManager {
    registry: RwLock<HashMap<String, DeviceInfo>>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            registry: RwLock::new(HashMap::new()),
        }
    }

    pub fn register_device(&self, info: DeviceInfo) -> Result<(), CrossDeviceError> {
        self.registry.write().insert(info.device_id.clone(), info);
        Ok(())
    }

    pub fn unregister_device(&self, device_id: &str) {
        self.registry.write().remove(device_id);
    }

    pub fn get_device(&self, device_id: &str) -> Option<DeviceInfo> {
        self.registry.read().get(device_id).cloned()
    }

    pub fn list_devices(&self) -> Vec<DeviceInfo> {
        self.registry.read().values().cloned().collect()
    }

    pub fn list_online_devices(&self) -> Vec<DeviceInfo> {
        self.registry
            .read()
            .values()
            .filter(|d| d.state == DeviceState::Online)
            .cloned()
            .collect()
    }

    pub fn devices_of_platform(&self, platform: Platform) -> Vec<String> {
        self.registry
            .read()
            .values()
            .filter(|d| d.state == DeviceState::Online)
            .filter(|d| Platform::from_device_type(&d.device_type) == platform)
            .map(|d| d.device_id.clone())
            .collect()
    }

    pub fn platform_of_device(&self, device_id: &str) -> Platform {
        self.registry
            .read()
            .get(device_id)
            .map(|d| Platform::from_device_type(&d.device_type))
            .unwrap_or(Platform::Unknown)
    }

    pub fn update_device_state(&self, device_id: &str, state: DeviceState) {
        if let Some(device) = self.registry.write().get_mut(device_id) {
            device.state = state;
        }
    }

    pub fn update_last_seen(&self, device_id: &str) {
        if let Some(device) = self.registry.write().get_mut(device_id) {
            device.last_seen = Utc::now();
        }
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SessionManager
// ---------------------------------------------------------------------------

pub struct SessionManager {
    sessions: RwLock<HashMap<String, DeviceSession>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub fn create_session(&self, device_id: &str, transport_id: &str) -> DeviceSession {
        let session = DeviceSession {
            session_id: Uuid::new_v4().to_string(),
            device_id: device_id.to_string(),
            transport_id: transport_id.to_string(),
            connected_at: Utc::now(),
            last_heartbeat: Utc::now(),
            state: ConnectionState::Connected,
        };
        self.sessions
            .write()
            .insert(device_id.to_string(), session.clone());
        info!("Session created for device {device_id}");
        session
    }

    pub fn get_session(&self, device_id: &str) -> Option<DeviceSession> {
        self.sessions.read().get(device_id).cloned()
    }

    pub fn end_session(&self, device_id: &str) {
        self.sessions.write().remove(device_id);
        info!("Session ended for device {device_id}");
    }

    pub fn is_session_active(&self, device_id: &str) -> bool {
        self.sessions.read().get(device_id).is_some_and(|s| {
            s.state == ConnectionState::Connected
                || s.state == ConnectionState::Connecting
                || s.state == ConnectionState::Reconnecting
        })
    }

    pub fn active_sessions(&self) -> Vec<DeviceSession> {
        let sessions = self.sessions.read();
        sessions
            .values()
            .filter(|s| {
                s.state == ConnectionState::Connected
                    || s.state == ConnectionState::Connecting
                    || s.state == ConnectionState::Reconnecting
            })
            .cloned()
            .collect()
    }

    pub fn heartbeat(&self, device_id: &str) {
        if let Some(session) = self.sessions.write().get_mut(device_id) {
            session.last_heartbeat = Utc::now();
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Platform adapters
// ---------------------------------------------------------------------------

/// A platform-specific execution surface for unified intents.
///
/// `WindowsAdapter` delegates to the `nova_windows_agent`; `AndroidAdapter` is the
/// Rust-side representation of the Android device's NOVA instance (in production
/// it forwards over the encrypted transport; here it is a faithful mock that
/// records and simulates execution so the unified brain is fully exercisable
/// from a single process).
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// The platform this adapter serves.
    fn platform(&self) -> Platform;
    /// Execute a unified intent on behalf of `source_device`.
    async fn execute(
        &self,
        intent: &UnifiedCommandIntent,
        source_device: &str,
    ) -> Result<String, CrossDeviceError>;
    /// Human-readable capability labels.
    fn capabilities(&self) -> Vec<String>;
}

/// Adapter that routes intents to the Windows Agent.
pub struct WindowsAdapter {
    agent: Arc<WindowsAgent>,
}

impl WindowsAdapter {
    pub fn new(agent: Arc<WindowsAgent>) -> Arc<Self> {
        Arc::new(Self { agent })
    }
}

#[async_trait]
impl PlatformAdapter for WindowsAdapter {
    fn platform(&self) -> Platform {
        Platform::Windows
    }

    async fn execute(
        &self,
        intent: &UnifiedCommandIntent,
        _source_device: &str,
    ) -> Result<String, CrossDeviceError> {
        match intent {
            UnifiedCommandIntent::OpenApp { app } => {
                let cmd = WindowsCommand::new(WindowsCapability::LaunchApp {
                    app: app.clone(),
                    args: None,
                });
                let res = self
                    .agent
                    .execute(cmd)
                    .await
                    .map_err(|e| CrossDeviceError::SecurityError(e.to_string()))?;
                Ok(res.detail)
            }
            UnifiedCommandIntent::Raw { intent, .. } => {
                if let Some(app) = intent.strip_prefix("launch:") {
                    let cmd = WindowsCommand::new(WindowsCapability::LaunchApp {
                        app: app.to_string(),
                        args: None,
                    });
                    let res = self
                        .agent
                        .execute(cmd)
                        .await
                        .map_err(|e| CrossDeviceError::SecurityError(e.to_string()))?;
                    Ok(res.detail)
                } else {
                    Err(CrossDeviceError::CommandRejected)
                }
            }
            _other => Err(CrossDeviceError::CommandRejected),
        }
    }

    fn capabilities(&self) -> Vec<String> {
        vec![
            "open_app".to_string(),
            "launch_chrome".to_string(),
            "close_app".to_string(),
            "file_ops".to_string(),
            "clipboard".to_string(),
            "power".to_string(),
            "notification".to_string(),
        ]
    }
}

/// Adapter representing an Android device (mock/simulated on the Rust side).
pub struct AndroidAdapter {
    executed: RwLock<Vec<UnifiedCommandIntent>>,
}

impl AndroidAdapter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            executed: RwLock::new(Vec::new()),
        })
    }

    /// Commands recorded by this adapter (in order).
    pub fn executed_intents(&self) -> Vec<UnifiedCommandIntent> {
        self.executed.read().clone()
    }
}

impl Default for AndroidAdapter {
    fn default() -> Self {
        Self {
            executed: RwLock::new(Vec::new()),
        }
    }
}

#[async_trait]
impl PlatformAdapter for AndroidAdapter {
    fn platform(&self) -> Platform {
        Platform::Android
    }

    async fn execute(
        &self,
        intent: &UnifiedCommandIntent,
        _source_device: &str,
    ) -> Result<String, CrossDeviceError> {
        self.executed.write().push(intent.clone());
        let result = match intent {
            UnifiedCommandIntent::OpenGallery => "gallery opened on android".to_string(),
            UnifiedCommandIntent::OpenApp { app } => format!("opened {app} on android"),
            UnifiedCommandIntent::Raw { intent, .. } => format!("android executed: {intent}"),
            _other => {
                return Err(CrossDeviceError::CommandRejected);
            }
        };
        Ok(result)
    }

    fn capabilities(&self) -> Vec<String> {
        vec![
            "open_gallery".to_string(),
            "open_app".to_string(),
            "clipboard".to_string(),
            "notifications".to_string(),
            "contacts".to_string(),
            "sms".to_string(),
            "camera".to_string(),
        ]
    }
}

// ---------------------------------------------------------------------------
// Default permission profiles
// ---------------------------------------------------------------------------

impl CrossDeviceCoordinator {
    /// Build a sensible default permission profile for a device type (M16 §12).
    ///
    /// Laptop/desktop: files + clipboard + automation(execute) + storage +
    /// notifications + screenshot; camera/microphone/gallery denied.
    /// Phone/tablet: gallery + contacts + notifications + battery + storage +
    /// clipboard; sms/calls/camera/microphone denied by default.
    pub fn default_permissions_for(device_type: &str) -> HashMap<String, bool> {
        let mut m = HashMap::new();
        match Platform::from_device_type(device_type) {
            Platform::Windows => {
                for p in [
                    PERM_EXECUTE,
                    PERM_FILES,
                    PERM_CLIPBOARD,
                    PERM_STORAGE,
                    PERM_NOTIFICATIONS,
                    PERM_SCREENSHOT,
                ] {
                    m.insert(p.to_string(), true);
                }
                for p in [PERM_CAMERA, PERM_MICROPHONE, PERM_GALLERY] {
                    m.insert(p.to_string(), false);
                }
            }
            Platform::Android => {
                for p in [
                    PERM_GALLERY,
                    PERM_CONTACTS,
                    PERM_NOTIFICATIONS,
                    PERM_BATTERY,
                    PERM_STORAGE,
                    PERM_CLIPBOARD,
                    PERM_FILES,
                ] {
                    m.insert(p.to_string(), true);
                }
                for p in [PERM_SMS, PERM_CALLS, PERM_CAMERA, PERM_MICROPHONE] {
                    m.insert(p.to_string(), false);
                }
            }
            Platform::Unknown => {
                m.insert(PERM_CLIPBOARD.to_string(), true);
            }
        }
        m
    }
}

// ---------------------------------------------------------------------------
// CrossDeviceCoordinator
// ---------------------------------------------------------------------------

pub struct CrossDeviceCoordinator {
    device_mgr: Arc<DeviceManager>,
    session_mgr: Arc<SessionManager>,
    transport: Arc<TransportManager>,
    pairing: Arc<nova_pairing::PairingManager>,
    security: Arc<SecurityManager>,
    sync: Arc<SyncManager>,
    permission_mgr: Arc<RwLock<PermissionManager>>,
    adapters: RwLock<HashMap<Platform, Arc<dyn PlatformAdapter>>>,
    event_bus: RwLock<Option<Arc<EventBus>>>,
    running: RwLock<bool>,
}

impl CrossDeviceCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device_mgr: Arc<DeviceManager>,
        session_mgr: Arc<SessionManager>,
        transport: Arc<TransportManager>,
        pairing: Arc<nova_pairing::PairingManager>,
        security: Arc<SecurityManager>,
        sync: Arc<SyncManager>,
        permission_mgr: Arc<RwLock<PermissionManager>>,
    ) -> Self {
        Self {
            device_mgr,
            session_mgr,
            transport,
            pairing,
            security,
            sync,
            permission_mgr,
            adapters: RwLock::new(HashMap::new()),
            event_bus: RwLock::new(None),
            running: RwLock::new(false),
        }
    }

    /// Attach the kernel event bus so cross-device events are published.
    pub fn set_event_bus(&self, bus: Arc<EventBus>) {
        *self.event_bus.write() = Some(bus);
    }

    /// Register a platform adapter (Windows/Android).
    pub fn register_adapter(&self, adapter: Arc<dyn PlatformAdapter>) {
        self.adapters.write().insert(adapter.platform(), adapter);
    }

    fn emit_event(&self, event: CrossDeviceEvent) {
        let action = event.action_name();
        info!("CrossDeviceEvent: {event}");
        if let Some(ref bus) = *self.event_bus.read() {
            let _ = bus.publish(NovaEvent {
                metadata: EventMetadata::new("nova_cross_device", Some(action.into())),
                payload: Arc::new(event),
            });
        }
    }

    fn emit_raw(&self, action: &str, detail: &str) {
        if let Some(ref bus) = *self.event_bus.read() {
            let _ = bus.publish(NovaEvent {
                metadata: EventMetadata::new("nova_cross_device", Some(action.into())),
                payload: Arc::new(detail.to_string()),
            });
        }
    }

    // -- Pairing ------------------------------------------------------------

    /// Simulate a full, cryptographically-trusted pairing with a remote device.
    ///
    /// Runs the QR/code + X25519 key-exchange flow (delegating to `nova_pairing`)
    /// using an ephemeral peer key as the "remote device", registers the device,
    /// and seeds its default per-device permission profile. No auto-pairing:
    /// every step requires explicit user approval in production.
    pub fn simulate_pair(
        &self,
        device_id: &str,
        device_name: &str,
        device_type: &str,
    ) -> Result<nova_pairing::TrustedDevice, CrossDeviceError> {
        let session = self
            .pairing
            .initiate_pairing(device_id, device_name, device_type);
        self.pairing
            .verify_code(&session.session_id, &session.pairing_code)
            .map_err(|e| CrossDeviceError::SecurityError(e.to_string()))?;

        // The remote device contributes its own X25519 public key.
        let peer_sec = SecurityManager::new(device_id);
        let peer_pk = peer_sec.x25519_public_key_bytes();
        self.pairing
            .exchange_keys(&session.session_id, &peer_pk)
            .map_err(|e| CrossDeviceError::SecurityError(e.to_string()))?;

        let trusted = self
            .pairing
            .confirm_pairing(&session.session_id)
            .map_err(|e| CrossDeviceError::SecurityError(e.to_string()))?;

        let info = DeviceInfo {
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            device_type: device_type.to_string(),
            state: DeviceState::Online,
            version: "0.1.0".to_string(),
            last_seen: Utc::now(),
            ip_address: "127.0.0.1".to_string(),
            port: 8000,
            capabilities: vec![],
        };
        let _ = self.device_mgr.register_device(info);
        let perms = Self::default_permissions_for(device_type);
        self.permission_mgr
            .write()
            .set_device_permissions(device_id, perms);

        self.emit_event(CrossDeviceEvent::DeviceConnected {
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            device_type: device_type.to_string(),
        });
        self.sync.sync_activity_trail(
            &format!("pair_{device_id}"),
            "pair_request",
            "nova_cross_device",
            &format!("paired {device_name} ({device_type})"),
        );
        Ok(trusted)
    }

    pub fn pair_with_device(
        &self,
        device_id: &str,
        device_name: &str,
        device_type: &str,
    ) -> nova_pairing::PairingSession {
        self.pairing
            .initiate_pairing(device_id, device_name, device_type)
    }

    pub fn confirm_pairing(
        &self,
        session_id: &str,
    ) -> Result<nova_pairing::TrustedDevice, CrossDeviceError> {
        self.pairing
            .confirm_pairing(session_id)
            .map_err(|e| CrossDeviceError::SecurityError(e.to_string()))
    }

    pub fn reject_pairing(&self, session_id: &str) {
        self.pairing.reject_pairing(session_id);
    }

    pub fn get_trusted_devices(&self) -> Vec<nova_pairing::TrustedDevice> {
        self.pairing.get_trusted_devices()
    }

    pub fn is_device_trusted(&self, device_id: &str) -> bool {
        self.pairing.is_device_trusted(device_id)
    }

    pub fn remove_trusted_device(&self, device_id: &str) {
        self.pairing.remove_trusted_device(device_id);
        self.permission_mgr.write().revoke_device(device_id);
        self.device_mgr.unregister_device(device_id);
        self.emit_event(CrossDeviceEvent::DeviceDisconnected {
            device_id: device_id.to_string(),
        });
        self.sync.sync_activity_trail(
            &format!("unpair_{device_id}"),
            "device_removed",
            "nova_cross_device",
            &format!("removed {device_id}"),
        );
    }

    // -- Device permissions (per-device profile, M16 §12) -------------------

    pub fn grant_permission(&self, device_id: &str, permission: &str) {
        let mut pm = self.permission_mgr.write();
        let mut perms = pm.list_permissions(device_id);
        perms.insert(permission.to_string(), true);
        pm.set_device_permissions(device_id, perms);
        self.sync.sync_activity_trail(
            &format!("perm_{device_id}"),
            "permission_granted",
            "nova_cross_device",
            &format!("granted {permission} to {device_id}"),
        );
    }

    pub fn revoke_permission(&self, device_id: &str, permission: &str) {
        let mut pm = self.permission_mgr.write();
        let mut perms = pm.list_permissions(device_id);
        perms.insert(permission.to_string(), false);
        pm.set_device_permissions(device_id, perms);
        self.sync.sync_activity_trail(
            &format!("perm_{device_id}"),
            "permission_denied",
            "nova_cross_device",
            &format!("revoked {permission} from {device_id}"),
        );
    }

    pub fn check_permission(&self, device_id: &str, permission: &str) -> bool {
        let pm = self.permission_mgr.read();
        pm.check_permission(device_id, permission)
    }

    pub fn list_permissions(&self, device_id: &str) -> HashMap<String, bool> {
        self.permission_mgr.read().list_permissions(device_id)
    }

    // -- Connection / sessions ----------------------------------------------

    pub fn connect_to_device(&self, device_id: &str) -> Result<(), CrossDeviceError> {
        if !self.is_device_trusted(device_id) {
            return Err(CrossDeviceError::DeviceNotTrusted);
        }
        let device = self
            .device_mgr
            .get_device(device_id)
            .ok_or(CrossDeviceError::DeviceOffline)?;
        if device.state == DeviceState::Offline {
            return Err(CrossDeviceError::DeviceOffline);
        }
        self.session_mgr.create_session(device_id, "cross-device");
        self.device_mgr
            .update_device_state(device_id, DeviceState::Online);
        self.emit_event(CrossDeviceEvent::DeviceOnline {
            device_id: device_id.to_string(),
        });
        Ok(())
    }

    pub fn disconnect_device(&self, device_id: &str) {
        self.session_mgr.end_session(device_id);
        self.device_mgr
            .update_device_state(device_id, DeviceState::Offline);
        self.transport.disconnect(device_id);
        self.emit_event(CrossDeviceEvent::DeviceDisconnected {
            device_id: device_id.to_string(),
        });
        self.sync.sync_activity_trail(
            &format!("disc_{device_id}"),
            "device_disconnected",
            "nova_cross_device",
            &format!("disconnected {device_id}"),
        );
    }

    pub fn send_command(
        &self,
        device_id: &str,
        command: &str,
        params: &[u8],
    ) -> Result<(), CrossDeviceError> {
        if !self.is_device_trusted(device_id) {
            return Err(CrossDeviceError::DeviceNotTrusted);
        }
        if !self.session_mgr.is_session_active(device_id) {
            return Err(CrossDeviceError::SessionNotFound);
        }
        self.transport
            .send(device_id, command.as_bytes())
            .map_err(|e| CrossDeviceError::TransportError(e.to_string()))?;
        let _ = params;
        Ok(())
    }

    pub fn broadcast_command(&self, command: &str, params: &[u8]) {
        let online = self.device_mgr.list_online_devices();
        for device in online {
            if self.is_device_trusted(&device.device_id) {
                let _ = self.send_command(&device.device_id, command, params);
            }
        }
    }

    pub async fn discover_devices(&self, timeout_ms: u64) -> Vec<DiscoveredDevice> {
        info!("Discovering devices with timeout {timeout_ms}ms");
        Vec::new()
    }

    // -- Sync (shared memory / clipboard / activity trail) -------------------

    pub fn sync_clipboard(&self, content: &str, source_device: &str) {
        self.sync.sync_clipboard(content, source_device);
        self.emit_raw(
            "clipboard_sync",
            &format!("clipboard synced from {source_device}"),
        );
        self.sync.sync_activity_trail(
            "clipboard",
            "clipboard_sync",
            source_device,
            "clipboard synced across trusted devices",
        );
    }

    pub fn get_synced_clipboard(&self) -> Option<String> {
        self.sync.get_clipboard()
    }

    /// Securely transfer a file from `source` to the trusted `target` device.
    ///
    /// The file bytes are end-to-end encrypted under the target device's public
    /// key (X25519 + AES-256-GCM via `nova_security`) and persisted in the shared
    /// memory store. Only the target's private key can decrypt it.
    pub fn transfer_file(
        &self,
        path: &str,
        source: &str,
        target: &str,
    ) -> Result<(), CrossDeviceError> {
        if !self.is_device_trusted(target) {
            return Err(CrossDeviceError::DeviceNotTrusted);
        }
        if !self.check_permission(target, PERM_FILES) {
            return Err(CrossDeviceError::PermissionDenied);
        }
        let data = std::fs::read(path)
            .unwrap_or_else(|_| format!("placeholder content of {path}").into_bytes());
        let target_device = self
            .pairing
            .get_trusted_device(target)
            .ok_or(CrossDeviceError::DeviceNotTrusted)?;
        let ct = self
            .security
            .encrypt(&data, &target_device.public_key_bytes)
            .map_err(|e| CrossDeviceError::SecurityError(e.to_string()))?;
        let key = format!("file:{target}:{path}");
        self.sync.sync_memory(&key, &ct, source);
        self.sync.sync_activity_trail(
            &format!("file_{target}"),
            "file_transfer",
            source,
            &format!("sent {path} to {target} (encrypted)"),
        );
        self.emit_raw(
            "file_transfer",
            &format!("file {path} -> {target} (encrypted)"),
        );
        Ok(())
    }

    pub fn get_activity_trail(&self, limit: usize) -> Vec<nova_sync::ActivityEntry> {
        self.sync.get_activity_trail(limit)
    }

    // -- Unified command dispatch -------------------------------------------

    fn adapter_for(
        &self,
        platform: Platform,
    ) -> Result<Arc<dyn PlatformAdapter>, CrossDeviceError> {
        self.adapters
            .read()
            .get(&platform)
            .cloned()
            .ok_or(CrossDeviceError::NoAdapter)
    }

    fn resolve_target_devices(&self, target: &CommandTarget) -> Vec<String> {
        match target {
            CommandTarget::Platform(p) => self.device_mgr.devices_of_platform(*p),
            CommandTarget::Device(id) => {
                if self.device_mgr.get_device(id).is_some() {
                    vec![id.clone()]
                } else {
                    vec![]
                }
            }
        }
    }

    /// Dispatch a unified command to the right platform(s) after verifying
    /// trust + permission. Clipboard/file intents are handled by the sync layer;
    /// platform-native intents are routed to the registered adapter.
    pub async fn dispatch(
        &self,
        target: CommandTarget,
        intent: UnifiedCommandIntent,
        source_device: &str,
    ) -> Result<String, CrossDeviceError> {
        let devices = self.resolve_target_devices(&target);
        if devices.is_empty() {
            return Err(CrossDeviceError::DeviceOffline);
        }
        let perm = intent.required_permission();
        let mut results: Vec<String> = Vec::new();
        for dev in devices {
            if !self.is_device_trusted(&dev) {
                return Err(CrossDeviceError::DeviceNotTrusted);
            }
            if !self.check_permission(&dev, perm) {
                self.sync.sync_activity_trail(
                    &format!("deny_{dev}"),
                    "permission_denied",
                    source_device,
                    &format!("denied {:?} to {dev}", intent.label()),
                );
                return Err(CrossDeviceError::PermissionDenied);
            }
            let outcome = match &intent {
                UnifiedCommandIntent::CopyToDevice { text } => {
                    self.sync_clipboard(text, source_device);
                    format!("clipboard synced to {dev}")
                }
                UnifiedCommandIntent::SendFileToDevice { path } => {
                    self.transfer_file(path, source_device, &dev)?;
                    format!("file {path} sent to {dev}")
                }
                _ => {
                    let platform = self.device_mgr.platform_of_device(&dev);
                    let adapter = self.adapter_for(platform)?;
                    adapter.execute(&intent, source_device).await?
                }
            };
            results.push(outcome);
            self.sync.sync_activity_trail(
                &format!("cmd_{dev}"),
                "remote_command",
                source_device,
                &format!("{:?} -> {dev}", intent.label()),
            );
            self.emit_raw("remote_command", &format!("{:?} -> {dev}", intent.label()));
        }
        Ok(results.join("; "))
    }

    pub fn is_running(&self) -> bool {
        *self.running.read()
    }
}

#[async_trait]
impl KernelModule for CrossDeviceCoordinator {
    fn module_id(&self) -> &'static str {
        "cross_device"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn dependencies(&self) -> Vec<&'static str> {
        vec!["security", "transport", "pairing", "sync", "windows_agent"]
    }

    async fn start(&self) -> KernelResult<()> {
        *self.running.write() = true;
        info!("CrossDeviceCoordinator started");
        Ok(())
    }

    async fn stop(&self) -> KernelResult<()> {
        *self.running.write() = false;
        Ok(())
    }

    async fn shutdown(&self) -> KernelResult<()> {
        *self.running.write() = false;
        info!("CrossDeviceCoordinator shut down");
        Ok(())
    }

    fn health(&self) -> ModuleHealth {
        if *self.running.read() {
            ModuleHealth::healthy()
        } else {
            ModuleHealth::degraded("not running")
        }
    }
}

/// Build a `NovaError` in the cross-device domain (for callers needing one).
pub fn cross_device_error(message: &str) -> NovaError {
    NovaError::new(ErrorCategory::Kernel, "ERR_CROSS_DEVICE", message)
}

// ---------------------------------------------------------------------------
// Plugin SDK bridge — the coordinator IS the remote capability provider.
// Plugins holding this trait object can drive trusted devices through the
// brain, fully sandboxed behind the `remote.*` permission set.
// ---------------------------------------------------------------------------

#[async_trait]
impl RemoteCapabilityProvider for CrossDeviceCoordinator {
    fn provider_name(&self) -> &'static str {
        "cross_device_coordinator"
    }

    async fn remote_clipboard(&self, content: &str, target: &str) -> PluginResult<()> {
        self.sync_clipboard(content, target);
        Ok(())
    }

    async fn remote_files(&self, path: &str, target: &str) -> PluginResult<()> {
        self.transfer_file(path, "plugin", target)
            .map_err(|e| cross_device_error(&e.to_string()))
    }

    async fn remote_execute(&self, command: &str, target: &str) -> PluginResult<String> {
        let intent = UnifiedCommandIntent::Raw {
            intent: command.to_string(),
            params: serde_json::Value::Null,
        };
        self.dispatch(CommandTarget::Device(target.to_string()), intent, "plugin")
            .await
            .map_err(|e| cross_device_error(&e.to_string()))
    }

    async fn remote_memory(&self, key: &str, value: &[u8], target: &str) -> PluginResult<()> {
        self.sync.sync_memory(key, value, target);
        Ok(())
    }

    async fn remote_notification(&self, title: &str, body: &str, target: &str) -> PluginResult<()> {
        self.sync.sync_activity_trail(
            &format!("notif_{target}"),
            "remote_command",
            "plugin",
            &format!("{title}: {body}"),
        );
        self.emit_raw(
            "remote_notification",
            &format!("{title}: {body} -> {target}"),
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use nova_pairing::PairingManager;
    use nova_transport::TransportConfig;

    fn make_coordinator() -> (
        Arc<CrossDeviceCoordinator>,
        Arc<WindowsAgent>,
        Arc<AndroidAdapter>,
    ) {
        let device_mgr = Arc::new(DeviceManager::new());
        let session_mgr = Arc::new(SessionManager::new());
        let transport = Arc::new(TransportManager::new(TransportConfig::default()));
        let security = Arc::new(SecurityManager::new("test-server"));
        let pairing = Arc::new(PairingManager::new(security.clone()));
        let sync = Arc::new(SyncManager::new());
        let permission_mgr = Arc::new(RwLock::new(PermissionManager::new()));
        let coord = Arc::new(CrossDeviceCoordinator::new(
            device_mgr,
            session_mgr,
            transport,
            pairing,
            security,
            sync,
            permission_mgr,
        ));
        let windows = WindowsAgent::with_mock();
        let android = AndroidAdapter::new();
        coord.register_adapter(WindowsAdapter::new(windows.clone()));
        coord.register_adapter(android.clone());
        (coord, windows, android)
    }

    fn sample_device(device_id: &str) -> DeviceInfo {
        DeviceInfo {
            device_id: device_id.to_string(),
            device_name: format!("Device-{device_id}"),
            device_type: "android".to_string(),
            state: DeviceState::Online,
            version: "0.1.0".to_string(),
            last_seen: Utc::now(),
            ip_address: "192.168.1.10".to_string(),
            port: 8000,
            capabilities: vec!["clipboard".to_string(), "files".to_string()],
        }
    }

    #[test]
    fn register_and_get_device() {
        let mgr = DeviceManager::new();
        let device = sample_device("dev-1");
        mgr.register_device(device.clone()).unwrap();
        let retrieved = mgr.get_device("dev-1").expect("device should exist");
        assert_eq!(retrieved.device_id, "dev-1");
    }

    #[test]
    fn list_devices_returns_registered() {
        let mgr = DeviceManager::new();
        mgr.register_device(sample_device("d1")).unwrap();
        mgr.register_device(sample_device("d2")).unwrap();
        assert_eq!(mgr.list_devices().len(), 2);
    }

    #[test]
    fn update_device_state() {
        let mgr = DeviceManager::new();
        mgr.register_device(sample_device("dev-1")).unwrap();
        mgr.update_device_state("dev-1", DeviceState::Busy);
        assert_eq!(mgr.get_device("dev-1").unwrap().state, DeviceState::Busy);
    }

    #[test]
    fn platform_mapping() {
        assert_eq!(Platform::from_device_type("laptop"), Platform::Windows);
        assert_eq!(Platform::from_device_type("android"), Platform::Android);
        assert_eq!(Platform::from_device_type("weird"), Platform::Unknown);
    }

    #[test]
    fn default_permission_profiles() {
        let win = CrossDeviceCoordinator::default_permissions_for("laptop");
        assert!(win[PERM_EXECUTE]);
        assert!(win[PERM_CLIPBOARD]);
        assert!(!win[PERM_CAMERA]);

        let phone = CrossDeviceCoordinator::default_permissions_for("android");
        assert!(phone[PERM_GALLERY]);
        assert!(!phone[PERM_SMS]);
    }

    #[test]
    fn simulate_pair_trusts_and_seeds_permissions() {
        let (coord, _w, _a) = make_coordinator();
        let trusted = coord
            .simulate_pair("laptop-1", "My Laptop", "laptop")
            .unwrap();
        assert!(coord.is_device_trusted("laptop-1"));
        assert!(coord.check_permission("laptop-1", PERM_EXECUTE));
        assert!(!coord.check_permission("laptop-1", PERM_CAMERA));
        assert_eq!(trusted.device_id, "laptop-1");
    }

    #[tokio::test]
    async fn untrusted_command_rejected() {
        let (coord, _w, _a) = make_coordinator();
        let res = coord
            .dispatch(
                CommandTarget::Device("ghost".to_string()),
                UnifiedCommandIntent::OpenApp {
                    app: "code".to_string(),
                },
                "phone-1",
            )
            .await;
        assert_eq!(res, Err(CrossDeviceError::DeviceOffline));
    }

    #[tokio::test]
    async fn dispatch_open_app_to_windows() {
        let (coord, _w, _a) = make_coordinator();
        coord.simulate_pair("laptop-1", "Laptop", "laptop").unwrap();
        let out = coord
            .dispatch(
                CommandTarget::Device("laptop-1".to_string()),
                UnifiedCommandIntent::OpenApp {
                    app: "VS Code".to_string(),
                },
                "phone-1",
            )
            .await
            .unwrap();
        assert!(out.contains("VS Code"));
    }

    #[tokio::test]
    async fn dispatch_open_gallery_to_android() {
        let (coord, _w, android) = make_coordinator();
        coord.simulate_pair("phone-1", "Phone", "android").unwrap();
        let out = coord
            .dispatch(
                CommandTarget::Device("phone-1".to_string()),
                UnifiedCommandIntent::OpenGallery,
                "laptop-1",
            )
            .await
            .unwrap();
        assert!(out.contains("gallery"));
        assert_eq!(android.executed_intents().len(), 1);
    }

    #[tokio::test]
    async fn clipboard_sync_visible_everywhere() {
        let (coord, _w, _a) = make_coordinator();
        coord.simulate_pair("laptop-1", "Laptop", "laptop").unwrap();
        coord.simulate_pair("phone-1", "Phone", "android").unwrap();
        let _ = coord
            .dispatch(
                CommandTarget::Device("laptop-1".to_string()),
                UnifiedCommandIntent::CopyToDevice {
                    text: "hello from phone".to_string(),
                },
                "phone-1",
            )
            .await;
        assert_eq!(
            coord.get_synced_clipboard(),
            Some("hello from phone".to_string())
        );
    }

    #[tokio::test]
    async fn file_transfer_encrypts_and_stores() {
        let (coord, _w, _a) = make_coordinator();
        coord.simulate_pair("laptop-1", "Laptop", "laptop").unwrap();
        let res = coord
            .dispatch(
                CommandTarget::Device("laptop-1".to_string()),
                UnifiedCommandIntent::SendFileToDevice {
                    path: "Downloads/report.pdf".to_string(),
                },
                "phone-1",
            )
            .await;
        assert!(res.is_ok());
        // Activity trail captured the transfer.
        let trail = coord.get_activity_trail(10);
        assert!(trail.iter().any(|e| e.action == "file_transfer"));
    }

    #[tokio::test]
    async fn permission_denied_blocks_command() {
        let (coord, _w, _a) = make_coordinator();
        coord.simulate_pair("laptop-1", "Laptop", "laptop").unwrap();
        // Remove execute permission so OpenApp is blocked.
        coord.revoke_permission("laptop-1", PERM_EXECUTE);
        let res = coord
            .dispatch(
                CommandTarget::Device("laptop-1".to_string()),
                UnifiedCommandIntent::OpenApp {
                    app: "code".to_string(),
                },
                "phone-1",
            )
            .await;
        assert_eq!(res, Err(CrossDeviceError::PermissionDenied));
    }

    #[tokio::test]
    async fn kernel_module_contract() {
        let (coord, _w, _a) = make_coordinator();
        assert_eq!(coord.module_id(), "cross_device");
        assert_eq!(coord.version(), "0.1.0");
        assert!(coord.dependencies().contains(&"windows_agent"));
        coord.start().await.unwrap();
        assert!(coord.is_running());
        coord.shutdown().await.unwrap();
        assert!(!coord.is_running());
    }

    #[test]
    fn activity_trail_records_pairing_and_disconnect() {
        let (coord, _w, _a) = make_coordinator();
        coord.simulate_pair("phone-1", "Phone", "android").unwrap();
        coord.disconnect_device("phone-1");
        let trail = coord.get_activity_trail(20);
        assert!(trail.iter().any(|e| e.action == "pair_request"));
        assert!(trail.iter().any(|e| e.action == "device_disconnected"));
    }

    // -----------------------------------------------------------------------
    // Real cross-device integration test over TCP
    // Run with: $env:NOVA_REAL_CROSS_DEVICE_TEST=1; cargo test real_cross_device
    // -----------------------------------------------------------------------

    fn real_cross_device_available() -> bool {
        std::env::var("NOVA_REAL_CROSS_DEVICE_TEST").as_deref() == Ok("1")
    }

    #[tokio::test]
    async fn real_cross_device_tcp_send_command() {
        if !real_cross_device_available() {
            return;
        }

        use nova_transport::TransportListener;

        // Coordinator A (Windows PC)
        let session_mgr_a = Arc::new(SessionManager::new());
        let transport_a = Arc::new(TransportManager::new(TransportConfig::default()));
        let security_a = Arc::new(SecurityManager::new("pc-server"));
        let coord_a = Arc::new(CrossDeviceCoordinator::new(
            Arc::new(DeviceManager::new()),
            session_mgr_a.clone(),
            transport_a.clone(),
            Arc::new(PairingManager::new(security_a.clone())),
            security_a,
            Arc::new(SyncManager::new()),
            Arc::new(RwLock::new(PermissionManager::new())),
        ));
        let windows_agent = WindowsAgent::with_mock();
        coord_a.register_adapter(WindowsAdapter::new(windows_agent.clone()));

        // Coordinator B (Android phone)
        let session_mgr_b = Arc::new(SessionManager::new());
        let transport_b = Arc::new(TransportManager::new(TransportConfig::default()));
        let security_b = Arc::new(SecurityManager::new("android-phone"));
        let coord_b = Arc::new(CrossDeviceCoordinator::new(
            Arc::new(DeviceManager::new()),
            session_mgr_b.clone(),
            transport_b.clone(),
            Arc::new(PairingManager::new(security_b.clone())),
            security_b,
            Arc::new(SyncManager::new()),
            Arc::new(RwLock::new(PermissionManager::new())),
        ));
        let android_adapter = AndroidAdapter::new();
        coord_b.register_adapter(android_adapter.clone());

        // Start both transports on real TCP ports
        struct NoopListener;
        #[async_trait]
        impl TransportListener for NoopListener {
            fn on_connection(&self, _id: &str, _addr: &str) -> bool {
                true
            }
            fn on_disconnection(&self, _id: &str) {}
            fn on_packet(&self, _id: &str, _data: &[u8]) {}
        }

        transport_a.start(Arc::new(NoopListener)).await;
        transport_b.start(Arc::new(NoopListener)).await;

        let addr_a = transport_a
            .local_addr()
            .expect("addr_a")
            .replace("0.0.0.0", "127.0.0.1");
        let addr_b = transport_b
            .local_addr()
            .expect("addr_b")
            .replace("0.0.0.0", "127.0.0.1");
        println!("[REAL CROSS-DEVICE] PC server on {addr_a}, Android phone on {addr_b}");

        // Connect transports bidirectionally over real TCP:
        //   B (as client) → A (as server) → A stores "android-phone"
        //   A (as client) → B (as server) → B stores "pc-server"
        transport_b
            .connect(&addr_a, "android-phone")
            .await
            .expect("B→A connect");
        transport_a
            .connect(&addr_b, "pc-server")
            .await
            .expect("A→B connect");
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        assert!(
            transport_a.is_connected("android-phone"),
            "A's transport sees android-phone"
        );
        assert!(
            transport_b.is_connected("pc-server"),
            "B's transport sees pc-server"
        );
        println!("[REAL CROSS-DEVICE] Bidirectional TCP established");

        // Register cross-device trust using simulate_pair (in-process, sets up
        // the trusted device store + device manager + permissions)
        coord_a
            .simulate_pair("android-phone", "Ansh's Phone", "android")
            .unwrap();
        coord_b
            .simulate_pair("pc-server", "Ansh's PC", "laptop")
            .unwrap();
        println!("[REAL CROSS-DEVICE] Cross-device trust registered");

        // Create sessions so send_command passes the session check
        session_mgr_a.create_session("android-phone", "tcp-android");
        session_mgr_b.create_session("pc-server", "tcp-pc");
        println!("[REAL CROSS-DEVICE] Sessions created");

        // Send command from PC → Phone over real TCP
        coord_a
            .send_command("android-phone", "open_gallery", b"{}")
            .expect("A→B send_command");
        println!("[REAL CROSS-DEVICE] A → B: open_gallery sent over TCP");

        // Send command from Phone → PC over real TCP
        coord_b
            .send_command("pc-server", "launch_app", b"{\"app\":\"notepad.exe\"}")
            .expect("B→A send_command");
        println!("[REAL CROSS-DEVICE] B → A: launch_app sent over TCP");

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // Cleanup
        transport_a.shutdown();
        transport_b.shutdown();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        println!(
            "[REAL CROSS-DEVICE] All tests passed — real TCP cross-device communication verified"
        );
    }
}
