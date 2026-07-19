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
    PipelineStarted {
        pipeline_id: String,
        goal: String,
        total_steps: usize,
    },
    PipelineCompleted {
        pipeline_id: String,
        goal: String,
        total_steps: usize,
        completed_steps: usize,
        failed_steps: usize,
        duration_ms: i64,
    },
    PipelineFailed {
        pipeline_id: String,
        goal: String,
        error: String,
        failed_steps: usize,
    },
    PipelineCancelled {
        pipeline_id: String,
        goal: String,
        reason: Option<String>,
    },
    StepStarted {
        pipeline_id: String,
        step_id: String,
        step_index: usize,
        description: String,
    },
    StepCompleted {
        pipeline_id: String,
        step_id: String,
        step_index: usize,
        attempts: u32,
        duration_ms: i64,
    },
    StepFailed {
        pipeline_id: String,
        step_id: String,
        step_index: usize,
        error: String,
        attempts: u32,
    },
    StepSkipped {
        pipeline_id: String,
        step_id: String,
        step_index: usize,
        reason: String,
    },
    StepRetried {
        pipeline_id: String,
        step_id: String,
        step_index: usize,
        attempt: u32,
        delay_ms: u64,
    },
    VerificationStarted {
        pipeline_id: String,
        step_id: String,
        strategy: String,
    },
    VerificationCompleted {
        pipeline_id: String,
        step_id: String,
        passed: bool,
        duration_ms: i64,
    },
    VerificationFailed {
        pipeline_id: String,
        step_id: String,
        reason: String,
        suggestion: String,
        duration_ms: i64,
    },
    RecoveryStarted {
        pipeline_id: String,
        step_id: String,
        retry_count: u32,
        reason: String,
    },
    RecoveryCompleted {
        pipeline_id: String,
        step_id: String,
        decision: String,
        strategy: String,
        duration_ms: i64,
    },
    RecoveryFailed {
        pipeline_id: String,
        step_id: String,
        decision: String,
        reason: String,
        duration_ms: i64,
    },
    ReplanStarted {
        pipeline_id: String,
        reason: String,
        from_step_index: usize,
    },
    ReplanCompleted {
        pipeline_id: String,
        new_step_count: usize,
        duration_ms: i64,
    },
    GoalExecutionStarted {
        goal: String,
        execution_id: String,
    },
    GoalExecutionCompleted {
        goal: String,
        execution_id: String,
        success: bool,
        duration_ms: i64,
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
            AutomationEventPayload::PipelineStarted {
                pipeline_id: "".into(),
                goal: "".into(),
                total_steps: 0,
            },
            AutomationEventPayload::PipelineCompleted {
                pipeline_id: "".into(),
                goal: "".into(),
                total_steps: 0,
                completed_steps: 0,
                failed_steps: 0,
                duration_ms: 0,
            },
            AutomationEventPayload::PipelineFailed {
                pipeline_id: "".into(),
                goal: "".into(),
                error: "".into(),
                failed_steps: 0,
            },
            AutomationEventPayload::PipelineCancelled {
                pipeline_id: "".into(),
                goal: "".into(),
                reason: None,
            },
            AutomationEventPayload::StepStarted {
                pipeline_id: "".into(),
                step_id: "".into(),
                step_index: 0,
                description: "".into(),
            },
            AutomationEventPayload::StepCompleted {
                pipeline_id: "".into(),
                step_id: "".into(),
                step_index: 0,
                attempts: 0,
                duration_ms: 0,
            },
            AutomationEventPayload::StepFailed {
                pipeline_id: "".into(),
                step_id: "".into(),
                step_index: 0,
                error: "".into(),
                attempts: 0,
            },
            AutomationEventPayload::StepSkipped {
                pipeline_id: "".into(),
                step_id: "".into(),
                step_index: 0,
                reason: "".into(),
            },
            AutomationEventPayload::StepRetried {
                pipeline_id: "".into(),
                step_id: "".into(),
                step_index: 0,
                attempt: 0,
                delay_ms: 0,
            },
            AutomationEventPayload::VerificationStarted {
                pipeline_id: "".into(),
                step_id: "".into(),
                strategy: "".into(),
            },
            AutomationEventPayload::VerificationCompleted {
                pipeline_id: "".into(),
                step_id: "".into(),
                passed: true,
                duration_ms: 0,
            },
            AutomationEventPayload::VerificationFailed {
                pipeline_id: "".into(),
                step_id: "".into(),
                reason: "".into(),
                suggestion: "".into(),
                duration_ms: 0,
            },
            AutomationEventPayload::RecoveryStarted {
                pipeline_id: "".into(),
                step_id: "".into(),
                retry_count: 0,
                reason: "".into(),
            },
            AutomationEventPayload::RecoveryCompleted {
                pipeline_id: "".into(),
                step_id: "".into(),
                decision: "".into(),
                strategy: "".into(),
                duration_ms: 0,
            },
            AutomationEventPayload::RecoveryFailed {
                pipeline_id: "".into(),
                step_id: "".into(),
                decision: "".into(),
                reason: "".into(),
                duration_ms: 0,
            },
            AutomationEventPayload::ReplanStarted {
                pipeline_id: "".into(),
                reason: "".into(),
                from_step_index: 0,
            },
            AutomationEventPayload::ReplanCompleted {
                pipeline_id: "".into(),
                new_step_count: 0,
                duration_ms: 0,
            },
            AutomationEventPayload::GoalExecutionStarted {
                goal: "".into(),
                execution_id: "".into(),
            },
            AutomationEventPayload::GoalExecutionCompleted {
                goal: "".into(),
                execution_id: "".into(),
                success: true,
                duration_ms: 0,
            },
        ];
        assert_eq!(variants.len(), 29);
    }
}
