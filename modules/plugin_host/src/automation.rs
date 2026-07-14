use crate::consent_gate::ConsequenceGate;
use nova_kernel::{log_activity, Result};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum FileOp {
    Create { path: String },
    Delete { path: String },
    Move { source: String, dest: String },
}

#[derive(Debug, Clone)]
pub enum ReminderAction {
    Set { title: String, time: String },
}

#[derive(Debug, Clone)]
pub enum SystemCmd {
    Shutdown,
    Sleep,
}

#[derive(Debug, Clone)]
pub enum AutomationAction {
    FileManagement(FileOp),
    Reminder(ReminderAction),
    AppLaunch(String),
    SystemCommand(SystemCmd),
}

pub struct AutomationEngine {
    gate: Arc<ConsequenceGate>,
}

impl AutomationEngine {
    pub fn new(gate: Arc<ConsequenceGate>) -> Self {
        Self { gate }
    }

    pub fn execute(&self, action: AutomationAction) -> Result<String> {
        self.gate.check(&action)?;

        let desc = describe(&action);
        log_activity("plugin_host", "automation_execute", &desc, None);
        tracing::info!(action = ?action, "AutomationEngine executed action");

        Ok(format!("Executed: {}", desc))
    }
}

fn describe(action: &AutomationAction) -> String {
    match action {
        AutomationAction::FileManagement(op) => match op {
            FileOp::Create { path } => format!("create file {}", path),
            FileOp::Delete { path } => format!("delete file {}", path),
            FileOp::Move { source, dest } => format!("move {} to {}", source, dest),
        },
        AutomationAction::Reminder(act) => match act {
            ReminderAction::Set { title, .. } => format!("set reminder '{}'", title),
        },
        AutomationAction::AppLaunch(app) => format!("launch app {}", app),
        AutomationAction::SystemCommand(cmd) => match cmd {
            SystemCmd::Shutdown => "system shutdown".into(),
            SystemCmd::Sleep => "system sleep".into(),
        },
    }
}
