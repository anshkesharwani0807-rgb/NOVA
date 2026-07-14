//! Model / provider abstractions for the NOVA AI Runtime (Milestone 6).
//!
//! The runtime never hard-codes a model backend. Every concrete engine — local GGUF,
//! llama.cpp, Ollama, MLC, ONNX Runtime, Gemma/Phi/Qwen/DeepSeek, or a future consent-gated
//! cloud provider — is reached only through the [`InferenceProvider`] trait. Providers
//! stream their output as [`InferenceChunk`]s and must observe the [`Cancellation`] token,
//! so cancellation and streaming are uniform regardless of backend. Everything here works
//! offline-first: the bundled [`MockProvider`] produces deterministic output with no I/O.

use crate::tool::{ToolCall, ToolSpec};
use async_trait::async_trait;
use nova_kernel::Result;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Role of a message in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A single chat message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
    pub fn tool(content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
        }
    }
}

/// Provider-agnostic decoding parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceParams {
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub stop: Vec<String>,
}

impl Default for InferenceParams {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            temperature: 0.7,
            top_p: 0.95,
            stop: Vec::new(),
        }
    }
}

/// A fully-assembled request handed to a provider.
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub messages: Vec<Message>,
    pub params: InferenceParams,
    pub tools: Vec<ToolSpec>,
}

impl InferenceRequest {
    pub fn new(messages: Vec<Message>, params: InferenceParams) -> Self {
        Self {
            messages,
            params,
            tools: Vec::new(),
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolSpec>) -> Self {
        self.tools = tools;
        self
    }
}

/// Why a generation stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    Cancelled,
    Error,
}

/// One streamed unit of provider output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceChunk {
    /// A piece of generated text.
    Token(String),
    /// The model requested a tool invocation.
    ToolCall(ToolCall),
    /// Terminal marker with the reason generation ended.
    Done { finish_reason: FinishReason },
}

/// Metadata describing a model a provider can serve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDescriptor {
    pub id: String,
    pub provider: String,
    pub context_window: usize,
    pub local: bool,
    pub loaded: bool,
}

/// A cooperative cancellation token shared with a running inference.
#[derive(Clone, Default)]
pub struct Cancellation {
    flag: Arc<AtomicBool>,
}

impl Cancellation {
    pub fn new() -> Self {
        Self::default()
    }
    /// Request cancellation; providers observe this between chunks.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// Accumulates streamed chunks into a final result (used by the runtime).
#[derive(Default)]
pub struct Accumulator {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<FinishReason>,
}

impl Accumulator {
    fn record(&mut self, chunk: &InferenceChunk) {
        match chunk {
            InferenceChunk::Token(t) => self.text.push_str(t),
            InferenceChunk::ToolCall(c) => self.tool_calls.push(c.clone()),
            InferenceChunk::Done { finish_reason } => self.finish_reason = Some(*finish_reason),
        }
    }
}

/// The sink a provider emits chunks into. It forwards each chunk to the live stream and
/// also records it so the runtime can build a final outcome without re-reading the stream.
#[derive(Clone)]
pub struct ChunkSink {
    tx: mpsc::UnboundedSender<InferenceChunk>,
    acc: Arc<Mutex<Accumulator>>,
}

impl ChunkSink {
    pub fn new(tx: mpsc::UnboundedSender<InferenceChunk>, acc: Arc<Mutex<Accumulator>>) -> Self {
        Self { tx, acc }
    }

    /// Emit one chunk (records it, then forwards to the live receiver if still attached).
    pub fn emit(&self, chunk: InferenceChunk) {
        self.acc.lock().record(&chunk);
        let _ = self.tx.send(chunk);
    }
}

/// The backend abstraction. Implementors stream output and honour cancellation.
#[async_trait]
pub trait InferenceProvider: Send + Sync {
    /// Stable provider/model id.
    fn id(&self) -> &str;

    /// Describe the served model.
    fn describe(&self) -> ModelDescriptor;

    /// Lazily bring the model into memory. Cheap/no-op for providers without state.
    async fn load(&self) -> Result<()> {
        Ok(())
    }

    /// Release model resources.
    async fn unload(&self) -> Result<()> {
        Ok(())
    }

    fn is_loaded(&self) -> bool;

    /// Run inference, emitting chunks on `sink`. Must poll `cancel` and terminate with a
    /// [`InferenceChunk::Done`] marker.
    async fn infer(
        &self,
        req: &InferenceRequest,
        cancel: &Cancellation,
        sink: &ChunkSink,
    ) -> Result<()>;
}

/// A deterministic, fully-offline provider used as the default backend, for the demo, and
/// in tests. It echoes a summary of the assembled request and streams it token-by-token.
/// If tools are offered and the last user message contains the marker `[tool]`, it emits a
/// tool call instead — enough to exercise the tool-calling path end to end.
pub struct MockProvider {
    id: String,
    loaded: AtomicBool,
    per_token_delay: Duration,
}

impl MockProvider {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            loaded: AtomicBool::new(false),
            per_token_delay: Duration::from_millis(0),
        }
    }

    /// A variant that pauses between tokens so cancellation is observable in tests/demo.
    pub fn with_delay(id: impl Into<String>, delay_ms: u64) -> Self {
        Self {
            id: id.into(),
            loaded: AtomicBool::new(false),
            per_token_delay: Duration::from_millis(delay_ms),
        }
    }
}

#[async_trait]
impl InferenceProvider for MockProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn describe(&self) -> ModelDescriptor {
        ModelDescriptor {
            id: self.id.clone(),
            provider: "mock".to_string(),
            context_window: 8192,
            local: true,
            loaded: self.is_loaded(),
        }
    }

    async fn load(&self) -> Result<()> {
        self.loaded.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn unload(&self) -> Result<()> {
        self.loaded.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded.load(Ordering::SeqCst)
    }

    async fn infer(
        &self,
        req: &InferenceRequest,
        cancel: &Cancellation,
        sink: &ChunkSink,
    ) -> Result<()> {
        let last_user = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // Tool-calling path: request a tool the first time only. Once a tool result is
        // present in the conversation, answer with text (so the reasoning loop terminates).
        let has_tool_result = req.messages.iter().any(|m| m.role == Role::Tool);
        if !req.tools.is_empty() && last_user.contains("[tool]") && !has_tool_result {
            let spec = &req.tools[0];
            sink.emit(InferenceChunk::ToolCall(ToolCall {
                id: format!("call-{}", spec.name),
                name: spec.name.clone(),
                arguments: "{}".to_string(),
            }));
            sink.emit(InferenceChunk::Done {
                finish_reason: FinishReason::ToolCalls,
            });
            return Ok(());
        }

        let reply = format!(
            "NOVA (offline mock): received {} context message(s); responding to: \"{}\".",
            req.messages.len(),
            last_user.trim()
        );

        for word in reply.split_inclusive(' ') {
            if cancel.is_cancelled() {
                sink.emit(InferenceChunk::Done {
                    finish_reason: FinishReason::Cancelled,
                });
                return Ok(());
            }
            if !self.per_token_delay.is_zero() {
                tokio::time::sleep(self.per_token_delay).await;
            }
            sink.emit(InferenceChunk::Token(word.to_string()));
        }

        sink.emit(InferenceChunk::Done {
            finish_reason: FinishReason::Stop,
        });
        Ok(())
    }
}
