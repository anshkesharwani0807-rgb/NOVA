use async_trait::async_trait;

use crate::error::InputResult;
use crate::traits::InputEngine;
use crate::types::*;
use parking_lot::RwLock;

pub struct MockInputProvider {
    executed: RwLock<Vec<InputAction>>,
}

impl MockInputProvider {
    pub fn new() -> Self {
        Self {
            executed: RwLock::new(Vec::new()),
        }
    }

    pub fn executed_actions(&self) -> Vec<InputAction> {
        self.executed.read().clone()
    }

    pub fn reset(&self) {
        self.executed.write().clear();
    }
}

impl Default for MockInputProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InputEngine for MockInputProvider {
    fn engine_name(&self) -> &'static str {
        "mock-input"
    }

    async fn execute(&self, action: &InputAction) -> InputResult<ActionResult> {
        self.executed.write().push(action.clone());
        tracing::info!("MockInputProvider executing {}", action.label());
        Ok(ActionResult::success(format!("executed {} (mock)", action.label())))
    }
}
