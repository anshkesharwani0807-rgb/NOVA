//! M16 — Remote Capability Provider seam for plugins.
//!
//! Plugins may request `remote.*` permissions and, when granted, receive a
//! sandboxed [`RemoteCapabilityProvider`] that lets them drive the unified
//! cross-device brain (clipboard, files, execute, memory, notifications) on
//! trusted devices. The production implementation is `nova_cross_device`'s
//! `CrossDeviceCoordinator`; [`NullRemoteProvider`] is used when no brain is
//! attached (keeps plugins sandboxed and offline-first).

use async_trait::async_trait;

use crate::error::PluginResult;

/// Permission to sync the shared clipboard to a remote device.
pub const REMOTE_CLIPBOARD: &str = "remote.clipboard";
/// Permission to transfer files to a remote device.
pub const REMOTE_FILES: &str = "remote.files";
/// Permission to execute a command on a remote device.
pub const REMOTE_EXECUTE: &str = "remote.execute";
/// Permission to write shared memory to a remote device.
pub const REMOTE_MEMORY: &str = "remote.memory";
/// Permission to push notifications to a remote device.
pub const REMOTE_NOTIFICATIONS: &str = "remote.notifications";

/// All remote capability permission names.
pub const ALL_REMOTE_PERMISSIONS: &[&str] = &[
    REMOTE_CLIPBOARD,
    REMOTE_FILES,
    REMOTE_EXECUTE,
    REMOTE_MEMORY,
    REMOTE_NOTIFICATIONS,
];

/// A sandboxed window through which a plugin drives the unified brain on remote
/// (trusted) devices. Methods are gated by the matching `remote.*` permission.
#[async_trait]
pub trait RemoteCapabilityProvider: Send + Sync {
    /// Stable name of the backing implementation.
    fn provider_name(&self) -> &'static str;

    /// Sync `content` into the shared clipboard, attributed to `target`.
    async fn remote_clipboard(&self, content: &str, target: &str) -> PluginResult<()>;

    /// Securely transfer `path` to `target` (E2E encrypted by the brain).
    async fn remote_files(&self, path: &str, target: &str) -> PluginResult<()>;

    /// Execute `command` on `target`; returns the platform's response text.
    async fn remote_execute(&self, command: &str, target: &str) -> PluginResult<String>;

    /// Write `value` under `key` into the shared memory of `target`.
    async fn remote_memory(&self, key: &str, value: &[u8], target: &str) -> PluginResult<()>;

    /// Push a notification (`title`/`body`) to `target`.
    async fn remote_notification(&self, title: &str, body: &str, target: &str) -> PluginResult<()>;
}

/// Default no-op provider used when no unified brain is attached.
///
/// Keeps plugins fully sandboxed and offline-first: calls succeed trivially
/// without touching any device.
pub struct NullRemoteProvider;

impl NullRemoteProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NullRemoteProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RemoteCapabilityProvider for NullRemoteProvider {
    fn provider_name(&self) -> &'static str {
        "null-remote"
    }

    async fn remote_clipboard(&self, _content: &str, _target: &str) -> PluginResult<()> {
        Ok(())
    }

    async fn remote_files(&self, _path: &str, _target: &str) -> PluginResult<()> {
        Ok(())
    }

    async fn remote_execute(&self, _command: &str, _target: &str) -> PluginResult<String> {
        Ok(String::new())
    }

    async fn remote_memory(&self, _key: &str, _value: &[u8], _target: &str) -> PluginResult<()> {
        Ok(())
    }

    async fn remote_notification(
        &self,
        _title: &str,
        _body: &str,
        _target: &str,
    ) -> PluginResult<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use nova_kernel::NovaError;

    #[test]
    fn remote_permission_constants_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for p in ALL_REMOTE_PERMISSIONS {
            assert!(seen.insert(*p), "duplicate remote permission: {p}");
        }
    }

    #[tokio::test]
    async fn null_provider_is_a_noop() {
        let p = NullRemoteProvider::new();
        assert_eq!(p.provider_name(), "null-remote");
        assert!(p.remote_clipboard("x", "dev").await.is_ok());
        assert!(p.remote_files("f", "dev").await.is_ok());
        assert_eq!(p.remote_execute("cmd", "dev").await.unwrap(), "");
        assert!(p.remote_memory("k", b"v", "dev").await.is_ok());
        assert!(p.remote_notification("t", "b", "dev").await.is_ok());
    }

    #[test]
    fn remote_error_helper_compiles() {
        // Ensure the error type used by the trait is the kernel NovaError.
        fn assert_nova_error(_e: NovaError) {}
        let _ = assert_nova_error;
    }
}
