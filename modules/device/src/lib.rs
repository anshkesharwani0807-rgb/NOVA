pub mod config;
pub mod events;
pub mod permission;
pub mod providers;
pub mod services;
pub mod tools;

pub use config::DeviceConfig;
pub use events::{DeviceEvent, DeviceEventPayload};
pub use permission::{DeviceCapability, PermissionManager, PermissionState};
pub use providers::mock::MockDeviceProvider;
pub use providers::DeviceProvider;
pub use tools::DeviceToolkit;

use async_trait::async_trait;
use nova_ai::tool::ToolRegistry;
use nova_kernel::{
    EventMetadata, HealthStatus, Kernel, KernelModule, ModuleHealth, NovaEvent, Result,
};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use uuid::Uuid;

pub struct DeviceSystem {
    kernel: Arc<Kernel>,
    provider: Arc<dyn DeviceProvider>,
    permissions: Arc<PermissionManager>,
    config: Arc<RwLock<DeviceConfig>>,
    audit: Arc<RwLock<Vec<DeviceEvent>>>,
    services: Arc<services::ServiceRegistry>,
    toolkit: Arc<DeviceToolkit>,
    monitor_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl DeviceSystem {
    pub fn new(kernel: Arc<Kernel>) -> Self {
        let provider = Arc::new(MockDeviceProvider::new()) as Arc<dyn DeviceProvider>;
        let permissions = Arc::new(PermissionManager::new());
        let config = Arc::new(RwLock::new(DeviceConfig::default()));
        let audit = Arc::new(RwLock::new(vec![]));
        let toolkit = Arc::new(DeviceToolkit::new(
            provider.clone(),
            permissions.clone(),
            audit.clone(),
        ));

        let mut service_registry = services::ServiceRegistry::new();
        service_registry.register(Arc::new(services::clipboard::ClipboardMonitor::new(
            provider.clone(),
            &kernel,
        )));
        service_registry.register(Arc::new(services::battery::BatteryMonitor::new(
            provider.clone(),
            &kernel,
            config.clone(),
        )));
        service_registry.register(Arc::new(services::connectivity::ConnectivityMonitor::new(
            provider.clone(),
            &kernel,
        )));
        service_registry.register(Arc::new(services::storage::StorageMonitor::new(
            provider.clone(),
            &kernel,
        )));
        service_registry.register(Arc::new(services::sensors::SensorMonitor::new(
            provider.clone(),
            &kernel,
        )));
        service_registry.register(Arc::new(services::notifications::NotificationMonitor::new(
            provider.clone(),
            &kernel,
            config.clone(),
        )));

        Self {
            kernel,
            provider,
            permissions,
            config,
            audit,
            services: Arc::new(service_registry),
            toolkit,
            monitor_handle: Arc::new(RwLock::new(None)),
        }
    }

    pub fn provider(&self) -> &Arc<dyn DeviceProvider> {
        &self.provider
    }

    pub fn permissions(&self) -> &Arc<PermissionManager> {
        &self.permissions
    }

    pub fn config(&self) -> &Arc<RwLock<DeviceConfig>> {
        &self.config
    }

    pub fn toolkit(&self) -> &Arc<DeviceToolkit> {
        &self.toolkit
    }

    pub fn audit_log(&self) -> Vec<DeviceEvent> {
        self.audit.read().clone()
    }

    pub fn register_tools(&self, tool_registry: &ToolRegistry) -> Result<()> {
        for tool in &self.toolkit.tools {
            tool_registry.register(tool.clone())?;
        }
        Ok(())
    }

    pub fn update_config(&self, new_config: DeviceConfig) {
        *self.config.write() = new_config;
    }

    #[allow(dead_code)]
    fn publish_event(&self, event: DeviceEvent) {
        let action = event.action_name().to_string();
        let desc = event.description();
        nova_kernel::log_activity("device", &action, &desc, Some(event.correlation_id));
        let meta = EventMetadata::new("device", Some(action));
        let _ = self.kernel.event_bus.publish(NovaEvent {
            metadata: meta,
            payload: Arc::new(event.clone()),
        });
        self.audit.write().push(event);
    }

    async fn run_monitor_loop(
        _permissions: Arc<PermissionManager>,
        provider: Arc<dyn DeviceProvider>,
        event_bus: Arc<nova_kernel::event_bus::EventBus>,
        config: Arc<RwLock<DeviceConfig>>,
        audit: Arc<RwLock<Vec<DeviceEvent>>>,
    ) {
        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;

            let monitor_battery;
            let monitor_connectivity;
            {
                let cfg = config.read();
                if !cfg.monitor_battery
                    && !cfg.monitor_clipboard
                    && !cfg.monitor_connectivity
                    && !cfg.monitor_storage
                    && !cfg.monitor_sensors
                    && !cfg.monitor_notifications
                {
                    continue;
                }
                monitor_battery = cfg.monitor_battery;
                monitor_connectivity = cfg.monitor_connectivity;
            }

            if monitor_battery {
                if let Ok(status) = provider.get_battery_status().await {
                    let level = status.level;
                    let threshold;
                    {
                        let cfg = config.read();
                        threshold = cfg.battery_low_threshold_pct;
                    }
                    if level <= threshold {
                        let event = DeviceEvent::new(
                            Uuid::new_v4(),
                            DeviceEventPayload::BatteryLow { level },
                        );
                        let action = event.action_name().to_string();
                        let desc = event.description();
                        nova_kernel::log_activity(
                            "device",
                            &action,
                            &desc,
                            Some(event.correlation_id),
                        );
                        let meta = EventMetadata::new("device", Some(action));
                        let _ = event_bus.publish(NovaEvent {
                            metadata: meta,
                            payload: Arc::new(event.clone()),
                        });
                        audit.write().push(event);
                    }
                }
            }
            if monitor_connectivity {
                if let Ok((online, net_type)) = provider.get_connectivity_status().await {
                    let event = DeviceEvent::new(
                        Uuid::new_v4(),
                        DeviceEventPayload::ConnectivityChanged {
                            online,
                            network_type: net_type,
                        },
                    );
                    let action = event.action_name().to_string();
                    let desc = event.description();
                    nova_kernel::log_activity("device", &action, &desc, Some(event.correlation_id));
                    let meta = EventMetadata::new("device", Some(action));
                    let _ = event_bus.publish(NovaEvent {
                        metadata: meta,
                        payload: Arc::new(event.clone()),
                    });
                    audit.write().push(event);
                }
            }
        }
    }
}

