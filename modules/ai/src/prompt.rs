//! Deterministic prompt assembly for the AI Runtime (Milestone 6).
//!
//! The pipeline turns a system prompt, safety instructions, retrieved context, conversation
//! history, and the user's message into an ordered [`Message`] list. Assembly is fully
//! deterministic (no time, randomness, or map iteration order) so the same inputs always
//! yield the same prompt — important for reproducibility and testing.

use crate::context::BuiltContext;
use crate::provider::Message;

/// Default NOVA system prompt (offline-first, privacy-first persona).
pub const DEFAULT_SYSTEM_PROMPT: &str =
    "You are NOVA, a private, on-device personal AI that belongs entirely to its user. \
You are helpful, concise, and honest about the limits of what you know.";

/// Default safety instructions appended to the system prompt.
pub const SAFETY_INSTRUCTIONS: &str = "Respect the user's privacy and sovereignty. \
Never fabricate facts; if unsure, say so. Only use provided context and tools.";

/// Assembles prompts deterministically.
#[derive(Debug, Clone)]
pub struct PromptPipeline {
    system_prompt: String,
    safety: String,
}

impl Default for PromptPipeline {
    fn default() -> Self {
        Self {
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            safety: SAFETY_INSTRUCTIONS.to_string(),
        }
    }
}

impl PromptPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    pub fn with_safety(mut self, safety: impl Into<String>) -> Self {
        self.safety = safety.into();
        self
    }

    /// Assemble the final message list: `[system(+safety)] [context] [history…] [user]`.
    pub fn assemble(
        &self,
        user: &str,
        context: &BuiltContext,
        history: &[Message],
    ) -> Vec<Message> {
        let mut out: Vec<Message> = Vec::with_capacity(history.len() + 3);

        let mut system = self.system_prompt.clone();
        if !self.safety.is_empty() {
            system.push_str("\n\n");
            system.push_str(&self.safety);
        }
        out.push(Message::system(system));

        if !context.is_empty() {
            out.push(Message::system(format!(
                "Relevant context:\n{}",
                context.render()
            )));
        }

        out.extend_from_slice(history);
        out.push(Message::user(user));
        out
    }
}
