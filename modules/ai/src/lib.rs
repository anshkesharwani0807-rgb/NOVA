//! # NOVA AI Runtime (`nova_ai`, Milestone 6)
//!
//! The intelligence layer used by every future module. It is **backend-agnostic** (models
//! are reached only through [`provider::InferenceProvider`]), **offline-first** (a
//! deterministic mock provider is registered by default), and **decoupled** — it depends on
//! no other module crate. Memory, Search and future sources contribute through the
//! [`context::ContextProvider`] and [`tool::Tool`] seams, wired by the composition root, so
//! the AI layer never couples to Memory/Search directly (BRAIN §3, keeps the graph acyclic).
//!
//! Pieces: [`provider`] (model abstraction, streaming, cancellation), [`model_manager`]
//! (lazy loading), [`context`] (modular context builder), [`prompt`] (deterministic prompt
//! assembly), [`session`] (conversation history), [`tool`] (tool-calling framework),
//! [`runtime`] (the inference engine: request queue, background inference, reasoning loop,
//! events), and [`events`] (event-bus + activity-trail narration).

pub mod candle_provider;
pub mod context;
pub mod embedder;
pub mod events;
pub mod model_manager;
pub mod prompt;
pub mod provider;
pub mod remote_provider;
pub mod runtime;
pub mod session;
pub mod tool;
pub mod uncertainty;

pub use candle_provider::CandleProvider;
pub use context::{BuiltContext, ContextBuilder, ContextFragment, ContextKind, ContextProvider};
pub use embedder::{CandleEmbedder, DEFAULT_EMBEDDING_DIM};
pub use model_manager::ModelManager;
pub use prompt::PromptPipeline;
pub use provider::{
    Cancellation, FinishReason, InferenceChunk, InferenceParams, InferenceProvider,
    InferenceRequest, Message, MockProvider, ModelDescriptor, Role,
};
pub use remote_provider::RemoteProvider;
pub use runtime::{InferenceEngine, InferenceHandle, InferenceOutcome};
pub use session::{ConversationManager, Session};
pub use tool::{Tool, ToolCall, ToolRegistry, ToolResult, ToolSpec};
pub use uncertainty::{UncertaintyConfig, UncertaintyResult, UncertaintyScorer};

use async_trait::async_trait;
use nova_kernel::{
    EventMetadata, HealthStatus, Kernel, KernelModule, ModuleHealth, NovaResponse, Result,
};
use std::sync::Arc;
use uuid::Uuid;

/// The default session id used by the `ai:inference` request handler.
pub const DEFAULT_SESSION: &str = "default";

/// The AI Runtime module. Owns the model manager, tool registry, context builder,
/// conversation manager and inference engine, and plugs into the kernel as `KernelModule`.
#[derive(Clone)]
pub struct AIEngine {
    kernel: Arc<Kernel>,
    models: Arc<ModelManager>,
    tools: Arc<ToolRegistry>,
    conversations: Arc<ConversationManager>,
    context: Arc<ContextBuilder>,
    prompt: PromptPipeline,
    engine: Arc<InferenceEngine>,
}

impl AIEngine {
    /// Construct the runtime bound to the kernel. A deterministic offline [`MockProvider`]
    /// is registered as the default model so the runtime works out of the box.
    pub fn new(kernel: Arc<Kernel>) -> Self {
        let models = Arc::new(ModelManager::new());
        // Default offline model; real providers are registered by the composition root.
        let _ = models.register(Arc::new(MockProvider::new("mock-local")));

        let tools = Arc::new(ToolRegistry::new());
        let conversations = Arc::new(ConversationManager::default());
        let context = Arc::new(ContextBuilder::new());
        let engine = Arc::new(InferenceEngine::new(
            kernel.clone(),
            models.clone(),
            tools.clone(),
        ));

        Self {
            kernel,
            models,
            tools,
            conversations,
            context,
            prompt: PromptPipeline::new(),
            engine,
        }
    }

    // ── Accessors / wiring (used by the composition root) ──────────────────────

    pub fn models(&self) -> &Arc<ModelManager> {
        &self.models
    }
    pub fn tools(&self) -> &Arc<ToolRegistry> {
        &self.tools
    }
    pub fn conversations(&self) -> &Arc<ConversationManager> {
        &self.conversations
    }
    pub fn context(&self) -> &Arc<ContextBuilder> {
        &self.context
    }
    pub fn engine(&self) -> &Arc<InferenceEngine> {
        &self.engine
    }

