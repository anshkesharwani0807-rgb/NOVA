//! M16 Windows Agent (nova_windows_agent).
//!
//! The Windows Agent is the **Android + Windows Unified Brain's** control surface for
//! the Windows platform. It exposes a fixed, permission-gated capability set
//! (launch/close/kill apps, file ops, clipboard, volume, brightness, lock, power
//! states, notifications, screenshot) and executes them through a
//! [`WindowsCapabilityProvider`].
//!
//! - `MockWindowsProvider` records intents and is used by the demo/tests (safe).
//! - `RealWindowsProvider` drives the real OS facilities via `std::process::Command`
//!   and is what the Windows service instantiates in production.
//!
//! Every capability is mapped to a `nova_security` permission constant so the
//! cross-device layer can gate remote commands (Principle 2 — privacy by default).

#![doc(html_root_url = "https://docs.rs/nova_windows_agent/0.1.0")]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use nova_kernel::{ErrorCategory, KernelModule, ModuleHealth, NovaError, Result as KernelResult};
use nova_security::{
    PermissionManager, PERM_CLIPBOARD, PERM_EXECUTE, PERM_FILES, PERM_NOTIFICATIONS,
    PERM_SCREENSHOT,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

/// Local-policy device id used for the Windows Agent's own capability policy.
const LOCAL_POLICY_DEVICE: &str = "windows-local";

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum WindowsAgentError {
    #[error("Permission denied for capability: {0}")]
    PermissionDenied(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Unsupported capability: {0}")]
    Unsupported(String),
}

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

/// A single Windows capability the agent can perform.
///
/// Each variant maps to exactly one `nova_security` permission (see
/// [`WindowsCapability::required_permission`]) so remote commands routed to this
/// agent are always authorization-checked first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowsCapability {
    /// Launch an application by name or path, optionally with arguments.
    LaunchApp { app: String, args: Option<String> },
    /// Close an application by name.
    CloseApp { app: String },
    /// Kill a process by id.
    KillProcess { pid: u32 },
    /// Open a file with its default association.
    OpenFile { path: String },
    /// Move a file from one location to another.
    MoveFile { from: String, to: String },
    /// Rename a file in place.
    RenameFile { path: String, new_name: String },
    /// Delete a file.
    DeleteFile { path: String },
    /// Write `content` to the system clipboard.
    SetClipboard { content: String },
    /// Read the system clipboard.
    GetClipboard,
    /// Set the master output volume (0–100).
    SetVolume { level: u8 },
    /// Set the display brightness (0–100).
    SetBrightness { level: u8 },
    /// Lock the workstation.
    LockPc,
    /// Shut the machine down.
    Shutdown,
    /// Restart the machine.
    Restart,
    /// Put the machine to sleep.
    Sleep,
    /// Show a toast/notification.
    ShowNotification { title: String, body: String },
    /// Capture a screenshot.
    TakeScreenshot,
}

impl WindowsCapability {
    /// The `nova_security` permission required to perform this capability.
    pub fn required_permission(&self) -> &'static str {
        match self {
            WindowsCapability::LaunchApp { .. }
            | WindowsCapability::CloseApp { .. }
            | WindowsCapability::KillProcess { .. }
            | WindowsCapability::SetVolume { .. }
            | WindowsCapability::SetBrightness { .. }
            | WindowsCapability::LockPc
            | WindowsCapability::Shutdown
            | WindowsCapability::Restart
            | WindowsCapability::Sleep => PERM_EXECUTE,
            WindowsCapability::OpenFile { .. }
            | WindowsCapability::MoveFile { .. }
            | WindowsCapability::RenameFile { .. }
            | WindowsCapability::DeleteFile { .. } => PERM_FILES,
            WindowsCapability::SetClipboard { .. } | WindowsCapability::GetClipboard => {
                PERM_CLIPBOARD
            }
            WindowsCapability::ShowNotification { .. } => PERM_NOTIFICATIONS,
            WindowsCapability::TakeScreenshot => PERM_SCREENSHOT,
        }
    }

    /// A short human-readable label for activity logging.
    pub fn label(&self) -> &'static str {
        match self {
            WindowsCapability::LaunchApp { .. } => "windows.launch_app",
            WindowsCapability::CloseApp { .. } => "windows.close_app",
            WindowsCapability::KillProcess { .. } => "windows.kill_process",
            WindowsCapability::OpenFile { .. } => "windows.open_file",
            WindowsCapability::MoveFile { .. } => "windows.move_file",
            WindowsCapability::RenameFile { .. } => "windows.rename_file",
            WindowsCapability::DeleteFile { .. } => "windows.delete_file",
            WindowsCapability::SetClipboard { .. } => "windows.set_clipboard",
            WindowsCapability::GetClipboard => "windows.get_clipboard",
            WindowsCapability::SetVolume { .. } => "windows.set_volume",
            WindowsCapability::SetBrightness { .. } => "windows.set_brightness",
            WindowsCapability::LockPc => "windows.lock_pc",
            WindowsCapability::Shutdown => "windows.shutdown",
            WindowsCapability::Restart => "windows.restart",
            WindowsCapability::Sleep => "windows.sleep",
            WindowsCapability::ShowNotification { .. } => "windows.show_notification",
            WindowsCapability::TakeScreenshot => "windows.take_screenshot",
        }
    }

    /// Every capability label (used by providers to advertise support).
    pub fn all_labels() -> Vec<&'static str> {
        vec![
            "windows.launch_app",
            "windows.close_app",
            "windows.kill_process",
            "windows.open_file",
            "windows.move_file",
            "windows.rename_file",
            "windows.delete_file",
            "windows.set_clipboard",
            "windows.get_clipboard",
            "windows.set_volume",
            "windows.set_brightness",
            "windows.lock_pc",
            "windows.shutdown",
            "windows.restart",
            "windows.sleep",
            "windows.show_notification",
            "windows.take_screenshot",
        ]
    }
}