#[async_trait]
impl KernelModule for DeviceSystem {
    fn module_id(&self) -> &'static str {
        "device"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn initialize(&self) -> Result<()> {
        tracing::info!(
            "DeviceSystem initialized ({} tools, {} services, {} permissions)",
            self.toolkit.count(),
            self.services.count(),
            self.permissions.count(),
        );
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        self.services.start_all().await?;

        let permissions = self.permissions.clone();
        let provider = self.provider.clone();
        let event_bus = self.kernel.event_bus.clone();
        let config = self.config.clone();
        let audit = self.audit.clone();

        let handle = tokio::spawn(async move {
            Self::run_monitor_loop(permissions, provider, event_bus, config, audit).await;
        });
        *self.monitor_handle.write() = Some(handle);

        tracing::info!("DeviceSystem started (monitor loop active)");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.services.stop_all().await?;
        if let Some(handle) = self.monitor_handle.write().take() {
            handle.abort();
        }
        tracing::info!("DeviceSystem stopped");
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("DeviceSystem shut down");
        Ok(())
    }

    fn health(&self) -> ModuleHealth {
        ModuleHealth {
            status: HealthStatus::Healthy,
            detail: format!(
                "{} tools, {} services, {} permissions granted",
                self.toolkit.count(),
                self.services.count(),
                self.permissions.list_grants().len(),
            ),
        }
    }
}
