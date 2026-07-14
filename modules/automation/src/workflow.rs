use serde::{Deserialize, Serialize};

use crate::action::ActionType;
use crate::condition::Condition;
use crate::trigger::TriggerType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub triggers: Vec<TriggerConfig>,
    pub steps: Vec<WorkflowStep>,
    pub parallel: bool,
    pub max_retries: u32,
    pub timeout_ms: u64,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    pub trigger: TriggerType,
    pub conditions: Option<Vec<Condition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub name: String,
    pub action: ActionType,
    pub condition: Option<Condition>,
    pub retry_count: u32,
    pub timeout_ms: u64,
    pub continue_on_failure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkflowState {
    Active,
    Paused,
    Disabled,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub trigger_count: usize,
    pub step_count: usize,
    pub state: WorkflowState,
    pub created_at: i64,
    pub tags: Vec<String>,
}

impl Workflow {
    pub fn new(id: String, name: String, description: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id,
            name,
            description,
            enabled: true,
            triggers: Vec::new(),
            steps: Vec::new(),
            parallel: false,
            max_retries: 0,
            timeout_ms: 30_000,
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn summary(&self) -> WorkflowSummary {
        WorkflowSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            enabled: self.enabled,
            trigger_count: self.triggers.len(),
            step_count: self.steps.len(),
            state: if self.enabled {
                WorkflowState::Active
            } else {
                WorkflowState::Disabled
            },
            created_at: self.created_at,
            tags: self.tags.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), crate::error::AutomationError> {
        if self.id.is_empty() {
            return Err(crate::error::AutomationError::InvalidWorkflow(
                "id is empty".to_string(),
            ));
        }
        if self.name.is_empty() {
            return Err(crate::error::AutomationError::InvalidWorkflow(
                "name is empty".to_string(),
            ));
        }
        if self.triggers.is_empty() && self.steps.is_empty() {
            return Err(crate::error::AutomationError::InvalidWorkflow(
                "workflow has no triggers or steps".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_creation() {
        let wf = Workflow::new("id1".into(), "Test".into(), "A test workflow".into());
        assert_eq!(wf.id, "id1");
        assert_eq!(wf.name, "Test");
        assert!(wf.enabled);
    }

    #[test]
    fn test_workflow_validate_empty_id_fails() {
        let wf = Workflow::new("".into(), "name".into(), "".into());
        assert!(wf.validate().is_err());
    }

    #[test]
    fn test_workflow_validate_empty_name_fails() {
        let wf = Workflow::new("id2".into(), "".into(), "".into());
        assert!(wf.validate().is_err());
    }

    #[test]
    fn test_workflow_step_defaults() {
        let step = WorkflowStep {
            id: "s1".into(),
            name: "step1".into(),
            action: crate::action::ActionType::Speak {
                text: "hello".into(),
            },
            condition: None,
            retry_count: 0,
            timeout_ms: 30_000,
            continue_on_failure: false,
        };
        assert_eq!(step.retry_count, 0);
        assert!(step.condition.is_none());
    }

    #[test]
    fn test_workflow_summary() {
        let wf = Workflow::new("id3".into(), "My WF".into(), "desc".into());
        let summary = wf.summary();
        assert_eq!(summary.id, "id3");
        assert_eq!(summary.state, WorkflowState::Active);
    }

    #[test]
    fn test_trigger_config_basic() {
        let tc = TriggerConfig {
            trigger: crate::trigger::TriggerType::Manual,
            conditions: None,
        };
        assert!(tc.conditions.is_none());
    }
}
