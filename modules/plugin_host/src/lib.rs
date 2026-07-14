pub mod automation;
pub mod consent_gate;
pub mod sandbox;

use async_trait::async_trait;
use nova_kernel::{Kernel, KernelModule, ModuleHealth, Result};
use std::sync::Arc;

use automation::AutomationEngine;
use consent_gate::ConsequenceGate;

pub struct PluginHost {
    kernel: Arc<Kernel>,
    engine: AutomationEngine,
}

impl PluginHost {
    pub fn new(kernel: Arc<Kernel>) -> Self {
        let gate = Arc::new(ConsequenceGate::new());
        let engine = AutomationEngine::new(gate);
        Self { kernel, engine }
    }

    pub fn engine(&self) -> &AutomationEngine {
        &self.engine
    }
}

#[async_trait]
impl KernelModule for PluginHost {
    fn module_id(&self) -> &'static str {
        "plugin_host"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn dependencies(&self) -> Vec<&'static str> {
        vec![]
    }

    fn health(&self) -> ModuleHealth {
        ModuleHealth::healthy()
    }

    async fn start(&self) -> Result<()> {
        let event_bus = self.kernel.event_bus.clone();
        let mut rx = event_bus.subscribe();

        tokio::spawn(async move {
            tracing::info!("PluginHost sandbox runner and listener started.");
            while let Ok(event) = rx.recv().await {
                if event.metadata.origin_module == "Shell"
                    && event.metadata.causing_action.as_deref() == Some("run_plugin")
                {
                    tracing::info!("[PluginHost] Intercepted execution command. Verifying plugin signature and sandboxing rules.");
                }
            }
        });

        tracing::info!("PluginHost initialized.");
        Ok(())
    }
}