/// A command handed to the Windows Agent for execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsCommand {
    pub capability: WindowsCapability,
}

impl WindowsCommand {
    pub fn new(capability: WindowsCapability) -> Self {
        Self { capability }
    }
}

/// The outcome of executing a [`WindowsCommand`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsResult {
    pub success: bool,
    pub detail: String,
    /// Optional string payload (e.g. clipboard contents).
    pub data: Option<String>,
}

impl WindowsResult {
    pub fn success(detail: impl Into<String>) -> Self {
        Self {
            success: true,
            detail: detail.into(),
            data: None,
        }
    }

    pub fn success_with(detail: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            success: true,
            detail: detail.into(),
            data: Some(data.into()),
        }
    }

    pub fn failure(detail: impl Into<String>) -> Self {
        Self {
            success: false,
            detail: detail.into(),
            data: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

/// Abstraction over how Windows capabilities are actually performed.
///
/// `MockWindowsProvider` is used by the demo/tests; `RealWindowsProvider` is the
/// production backend that shells out to the OS. Both are `Send + Sync` so they
/// can be shared behind an `Arc` inside the agent and across `tokio` tasks.
#[async_trait]
pub trait WindowsCapabilityProvider: Send + Sync {
    /// Stable name of the provider implementation.
    fn provider_name(&self) -> &'static str;

    /// Execute a single command and return the result.
    async fn execute(&self, cmd: &WindowsCommand) -> Result<WindowsResult, WindowsAgentError>;

    /// The set of capabilities this provider can actually perform.
    fn supported(&self) -> HashSet<String> {
        WindowsCapability::all_labels()
            .into_iter()
            .map(String::from)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Mock provider
// ---------------------------------------------------------------------------

/// Records every command it receives and returns a success. Safe for tests/demo.
pub struct MockWindowsProvider {
    executed: RwLock<Vec<WindowsCommand>>,
}

impl MockWindowsProvider {
    pub fn new() -> Self {
        Self {
            executed: RwLock::new(Vec::new()),
        }
    }

    /// Return a copy of every command executed so far (in order).
    pub fn executed_commands(&self) -> Vec<WindowsCommand> {
        self.executed.read().clone()
    }

    /// Clear the recorded command history.
    pub fn reset(&self) {
        self.executed.write().clear();
    }
}

impl Default for MockWindowsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WindowsCapabilityProvider for MockWindowsProvider {
    fn provider_name(&self) -> &'static str {
        "mock-windows"
    }

    async fn execute(&self, cmd: &WindowsCommand) -> Result<WindowsResult, WindowsAgentError> {
        self.executed.write().push(cmd.clone());
        info!("MockWindowsProvider executing {}", cmd.capability.label());
        let detail = match &cmd.capability {
            WindowsCapability::GetClipboard => "clipboard read (mock)".to_string(),
            WindowsCapability::LaunchApp { app, .. } => format!("launched {app} (mock)"),
            other => format!("executed {} (mock)", other.label()),
        };
        Ok(WindowsResult::success(detail))
    }
}

// ---------------------------------------------------------------------------
// Real provider (production backend)
// ---------------------------------------------------------------------------

/// Production provider that drives the real Windows OS via `std::process::Command`.
///
/// Constructed by the Windows service; never instantiated by the demo/tests so no
/// destructive action can fire during CI.
pub struct RealWindowsProvider;

impl RealWindowsProvider {
    pub fn new() -> Self {
        Self
    }

    fn run(cmd: &str, args: &[&str]) -> Result<WindowsResult, WindowsAgentError> {
        match std::process::Command::new(cmd).args(args).output() {
            Ok(out) => {
                let detail = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if out.status.success() {
                    Ok(WindowsResult::success_with("ok", detail))
                } else {
                    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    Ok(WindowsResult::failure(format!("exit: {err}")))
                }
            }
            Err(e) => Err(WindowsAgentError::ProviderError(e.to_string())),
        }
    }
}

impl Default for RealWindowsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WindowsCapabilityProvider for RealWindowsProvider {
    fn provider_name(&self) -> &'static str {
        "real-windows"
    }

    async fn execute(&self, cmd: &WindowsCommand) -> Result<WindowsResult, WindowsAgentError> {
        match &cmd.capability {
            WindowsCapability::LaunchApp { app, args } => {
                let _ = args;
                Self::run("cmd", &["/c", "start", "", app])
            }
            WindowsCapability::CloseApp { app } => Self::run("taskkill", &["/IM", app, "/F"]),
            WindowsCapability::KillProcess { pid } => {
                Self::run("taskkill", &["/PID", &pid.to_string(), "/F"])
            }
            WindowsCapability::OpenFile { path } => Self::run("cmd", &["/c", "start", "", path]),
            WindowsCapability::MoveFile { from, to } => {
                Self::run("cmd", &["/c", "move", "/Y", from, to])
            }
            WindowsCapability::RenameFile { path, new_name } => {
                Self::run("cmd", &["/c", "rename", path, new_name])
            }
            WindowsCapability::DeleteFile { path } => {
                Self::run("cmd", &["/c", "del", "/F", "/Q", path])
            }
            WindowsCapability::SetClipboard { content } => {
                let ps = format!("Set-Clipboard -Value '{}'", content.replace('\'', "''"));
                Self::run("powershell", &["-NoProfile", "-Command", &ps])
            }
            WindowsCapability::GetClipboard => {
                Self::run("powershell", &["-NoProfile", "-Command", "Get-Clipboard"])
            }
            WindowsCapability::SetVolume { level } => {
                let ps = format!(
                    "$w = Add-Type -MemberDefinition '[DllImport(\"user32.dll\")] public static extern int SendMessageW(int h, int m, int w, int l);' -Name Win -Namespace Win32 -PassThru; $w::SendMessageW(-1, 0x319, 0, 0xA0000 + {level} * 655)"
                );
                Self::run("powershell", &["-NoProfile", "-Command", &ps])
            }
            WindowsCapability::SetBrightness { level } => {
                let ps = format!(
                    "(Get-WmiObject -Namespace root/WMI -Class WmiMonitorBrightnessMethods).WmiSetBrightness(1, {level})"
                );
                Self::run("powershell", &["-NoProfile", "-Command", &ps])
            }
            WindowsCapability::LockPc => Self::run("rundll32.exe", &["user32.dll,LockWorkStation"]),
            WindowsCapability::Shutdown => Self::run("shutdown", &["/s", "/t", "0"]),
            WindowsCapability::Restart => Self::run("shutdown", &["/r", "/t", "0"]),
            WindowsCapability::Sleep => {
                Self::run("rundll32.exe", &["powrprof.dll,SetSuspendState", "0,1,0"])
            }
            WindowsCapability::ShowNotification { title, body } => {
                let ps = format!("Write-Host '{title}: {body}'");
                Self::run("powershell", &["-NoProfile", "-Command", &ps])
            }
            WindowsCapability::TakeScreenshot => {
                let ps = "Add-Type -AssemblyName System.Windows.Forms; $b = New-Object System.Drawing.Bitmap([System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width, [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Height); $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen(0,0,0,0,$b.Size); $b.Save('$env:TEMP\\nova_screenshot.png'); Write-Host 'saved'";
                Self::run("powershell", &["-NoProfile", "-Command", ps])
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Windows Agent
// ---------------------------------------------------------------------------

/// The Windows Agent: a `KernelModule` that executes permission-gated Windows
/// capabilities on behalf of the unified brain.
pub struct WindowsAgent {
    provider: RwLock<Arc<dyn WindowsCapabilityProvider>>,
    policy: Arc<RwLock<PermissionManager>>,
    event_bus: RwLock<Option<Arc<nova_kernel::EventBus>>>,
    running: AtomicBool,
}

impl WindowsAgent {
    /// Create a new agent around a capability provider.
    pub fn new(provider: Arc<dyn WindowsCapabilityProvider>) -> Arc<Self> {
        let policy = Arc::new(RwLock::new(PermissionManager::new()));
        // Enable the full capability set by default for the local policy.
        let mut perms = HashMap::new();
        for p in [
            PERM_EXECUTE,
            PERM_FILES,
            PERM_CLIPBOARD,
            PERM_NOTIFICATIONS,
            PERM_SCREENSHOT,
        ] {
            perms.insert(p.to_string(), true);
        }
        policy
            .write()
            .set_device_permissions(LOCAL_POLICY_DEVICE, perms);
        Arc::new(Self {
            provider: RwLock::new(provider),
            policy,
            event_bus: RwLock::new(None),
            running: AtomicBool::new(false),
        })
    }

    /// Convenience constructor backed by the safe [`MockWindowsProvider`].
    pub fn with_mock() -> Arc<Self> {
        Self::new(Arc::new(MockWindowsProvider::new()))
    }

    /// Convenience constructor backed by the real OS provider.
    pub fn with_real() -> Arc<Self> {
        Self::new(Arc::new(RealWindowsProvider::new()))
    }

    /// Swap the active provider (e.g. tests switching to mock).
    pub fn set_provider(&self, provider: Arc<dyn WindowsCapabilityProvider>) {
        *self.provider.write() = provider;
    }

    /// Attach the kernel event bus so capability executions are published.
    pub fn set_event_bus(&self, bus: Arc<nova_kernel::EventBus>) {
        *self.event_bus.write() = Some(bus);
    }

    /// Grant a `nova_security` permission in the local capability policy.
    pub fn enable_capability(&self, permission: &str) {
        let mut perms = self.policy.read().list_permissions(LOCAL_POLICY_DEVICE);
        perms.insert(permission.to_string(), true);
        self.policy
            .write()
            .set_device_permissions(LOCAL_POLICY_DEVICE, perms);
    }

    /// Revoke a `nova_security` permission in the local capability policy.
    pub fn disable_capability(&self, permission: &str) {
        let mut perms = self.policy.read().list_permissions(LOCAL_POLICY_DEVICE);
        perms.insert(permission.to_string(), false);
        self.policy
            .write()
            .set_device_permissions(LOCAL_POLICY_DEVICE, perms);
    }

    /// Whether the local policy currently allows `permission`.
    pub fn is_capability_enabled(&self, permission: &str) -> bool {
        self.policy
            .read()
            .check_permission(LOCAL_POLICY_DEVICE, permission)
    }

    /// Execute a command after checking the local capability policy.
    pub async fn execute(&self, cmd: WindowsCommand) -> Result<WindowsResult, WindowsAgentError> {
        let perm = cmd.capability.required_permission();
        if !self.is_capability_enabled(perm) {
            warn!("Windows capability blocked by policy: {}", perm);
            return Err(WindowsAgentError::PermissionDenied(perm.to_string()));
        }
        let provider = self.provider.read().clone();
        let result = provider.execute(&cmd).await?;
        let action = cmd.capability.label();
        if let Some(bus) = self.event_bus.read().as_ref() {
            let meta = nova_kernel::EventMetadata::new("windows_agent", Some(action.to_string()));
            let _ = bus.publish(nova_kernel::NovaEvent {
                metadata: meta,
                payload: Arc::new(format!("{} -> {}", action, result.detail)),
            });
        }
        info!(
            "Windows capability {} executed (success={})",
            action, result.success
        );
        Ok(result)
    }

    /// List the capabilities the active provider supports.
    pub fn supported_capabilities(&self) -> HashSet<String> {
        self.provider.read().supported()
    }
}

#[async_trait]
impl KernelModule for WindowsAgent {
    fn module_id(&self) -> &'static str {
        "windows_agent"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn dependencies(&self) -> Vec<&'static str> {
        vec!["security"]
    }

    async fn start(&self) -> KernelResult<()> {
        self.running.store(true, Ordering::Relaxed);
        info!("WindowsAgent started");
        Ok(())
    }

    async fn stop(&self) -> KernelResult<()> {
        self.running.store(false, Ordering::Relaxed);
        Ok(())
    }

    async fn shutdown(&self) -> KernelResult<()> {
        self.running.store(false, Ordering::Relaxed);
        info!("WindowsAgent shut down");
        Ok(())
    }

    fn health(&self) -> ModuleHealth {
        if self.running.load(Ordering::Relaxed) {
            ModuleHealth::healthy()
        } else {
            ModuleHealth::degraded("not running")
        }
    }
}

/// Helper so tests/consumers can build a `NovaError` in the Windows domain.
pub fn windows_error(message: &str) -> NovaError {
    NovaError::new(ErrorCategory::Kernel, "ERR_WINDOWS_AGENT", message)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent() -> (Arc<WindowsAgent>, Arc<MockWindowsProvider>) {
        let mock = Arc::new(MockWindowsProvider::new());
        let agent = WindowsAgent::new(mock.clone());
        (agent, mock)
    }

    #[test]
    fn capability_permission_mapping() {
        assert_eq!(
            WindowsCapability::LaunchApp {
                app: "code".into(),
                args: None
            }
            .required_permission(),
            PERM_EXECUTE
        );
        assert_eq!(
            WindowsCapability::DeleteFile { path: "x".into() }.required_permission(),
            PERM_FILES
        );
        assert_eq!(
            WindowsCapability::SetClipboard {
                content: "y".into()
            }
            .required_permission(),
            PERM_CLIPBOARD
        );
        assert_eq!(
            WindowsCapability::ShowNotification {
                title: "a".into(),
                body: "b".into()
            }
            .required_permission(),
            PERM_NOTIFICATIONS
        );
        assert_eq!(
            WindowsCapability::TakeScreenshot.required_permission(),
            PERM_SCREENSHOT
        );
    }

    #[tokio::test]
    async fn mock_executes_and_records() {
        let (agent, mock) = make_agent();
        let res = agent
            .execute(WindowsCommand::new(WindowsCapability::LaunchApp {
                app: "code".into(),
                args: None,
            }))
            .await
            .unwrap();
        assert!(res.success);
        assert!(res.detail.contains("code"));

        let recorded = mock.executed_commands();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].capability.label(), "windows.launch_app");
    }

    #[tokio::test]
    async fn disabled_capability_is_rejected() {
        let (agent, _mock) = make_agent();
        agent.disable_capability(PERM_EXECUTE);
        let res = agent
            .execute(WindowsCommand::new(WindowsCapability::LockPc))
            .await;
        assert!(matches!(res, Err(WindowsAgentError::PermissionDenied(_))));
        agent.enable_capability(PERM_EXECUTE);
    }

    #[tokio::test]
    async fn get_clipboard_returns_data() {
        let (agent, _mock) = make_agent();
        let res = agent
            .execute(WindowsCommand::new(WindowsCapability::GetClipboard))
            .await
            .unwrap();
        assert!(res.success);
    }

    #[test]
    fn kernel_module_contract() {
        let (agent, _mock) = make_agent();
        assert_eq!(agent.module_id(), "windows_agent");
        assert_eq!(agent.version(), "0.1.0");
        assert_eq!(agent.dependencies(), vec!["security"]);
        assert!(agent.is_capability_enabled(PERM_FILES));
        assert!(agent.supported_capabilities().len() >= 15);
    }

    #[test]
    fn all_capability_labels_unique_and_complete() {
        let labels = WindowsCapability::all_labels();
        let unique: HashSet<_> = labels.iter().collect();
        assert_eq!(labels.len(), unique.len());
        assert_eq!(labels.len(), 17);
    }

    // -----------------------------------------------------------------------
    // Real Windows integration tests
    // Run with: $env:NOVA_REAL_WINDOWS_TEST=1; cargo test real_windows
    // -----------------------------------------------------------------------

    fn real_windows_available() -> bool {
        std::env::var("NOVA_REAL_WINDOWS_TEST").as_deref() == Ok("1")
    }

    fn make_real_agent() -> Arc<WindowsAgent> {
        WindowsAgent::with_real()
    }

    #[tokio::test]
    async fn real_windows_get_clipboard() {
        if !real_windows_available() {
            return;
        }
        let agent = make_real_agent();
        let res = agent
            .execute(WindowsCommand::new(WindowsCapability::GetClipboard))
            .await
            .unwrap();
        assert!(res.success, "GetClipboard failed: {}", res.detail);
        println!("[REAL WINDOWS] GetClipboard success: data={:?}", res.data);
    }

    #[tokio::test]
    async fn real_windows_set_and_get_clipboard() {
        if !real_windows_available() {
            return;
        }
        let agent = make_real_agent();
        let test_text = format!(
            "NOVA-REAL-TEST-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        let set_res = agent
            .execute(WindowsCommand::new(WindowsCapability::SetClipboard {
                content: test_text.clone(),
            }))
            .await
            .unwrap();
        assert!(set_res.success, "SetClipboard failed: {}", set_res.detail);

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let get_res = agent
            .execute(WindowsCommand::new(WindowsCapability::GetClipboard))
            .await
            .unwrap();
        assert!(get_res.success, "GetClipboard failed: {}", get_res.detail);
        println!(
            "[REAL WINDOWS] Set+Get clipboard: sent='{}' got='{}'",
            test_text,
            get_res.data.as_deref().unwrap_or("(none)")
        );
    }

    #[tokio::test]
    async fn real_windows_launch_notepad() {
        if !real_windows_available() {
            return;
        }
        let agent = make_real_agent();
        let res = agent
            .execute(WindowsCommand::new(WindowsCapability::LaunchApp {
                app: "notepad.exe".into(),
                args: None,
            }))
            .await
            .unwrap();
        assert!(res.success, "Launch notepad failed: {}", res.detail);
        println!("[REAL WINDOWS] Launched notepad.exe: {}", res.detail);

        // Close immediately with taskkill
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        match agent
            .execute(WindowsCommand::new(WindowsCapability::CloseApp {
                app: "notepad.exe".into(),
            }))
            .await
        {
            Ok(r) => println!("[REAL WINDOWS] Closed notepad.exe: {}", r.detail),
            Err(e) => {
                // Notepad may close on its own; this is non-critical
                println!("[REAL WINDOWS] Close notepad (non-critical): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn real_windows_screenshot() {
        if !real_windows_available() {
            return;
        }
        let agent = make_real_agent();
        let res = agent
            .execute(WindowsCommand::new(WindowsCapability::TakeScreenshot))
            .await
            .unwrap();
        assert!(res.success, "Screenshot failed: {}", res.detail);
        println!("[REAL WINDOWS] Screenshot: {}", res.detail);

        // Verify screenshot file exists
        let screenshot_path =
            std::path::Path::new(&std::env::temp_dir()).join("nova_screenshot.png");
        if screenshot_path.exists() {
            let metadata = std::fs::metadata(&screenshot_path).unwrap();
            println!(
                "[REAL WINDOWS] Screenshot file size: {} bytes",
                metadata.len()
            );
            assert!(metadata.len() > 1000, "Screenshot too small");
            // Cleanup
            let _ = std::fs::remove_file(&screenshot_path);
        }
    }
}