    /// Register an additional model backend (GGUF/Ollama/ONNX/cloud/…).
    pub fn register_provider(&self, provider: Arc<dyn InferenceProvider>) -> Result<()> {
        self.models.register(provider)
    }

    /// Register a tool the model may call.
    pub fn register_tool(&self, tool: Arc<dyn Tool>) -> Result<()> {
        self.tools.register(tool)
    }

    /// Register a modular context source (memory, search, …).
    pub fn add_context_provider(&self, provider: Arc<dyn ContextProvider>) {
        self.context.add_provider(provider);
    }

    // ── High-level inference ───────────────────────────────────────────────────

    /// Build context, assemble the prompt (with the session's history), and start a
    /// streaming inference. The user turn is recorded in the session; the caller records
    /// the assistant turn (or use [`AIEngine::chat`]).
    pub async fn complete(&self, session_id: &str, user: &str) -> Result<InferenceHandle> {
        let correlation_id = Uuid::new_v4();
        let session = self.conversations.get_or_create(session_id);

        let context = self.context.build(user).await;
        events::publish(
            &self.kernel,
            events::CONTEXT_BUILT,
            correlation_id,
            format!(
                "fragments={} providers={}",
                context.len(),
                self.context.provider_count()
            ),
        );

        let history = session.history();
        let messages = self.prompt.assemble(user, &context, &history);
        session.push(Message::user(user));

        let req = InferenceRequest::new(messages, InferenceParams::default())
            .with_tools(self.tools.specs());
        Ok(self.engine.infer(req, correlation_id))
    }

    /// Run a full turn to completion, recording both the user and assistant messages.
    pub async fn chat(&self, session_id: &str, user: &str) -> Result<String> {
        let handle = self.complete(session_id, user).await?;
        let outcome = handle.finish().await?;
        self.conversations
            .get_or_create(session_id)
            .push(Message::assistant(outcome.text.clone()));
        Ok(outcome.text)
    }
}

#[async_trait]
impl KernelModule for AIEngine {
    fn module_id(&self) -> &'static str {
        "ai"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn initialize(&self) -> Result<()> {
        tracing::info!(
            "AIEngine initialized ({} model provider(s), active={:?}).",
            self.models.count(),
            self.models.active_id()
        );
        Ok(())
    }

    /// Start the `ai:inference` request handler. It accepts a `String` prompt and returns a
    /// `String` response produced by the (offline mock) runtime — a real, streaming-capable
    /// inference behind a simple request/response seam.
    async fn start(&self) -> Result<()> {
        let mut rx = self
            .kernel
            .event_bus
            .register_request_handler("ai:inference", 64)?;

        let this = self.clone();
        tokio::spawn(async move {
            tracing::info!("AIEngine request handler started.");
            while let Some(req) = rx.recv().await {
                let res_meta = EventMetadata::child_of(
                    &req.metadata,
                    "AIEngine",
                    Some("inference_response".to_string()),
                );

                let prompt = req
                    .payload
                    .downcast_ref::<String>()
                    .cloned()
                    .unwrap_or_default();

                let reply = match this.chat(DEFAULT_SESSION, &prompt).await {
                    Ok(text) => text,
                    Err(e) => format!("inference error: {e}"),
                };

                let payload: Arc<String> = Arc::new(reply);
                let _ = req.response_tx.send(Ok(NovaResponse {
                    metadata: res_meta,
                    payload,
                }));
            }
            tracing::warn!("AIEngine request handler stopped (channel closed).");
        });

        tracing::info!("AIEngine started (inference handler ready).");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        tracing::info!("AIEngine stopping.");
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        // Release any loaded model resources.
        if let Ok(provider) = self.models.active().await {
            let _ = provider.unload().await;
        }
        tracing::info!("AIEngine shut down.");
        Ok(())
    }

    fn health(&self) -> ModuleHealth {
        if self.models.count() == 0 {
            return ModuleHealth::unhealthy("no model provider registered");
        }
        ModuleHealth {
            status: HealthStatus::Healthy,
            detail: format!(
                "{} model(s), active={}, {} tool(s), {} context provider(s)",
                self.models.count(),
                self.models
                    .active_id()
                    .unwrap_or_else(|| "none".to_string()),
                self.tools.count(),
                self.context.provider_count()
            ),
        }
    }
}
