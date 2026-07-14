//! Tool-calling framework for the AI Runtime (Milestone 6).
//!
//! Tools are how the model acts on the user's world (memory, search, voice, calendar,
//! gallery, contacts, browser, plugins…). The runtime only knows the [`Tool`] trait and a
//! [`ToolRegistry`]; concrete tools are registered by their owning module or the
//! composition root, so no capability is hard-coded and new tools need no runtime changes.

use async_trait::async_trait;
use nova_kernel::{ErrorCategory, NovaError, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// A declarative description the model uses to decide when/how to call a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON-schema-style parameter description, kept as a JSON string (provider-agnostic).
    pub parameters: String,
}

impl ToolSpec {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: parameters.into(),
        }
    }
}

/// A model-issued request to invoke a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// JSON-encoded arguments.
    pub arguments: String,
}

/// The outcome of running a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub content: String,
    pub is_error: bool,
}

/// A callable tool. Implementations live outside the runtime.
#[async_trait]
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;

    /// Execute the tool with JSON `arguments`, returning textual content for the model.
    async fn invoke(&self, arguments: &str) -> Result<String>;
}

/// Thread-safe registry of available tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool. Fails if a tool with the same name already exists.
    pub fn register(&self, tool: Arc<dyn Tool>) -> Result<()> {
        let name = tool.spec().name;
        let mut tools = self.tools.write();
        if tools.contains_key(&name) {
            return Err(NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_TOOL_DUPLICATE",
                &format!("Tool '{name}' is already registered"),
            ));
        }
        tools.insert(name, tool);
        Ok(())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tools.read().contains_key(name)
    }

    pub fn count(&self) -> usize {
        self.tools.read().len()
    }

    /// Specs of all registered tools (for prompt/provider tool advertisement).
    pub fn specs(&self) -> Vec<ToolSpec> {
        let mut specs: Vec<ToolSpec> = self.tools.read().values().map(|t| t.spec()).collect();
        specs.sort_by(|a, b| a.name.cmp(&b.name)); // deterministic ordering
        specs
    }

    /// Invoke a tool by call. Errors are captured into the [`ToolResult`] (never panics),
    /// so a misbehaving tool degrades gracefully instead of failing the whole turn.
    pub async fn invoke(&self, call: &ToolCall) -> ToolResult {
        let tool = self.tools.read().get(&call.name).cloned();
        match tool {
            None => ToolResult {
                call_id: call.id.clone(),
                name: call.name.clone(),
                content: format!("no such tool: {}", call.name),
                is_error: true,
            },
            Some(tool) => match tool.invoke(&call.arguments).await {
                Ok(content) => ToolResult {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    content,
                    is_error: false,
                },
                Err(e) => ToolResult {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    content: format!("tool error: {e}"),
                    is_error: true,
                },
            },
        }
    }
}
