mod action;
mod condition;
mod config;
mod consent_gate;
mod controller;
mod error;
mod events;
mod execution;
mod execution_plan_adapter;
mod history;
mod observability;
mod outcome_verifier;
mod pipeline_step;
mod plan_executor;
mod planner;
mod real_executors;
mod recovery_orchestrator;
mod registry;
mod scheduler;
mod screen_executor;
mod trigger;
mod workflow;
mod world_state;

pub use action::*;
pub use condition::*;
pub use config::*;
pub use consent_gate::*;
pub use controller::*;
pub use error::*;
pub use events::*;
pub use execution::*;
pub use execution_plan_adapter::*;
pub use history::*;
pub use observability::*;
pub use outcome_verifier::*;
pub use pipeline_step::*;
pub use plan_executor::*;
pub use planner::*;
pub use real_executors::*;
pub use recovery_orchestrator::*;
pub use registry::*;
pub use scheduler::*;
pub use screen_executor::*;
pub use trigger::*;
pub use workflow::*;
pub use world_state::*;

use async_trait::async_trait;
use nova_kernel::module::{KernelModule, ModuleHealth};
use nova_kernel::{log_activity, EventBus, EventMetadata, NovaEvent, Result as NovaResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct AutomationEngine {
    config: parking_lot::RwLock<AutomationConfig>,
    registry: Arc<WorkflowRegistry>,
    scheduler: parking_lot::RwLock<Scheduler>,
    executor: Arc<ExecutionEngine>,
    history: Arc<dyn HistoryStore>,
    event_bus: parking_lot::RwLock<Option<Arc<EventBus>>>,
    shutdown_tx: parking_lot::RwLock<Option<mpsc::Sender<()>>>,
}

impl AutomationEngine {
    pub fn new() -> Self {
        let cfg = AutomationConfig::default();
        let history = Arc::new(InMemoryHistory::with_max(cfg.history_max_entries));
        Self {
            config: parking_lot::RwLock::new(cfg.clone()),
            registry: Arc::new(WorkflowRegistry::new()),
            scheduler: parking_lot::RwLock::new(Scheduler::new(cfg.clone())),
            executor: Arc::new(ExecutionEngine::new(cfg, history.clone())),
            history,
            event_bus: parking_lot::RwLock::new(None),
            shutdown_tx: parking_lot::RwLock::new(None),
        }
    }

    pub fn set_event_bus(&self, bus: Arc<EventBus>) {
        *self.event_bus.write() = Some(bus);
    }

    pub fn set_consent_gate(&self, gate: Arc<ConsentGate>) {
        self.executor.set_consent_gate(gate);
    }

    pub fn set_autonomy_level(&self, level: &str) {
        self.executor.set_autonomy_level(level);
    }

    pub fn registry(&self) -> &WorkflowRegistry {
        &self.registry
    }

    pub fn history(&self) -> &dyn HistoryStore {
        self.history.as_ref()
    }

    pub fn executor(&self) -> &ExecutionEngine {
        &self.executor
    }

    pub fn create_workflow(&self, name: &str, description: &str) -> Workflow {
        Workflow::new(
            uuid::Uuid::new_v4().to_string(),
            name.to_string(),
            description.to_string(),
        )
    }

    pub fn register_workflow(&self, workflow: Workflow) -> Result<(), AutomationError> {
        let id = workflow.id.clone();
        let name = workflow.name.clone();
        self.registry.register(workflow)?;
        self.publish(AutomationEventPayload::WorkflowCreated {
            workflow_id: id,
            name,
        });
        Ok(())
    }

    pub fn trigger_manual(&self, workflow_id: &str) -> Result<String, AutomationError> {
        let wf = self
            .registry
            .get(workflow_id)
            .ok_or_else(|| AutomationError::WorkflowNotFound(workflow_id.to_string()))?;
        if !wf.enabled {
            return Err(AutomationError::WorkflowDisabled(workflow_id.to_string()));
        }

        self.publish(AutomationEventPayload::TriggerActivated {
            workflow_id: workflow_id.to_string(),
            trigger_type: "manual".to_string(),
            reason: "manual trigger".to_string(),
        });

        log_activity(
            "automation",
            "manual_trigger",
            &format!("workflow={}", workflow_id),
            None,
        );
        let ctx = HashMap::new();
        let publish = |payload: AutomationEventPayload| self.publish(payload);
        Ok(self.executor.execute_workflow(wf, ctx, &publish))
    }

    pub fn cancel_execution(&self, execution_id: &str) -> Result<(), AutomationError> {
        self.executor.cancel_execution(execution_id)
    }

    pub fn get_config(&self) -> parking_lot::RwLockReadGuard<'_, AutomationConfig> {
        self.config.read()
    }

    pub fn get_config_mut(&self) -> parking_lot::RwLockWriteGuard<'_, AutomationConfig> {
        self.config.write()
    }

    fn publish(&self, payload: AutomationEventPayload) {
        if let Some(ref bus) = *self.event_bus.read() {
            let meta = EventMetadata::new("automation", None);
            let event = NovaEvent {
                metadata: meta,
                payload: Arc::new(payload),
            };
            let _ = bus.publish(event);
        }
    }
}

impl Default for AutomationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KernelModule for AutomationEngine {
    fn module_id(&self) -> &'static str {
        "automation"
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

    async fn start(&self) -> NovaResult<()> {
        let (tx, mut rx) = mpsc::channel::<()>(16);
        *self.shutdown_tx.write() = Some(tx);

        let scheduler = self.scheduler.read().tick_interval_ms();
        let registry = self.registry.clone();
        let _executor = self.executor.clone();
        let eb = self.event_bus.read().clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(scheduler));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let workflows = registry.all();
                        let _ctx = HashMap::<String, String>::new();
                        // scheduler tick - simplified; actual trigger check is done by external calls
                        for wf in &workflows {
                            if wf.enabled && wf.triggers.iter().any(|t| matches!(t.trigger, crate::trigger::TriggerType::Manual)) {
                                if let Some(ref bus) = eb {
                                    let meta = EventMetadata::new("automation", None);
                                    let payload = AutomationEventPayload::TriggerActivated {
                                        workflow_id: wf.id.clone(),
                                        trigger_type: "scheduler".to_string(),
                                        reason: "scheduler tick".to_string(),
                                    };
                                    let _ = bus.publish(NovaEvent {
                                        metadata: meta,
                                        payload: Arc::new(payload),
                                    });
                                }
                            }
                        }
                    }
                    _ = rx.recv() => break,
                }
            }
        });

        log_activity("automation", "started", "automation engine started", None);
        Ok(())
    }

    async fn stop(&self) -> NovaResult<()> {
        let tx = self.shutdown_tx.write().take();
        if let Some(tx) = tx {
            let _ = tx.send(()).await;
        }
        log_activity("automation", "stopped", "automation engine stopped", None);
        Ok(())
    }
}
