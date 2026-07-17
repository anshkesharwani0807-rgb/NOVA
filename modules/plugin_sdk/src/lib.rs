pub mod context;
pub mod error;
pub mod events;
pub mod lifecycle;
pub mod loader;
pub mod manager;
pub mod manifest;
pub mod permissions;
pub mod plugin;
pub mod registry;
pub mod remote;
pub mod sandbox;
pub mod storage;

pub use context::PluginContext;
pub use error::{plugin_error, PluginResult};
pub use events::{PluginEventPayload, PluginEventType};
pub use lifecycle::PluginLifecycleManager;
pub use loader::PluginLoader;
pub use manager::PluginManager;
pub use manifest::PluginManifest;
pub use permissions::PluginPermissionManager;
pub use plugin::Plugin;
pub use registry::{PluginEntry, PluginRegistry, PluginState};
pub use remote::{
    NullRemoteProvider, RemoteCapabilityProvider, ALL_REMOTE_PERMISSIONS, REMOTE_CLIPBOARD,
    REMOTE_EXECUTE, REMOTE_FILES, REMOTE_MEMORY, REMOTE_NOTIFICATIONS,
};
pub use sandbox::{PluginSandbox, Sandbox};
pub use storage::PluginStorage;
