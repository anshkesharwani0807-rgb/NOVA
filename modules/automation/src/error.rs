use nova_kernel::{ErrorCategory, NovaError};

#[derive(Debug, Clone)]
pub enum AutomationError {
    WorkflowNotFound(String),
    WorkflowAlreadyExists(String),
    InvalidWorkflow(String),
    TriggerEvaluationFailed(String),
    ConditionNotMet(String),
    ActionExecutionFailed(String),
    ExecutionTimeout(String),
    ExecutionCancelled(String),
    ScheduleConflict(String),
    PermissionDenied(String),
    SerializationFailed(String),
    MissingDependency(String),
    WorkflowDisabled(String),
    StepExecutionFailed { step: usize, reason: String },
    ElementNotFound {
        query: String,
        suggestion: String,
    },
    StepTimeout {
        step: usize,
        timeout_ms: u64,
    },
    Internal(String),
}

impl std::fmt::Display for AutomationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutomationError::WorkflowNotFound(id) => write!(f, "workflow not found: {}", id),
            AutomationError::WorkflowAlreadyExists(id) => {
                write!(f, "workflow already exists: {}", id)
            }
            AutomationError::InvalidWorkflow(msg) => write!(f, "invalid workflow: {}", msg),
            AutomationError::TriggerEvaluationFailed(msg) => {
                write!(f, "trigger evaluation failed: {}", msg)
            }
            AutomationError::ConditionNotMet(msg) => write!(f, "condition not met: {}", msg),
            AutomationError::ActionExecutionFailed(msg) => {
                write!(f, "action execution failed: {}", msg)
            }
            AutomationError::ExecutionTimeout(msg) => write!(f, "execution timeout: {}", msg),
            AutomationError::ExecutionCancelled(msg) => write!(f, "execution cancelled: {}", msg),
            AutomationError::ScheduleConflict(msg) => write!(f, "schedule conflict: {}", msg),
            AutomationError::PermissionDenied(msg) => write!(f, "permission denied: {}", msg),
            AutomationError::SerializationFailed(msg) => write!(f, "serialization failed: {}", msg),
            AutomationError::MissingDependency(msg) => write!(f, "missing dependency: {}", msg),
            AutomationError::WorkflowDisabled(id) => write!(f, "workflow disabled: {}", id),
            AutomationError::StepExecutionFailed { step, reason } => {
                write!(f, "step {} failed: {}", step, reason)
            }
            AutomationError::ElementNotFound { query, suggestion } => {
                write!(f, "element '{}' not found on screen. {}", query, suggestion)
            }
            AutomationError::StepTimeout { step, timeout_ms } => {
                write!(f, "step {} timed out after {}ms", step, timeout_ms)
            }
            AutomationError::Internal(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

impl std::error::Error for AutomationError {}

impl From<AutomationError> for NovaError {
    fn from(e: AutomationError) -> Self {
        let category = match &e {
            AutomationError::WorkflowNotFound(_) => ErrorCategory::Kernel,
            AutomationError::WorkflowAlreadyExists(_) => ErrorCategory::ConfigInvalid,
            AutomationError::InvalidWorkflow(_) => ErrorCategory::ConfigInvalid,
            AutomationError::TriggerEvaluationFailed(_) => ErrorCategory::Inference,
            AutomationError::ConditionNotMet(_) => ErrorCategory::Kernel,
            AutomationError::ActionExecutionFailed(_) => ErrorCategory::Inference,
            AutomationError::ExecutionTimeout(_) => ErrorCategory::Internal,
            AutomationError::ExecutionCancelled(_) => ErrorCategory::Kernel,
            AutomationError::ScheduleConflict(_) => ErrorCategory::ConfigInvalid,
            AutomationError::PermissionDenied(_) => ErrorCategory::EgressDenied,
            AutomationError::SerializationFailed(_) => ErrorCategory::Storage,
            AutomationError::MissingDependency(_) => ErrorCategory::Kernel,
            AutomationError::WorkflowDisabled(_) => ErrorCategory::Kernel,
            AutomationError::StepExecutionFailed { .. } => ErrorCategory::Inference,
            AutomationError::ElementNotFound { .. } => ErrorCategory::Inference,
            AutomationError::StepTimeout { .. } => ErrorCategory::Internal,
            AutomationError::Internal(_) => ErrorCategory::Internal,
        };
        NovaError::new(category, "ERR_AUTOMATION", &e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let e = AutomationError::WorkflowNotFound("test".into());
        assert_eq!(e.to_string(), "workflow not found: test");
    }

    #[test]
    fn test_error_conversion_to_nova() {
        let e = AutomationError::WorkflowNotFound("w1".into());
        let ne: nova_kernel::NovaError = e.into();
        assert!(ne.message.contains("w1"));
    }

    #[test]
    fn test_all_error_variants() {
        let variants: Vec<AutomationError> = vec![
            AutomationError::WorkflowNotFound("a".into()),
            AutomationError::WorkflowAlreadyExists("a".into()),
            AutomationError::InvalidWorkflow("a".into()),
            AutomationError::TriggerEvaluationFailed("a".into()),
            AutomationError::ConditionNotMet("a".into()),
            AutomationError::ActionExecutionFailed("a".into()),
            AutomationError::ExecutionTimeout("a".into()),
            AutomationError::ExecutionCancelled("a".into()),
            AutomationError::ScheduleConflict("a".into()),
            AutomationError::PermissionDenied("a".into()),
            AutomationError::SerializationFailed("a".into()),
            AutomationError::MissingDependency("a".into()),
            AutomationError::WorkflowDisabled("a".into()),
            AutomationError::StepExecutionFailed {
                step: 0,
                reason: "a".into(),
            },
            AutomationError::ElementNotFound {
                query: "btn".into(),
                suggestion: "try a different query".into(),
            },
            AutomationError::StepTimeout {
                step: 0,
                timeout_ms: 5000,
            },
            AutomationError::Internal("a".into()),
        ];
        assert_eq!(variants.len(), 17);
        for v in &variants {
            let s = v.to_string();
            assert!(!s.is_empty());
        }
    }
}
