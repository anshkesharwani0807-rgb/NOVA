use crate::automation::AutomationAction;
use nova_kernel::{log_activity, ErrorCategory, NovaError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsequenceLevel {
    Low,
    Medium,
    High,
}

pub struct ConsequenceGate;

impl ConsequenceGate {
    pub fn new() -> Self {
        Self
    }

    pub fn classify(&self, action: &AutomationAction) -> ConsequenceLevel {
        use AutomationAction::*;
        match action {
            FileManagement(op) => match op {
                crate::automation::FileOp::Delete { .. } => ConsequenceLevel::High,
                crate::automation::FileOp::Move { .. } => ConsequenceLevel::Medium,
                _ => ConsequenceLevel::Low,
            },
            SystemCommand(cmd) => match cmd {
                crate::automation::SystemCmd::Shutdown => ConsequenceLevel::High,
                crate::automation::SystemCmd::Sleep => ConsequenceLevel::Medium,
            },
            AppLaunch(_) => ConsequenceLevel::Low,
            Reminder(_) => ConsequenceLevel::Low,
        }
    }

    pub fn check(&self, action: &AutomationAction) -> Result<()> {
        let level = self.classify(action);
        let desc = format!("{:?}", action);

        match level {
            ConsequenceLevel::High => {
                log_activity(
                    "plugin_host",
                    "consent_gate_denied",
                    &format!(
                        "High-consequence action requires explicit confirmation: {}",
                        desc
                    ),
                    None,
                );
                tracing::warn!(action = ?action, "Consent required for irreversible action");
                Err(NovaError::new(
                    ErrorCategory::ConsentRequired,
                    "ERR_CONSENT_REQUIRED",
                    &format!(
                        "Irreversible action requires explicit confirmation: {}",
                        desc
                    ),
                ))
            }
            ConsequenceLevel::Medium => {
                log_activity(
                    "plugin_host",
                    "consent_gate_allowed",
                    &format!("Medium-consequence action allowed: {}", desc),
                    None,
                );
                tracing::info!(action = ?action, "Medium-consequence action allowed");
                Ok(())
            }
            ConsequenceLevel::Low => {
                log_activity(
                    "plugin_host",
                    "consent_gate_allowed",
                    &format!("Low-consequence action allowed: {}", desc),
                    None,
                );
                Ok(())
            }
        }
    }
}

impl Default for ConsequenceGate {
    fn default() -> Self {
        Self::new()
    }
}
