use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::AutomationError;
use crate::workflow::{Workflow, WorkflowSummary};

pub struct WorkflowRegistry {
    workflows: RwLock<HashMap<String, Arc<Workflow>>>,
}

impl WorkflowRegistry {
    pub fn new() -> Self {
        Self {
            workflows: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, workflow: Workflow) -> Result<(), AutomationError> {
        workflow.validate()?;
        let id = workflow.id.clone();
        let mut map = self.workflows.write();
        if map.contains_key(&id) {
            return Err(AutomationError::WorkflowAlreadyExists(id));
        }
        map.insert(id, Arc::new(workflow));
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<Arc<Workflow>> {
        self.workflows.read().get(id).cloned()
    }

    pub fn update(&self, workflow: Workflow) -> Result<(), AutomationError> {
        workflow.validate()?;
        let id = workflow.id.clone();
        let mut map = self.workflows.write();
        if !map.contains_key(&id) {
            return Err(AutomationError::WorkflowNotFound(id));
        }
        map.insert(id, Arc::new(workflow));
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), AutomationError> {
        let mut map = self.workflows.write();
        map.remove(id)
            .ok_or_else(|| AutomationError::WorkflowNotFound(id.to_string()))?;
        Ok(())
    }

    pub fn list(&self) -> Vec<WorkflowSummary> {
        self.workflows
            .read()
            .values()
            .map(|wf| wf.summary())
            .collect()
    }

    pub fn list_enabled(&self) -> Vec<Arc<Workflow>> {
        self.workflows
            .read()
            .values()
            .filter(|wf| wf.enabled)
            .cloned()
            .collect()
    }

    pub fn all(&self) -> Vec<Arc<Workflow>> {
        self.workflows.read().values().cloned().collect()
    }

    pub fn enable(&self, id: &str) -> Result<(), AutomationError> {
        let mut map = self.workflows.write();
        let wf = map
            .get_mut(id)
            .ok_or_else(|| AutomationError::WorkflowNotFound(id.to_string()))?;
        Arc::make_mut(wf).enabled = true;
        Ok(())
    }

    pub fn disable(&self, id: &str) -> Result<(), AutomationError> {
        let mut map = self.workflows.write();
        let wf = map
            .get_mut(id)
            .ok_or_else(|| AutomationError::WorkflowNotFound(id.to_string()))?;
        Arc::make_mut(wf).enabled = false;
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.workflows.read().len()
    }

    pub fn find_by_trigger(&self, trigger_kind: &str) -> Vec<Arc<Workflow>> {
        self.workflows
            .read()
            .values()
            .filter(|wf| {
                wf.enabled
                    && wf.triggers.iter().any(|t| {
                        let name = match &t.trigger {
                            crate::trigger::TriggerType::Time { .. } => "time",
                            crate::trigger::TriggerType::Date { .. } => "date",
                            crate::trigger::TriggerType::Manual => "manual",
                            crate::trigger::TriggerType::EventBus { .. } => "event_bus",
                            crate::trigger::TriggerType::Memory { .. } => "memory",
                            crate::trigger::TriggerType::Voice { .. } => "voice",
                            crate::trigger::TriggerType::Battery { .. } => "battery",
                            crate::trigger::TriggerType::Charging { .. } => "charging",
                            crate::trigger::TriggerType::WiFi { .. } => "wifi",
                            crate::trigger::TriggerType::Bluetooth { .. } => "bluetooth",
                            crate::trigger::TriggerType::DeviceState { .. } => "device_state",
                            crate::trigger::TriggerType::Vision { .. } => "vision",
                            crate::trigger::TriggerType::Plugin { .. } => "plugin",
                        };
                        name == trigger_kind
                    })
            })
            .cloned()
            .collect()
    }
}

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::Workflow;

    fn make_wf(id: &str, name: &str) -> Workflow {
        let mut wf = Workflow::new(id.into(), name.into(), "desc".into());
        wf.triggers.push(crate::workflow::TriggerConfig {
            trigger: crate::trigger::TriggerType::Manual,
            conditions: None,
        });
        wf
    }

    #[test]
    fn test_register_and_get() {
        let reg = WorkflowRegistry::new();
        let wf = make_wf("r1", "R1");
        assert!(reg.register(wf.clone()).is_ok());
        assert!(reg.get("r1").is_some());
    }

    #[test]
    fn test_register_duplicate() {
        let reg = WorkflowRegistry::new();
        let wf = make_wf("r2", "R2");
        reg.register(wf.clone()).unwrap();
        assert!(reg.register(wf).is_err());
    }

    #[test]
    fn test_delete() {
        let reg = WorkflowRegistry::new();
        let wf = make_wf("r3", "R3");
        reg.register(wf).unwrap();
        reg.delete("r3").unwrap();
        assert!(reg.get("r3").is_none());
    }

    #[test]
    fn test_enable_disable() {
        let reg = WorkflowRegistry::new();
        let wf = make_wf("r4", "R4");
        reg.register(wf).unwrap();
        reg.disable("r4").unwrap();
        assert!(!reg.get("r4").unwrap().enabled);
        reg.enable("r4").unwrap();
        assert!(reg.get("r4").unwrap().enabled);
    }

    #[test]
    fn test_list_enabled() {
        let reg = WorkflowRegistry::new();
        let mut wf1 = make_wf("r5", "R5");
        wf1.enabled = true;
        let mut wf2 = make_wf("r6", "R6");
        wf2.enabled = false;
        reg.register(wf1).unwrap();
        reg.register(wf2).unwrap();
        assert_eq!(reg.list_enabled().len(), 1);
        assert_eq!(reg.all().len(), 2);
    }

    #[test]
    fn test_find_by_trigger() {
        let reg = WorkflowRegistry::new();
        let mut wf = Workflow::new("r7".into(), "R7".into(), "desc".into());
        wf.triggers.push(crate::workflow::TriggerConfig {
            trigger: crate::trigger::TriggerType::Manual,
            conditions: None,
        });
        reg.register(wf).unwrap();
        let found = reg.find_by_trigger("manual");
        assert_eq!(found.len(), 1);
        let found2 = reg.find_by_trigger("time");
        assert_eq!(found2.len(), 0);
    }
}
