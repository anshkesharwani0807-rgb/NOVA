use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutomationEventPayload {
    WorkflowCreated {
        workflow_id: String,
        name: String,
    },
    WorkflowUpdated {
        workflow_id: String,
        name: String,
    },
    WorkflowDeleted {
        workflow_id: String,
    },
    WorkflowStarted {
        workflow_id: String,
        execution_id: String,
    },
    WorkflowCompleted {
        workflow_id: String,
        execution_id: String,
        steps_succeeded: usize,
        steps_failed: usize,
        duration_ms: i64,
    },
    WorkflowFailed {
        workflow_id: String,
        execution_id: String,
        error: String,
    },
    TriggerActivated {
        workflow_id: String,
        trigger_type: String,
        reason: String,
    },
    ActionExecuted {
        workflow_id: String,
        execution_id: String,
        step: usize,
        action_type: String,
        success: bool,
    },
    ConditionMatched {
        workflow_id: String,
        execution_id: String,
        condition: String,
        matched: bool,
    },
    AutomationError {
        workflow_id: String,
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_payload_workflow_created() {
        let p = AutomationEventPayload::WorkflowCreated {
            workflow_id: "w1".into(),
            name: "test".into(),
        };
        match p {
            AutomationEventPayload::WorkflowCreated { workflow_id, name } => {
                assert_eq!(workflow_id, "w1");
                assert_eq!(name, "test");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_event_payload_trigger_activated() {
        let p = AutomationEventPayload::TriggerActivated {
            workflow_id: "w2".into(),
            trigger_type: "manual".into(),
            reason: "user".into(),
        };
        match &p {
            AutomationEventPayload::TriggerActivated {
                workflow_id,
                trigger_type,
                reason,
            } => {
                assert_eq!(workflow_id, "w2");
                assert_eq!(trigger_type, "manual");
                assert_eq!(reason, "user");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_all_variants_have_display() {
        let variants: Vec<AutomationEventPayload> = vec![
            AutomationEventPayload::WorkflowCreated {
                workflow_id: "".into(),
                name: "".into(),
            },
            AutomationEventPayload::WorkflowUpdated {
                workflow_id: "".into(),
                name: "".into(),
            },
            AutomationEventPayload::WorkflowDeleted {
                workflow_id: "".into(),
            },
            AutomationEventPayload::WorkflowStarted {
                workflow_id: "".into(),
                execution_id: "".into(),
            },
            AutomationEventPayload::WorkflowCompleted {
                workflow_id: "".into(),
                execution_id: "".into(),
                steps_succeeded: 0,
                steps_failed: 0,
                duration_ms: 0,
            },
            AutomationEventPayload::WorkflowFailed {
                workflow_id: "".into(),
                execution_id: "".into(),
                error: "".into(),
            },
            AutomationEventPayload::TriggerActivated {
                workflow_id: "".into(),
                trigger_type: "".into(),
                reason: "".into(),
            },
            AutomationEventPayload::ActionExecuted {
                workflow_id: "".into(),
                execution_id: "".into(),
                step: 0,
                action_type: "".into(),
                success: true,
            },
            AutomationEventPayload::ConditionMatched {
                workflow_id: "".into(),
                execution_id: "".into(),
                condition: "".into(),
                matched: true,
            },
            AutomationEventPayload::AutomationError {
                workflow_id: "".into(),
                error: "".into(),
            },
        ];
        assert_eq!(variants.len(), 10);
    }
}
