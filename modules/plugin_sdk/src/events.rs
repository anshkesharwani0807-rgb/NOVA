use nova_kernel::{log_activity, EventMetadata, NovaEvent};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEventPayload {
    pub event_type: PluginEventType,
    pub plugin_id: String,
    pub plugin_name: String,
    pub version: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginEventType {
    PluginInstalled,
    PluginEnabled,
    PluginDisabled,
    PluginUpdated,
    PluginRemoved,
    PluginCrashed,
    PluginPermissionDenied,
    PluginLoaded,
    PluginUnloaded,
}

pub fn publish_plugin_event(
    event_bus: &Arc<nova_kernel::EventBus>,
    event_type: PluginEventType,
    plugin_id: &str,
    plugin_name: &str,
    version: &str,
    detail: &str,
) {
    let payload = PluginEventPayload {
        event_type: event_type.clone(),
        plugin_id: plugin_id.to_string(),
        plugin_name: plugin_name.to_string(),
        version: version.to_string(),
        detail: detail.to_string(),
    };

    let meta = EventMetadata::new(
        "plugin_sdk",
        Some(format!("plugin_{:?}", event_type).to_lowercase()),
    );
    let event = NovaEvent {
        metadata: meta,
        payload: Arc::new(payload),
    };

    event_bus.publish(event).ok();

    log_activity(
        "plugin_sdk",
        &format!("plugin_{:?}", event_type).to_lowercase(),
        &format!("Plugin '{}' ({}) event: {}", plugin_id, version, detail),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_kernel::EventBus;

    #[test]
    fn test_plugin_event_creation() {
        let payload = PluginEventPayload {
            event_type: PluginEventType::PluginInstalled,
            plugin_id: "hello".to_string(),
            plugin_name: "Hello Plugin".to_string(),
            version: "1.0.0".to_string(),
            detail: "Installed successfully".to_string(),
        };
        assert_eq!(payload.event_type, PluginEventType::PluginInstalled);
        assert_eq!(payload.plugin_id, "hello");
    }

    #[tokio::test]
    async fn test_publish_plugin_event() {
        let event_bus = Arc::new(EventBus::new(16));
        let mut rx = event_bus.subscribe();

        publish_plugin_event(
            &event_bus,
            PluginEventType::PluginEnabled,
            "test",
            "Test Plugin",
            "1.0",
            "Enabled",
        );

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let received = rx.try_recv();
        assert!(received.is_ok());
        if let Ok(event) = received {
            let payload = event.payload.downcast_ref::<PluginEventPayload>();
            assert!(payload.is_some());
            assert_eq!(payload.unwrap().plugin_id, "test");
        }
    }
}
