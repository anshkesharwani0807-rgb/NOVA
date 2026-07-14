//! Comprehensive tests for the NOVA AI Runtime (Milestone 6): context builder, prompt
//! pipeline, model manager, inference engine, cancellation, streaming, tool invocation,
//! kernel lifecycle, health, and failure recovery.

use async_trait::async_trait;
use nova_ai::provider::{Cancellation, ChunkSink, InferenceRequest};
use nova_ai::{
    AIEngine, ContextBuilder, ContextFragment, ContextKind, ContextProvider, InferenceChunk,
    InferenceEngine, InferenceParams, InferenceProvider, Message, MockProvider, ModelManager,
    PromptPipeline, Tool, ToolCall, ToolRegistry, ToolSpec,
};
use nova_kernel::{Kernel, KernelModule};
use std::sync::Arc;
use uuid::Uuid;

// ── Test doubles ────────────────────────────────────────────────────────────

struct StaticContext {
    name: String,
    frags: Vec<ContextFragment>,
}

#[async_trait]
impl ContextProvider for StaticContext {
    fn name(&self) -> &str {
        &self.name
    }
    async fn provide(
        &self,
        _query: &str,
        limit: usize,
    ) -> nova_kernel::Result<Vec<ContextFragment>> {
        Ok(self.frags.iter().take(limit).cloned().collect())
    }
}

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new("echo", "Echoes its input", "{}")
    }
    async fn invoke(&self, arguments: &str) -> nova_kernel::Result<String> {
        Ok(format!("echoed:{arguments}"))
    }
}

/// A provider that always fails, to exercise failure recovery.
struct FailingProvider {
    loaded: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl InferenceProvider for FailingProvider {
    fn id(&self) -> &str {
        "failing"
    }
    fn describe(&self) -> nova_ai::ModelDescriptor {
        nova_ai::ModelDescriptor {
            id: "failing".into(),
            provider: "test".into(),
            context_window: 1,
            local: true,
            loaded: false,
        }
    }
    fn is_loaded(&self) -> bool {
        self.loaded.load(std::sync::atomic::Ordering::SeqCst)
    }
    async fn infer(
        &self,
        _req: &InferenceRequest,
        _cancel: &Cancellation,
        _sink: &ChunkSink,
    ) -> nova_kernel::Result<()> {
        Err(nova_kernel::NovaError::new(
            nova_kernel::ErrorCategory::Internal,
            "ERR_TEST_FAIL",
            "intentional failure",
        ))
    }
}

fn test_kernel() -> Arc<Kernel> {
    let base = std::env::temp_dir().join(format!("nova-ai-test-{}", Uuid::new_v4()));
    let config_dir = base.join("config");
    let log_dir = base.join("logs");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&log_dir).unwrap();
    // Kernel::bootstrap is a process-global singleton; ignore "already bootstrapped".
    Kernel::bootstrap(&config_dir, &log_dir).unwrap_or_else(|_| Kernel::instance().unwrap())
}

// ── Context builder ───────────────────────────────────────────────────────────

#[tokio::test]
async fn context_builder_ranks_and_renders() {
    let builder = ContextBuilder::new();
    builder.add_provider(Arc::new(StaticContext {
        name: "mem".into(),
        frags: vec![
            ContextFragment::new(ContextKind::Memory, "m1", "low", 0.2),
            ContextFragment::new(ContextKind::Memory, "m2", "high", 0.9),
        ],
    }));
    let ctx = builder.build("q").await;
    assert_eq!(ctx.len(), 2);
    // Highest score first.
    assert_eq!(ctx.fragments[0].text, "high");
    assert!(ctx.render().contains("memory/m2"));
}

#[tokio::test]
async fn context_builder_survives_provider_error() {
    struct Boom;
    #[async_trait]
    impl ContextProvider for Boom {
        fn name(&self) -> &str {
            "boom"
        }
        async fn provide(&self, _q: &str, _l: usize) -> nova_kernel::Result<Vec<ContextFragment>> {
            Err(nova_kernel::NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR",
                "boom",
            ))
        }
    }
    let builder = ContextBuilder::new();
    builder.add_provider(Arc::new(Boom));
    builder.add_provider(Arc::new(StaticContext {
        name: "ok".into(),
        frags: vec![ContextFragment::new(ContextKind::Search, "s", "hit", 0.5)],
    }));
    let ctx = builder.build("q").await;
    assert_eq!(ctx.len(), 1); // boom skipped, ok kept
}

// ── Prompt pipeline ───────────────────────────────────────────────────────────

#[tokio::test]
async fn prompt_pipeline_is_deterministic_and_ordered() {
    let pipe = PromptPipeline::new();
    let builder = ContextBuilder::new();
    builder.add_provider(Arc::new(StaticContext {
        name: "mem".into(),
        frags: vec![ContextFragment::new(ContextKind::Memory, "m", "ctx", 1.0)],
    }));
    let ctx = builder.build("q").await;
    let history = vec![Message::user("earlier"), Message::assistant("reply")];

    let a = pipe.assemble("hello", &ctx, &history);
    let b = pipe.assemble("hello", &ctx, &history);
    assert_eq!(a, b, "assembly must be deterministic");

    // system, context, 2 history, user = 5
    assert_eq!(a.len(), 5);
    assert_eq!(a[0].role, nova_ai::Role::System);
    assert_eq!(a[a.len() - 1].role, nova_ai::Role::User);
    assert_eq!(a[a.len() - 1].content, "hello");
}

// ── Model manager ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn model_manager_registers_activates_and_lazy_loads() {
    let mm = ModelManager::new();
    mm.register(Arc::new(MockProvider::new("a"))).unwrap();
    mm.register(Arc::new(MockProvider::new("b"))).unwrap();
    assert_eq!(mm.count(), 2);
    assert_eq!(mm.active_id().as_deref(), Some("a")); // first becomes active

    // Duplicate rejected.
    assert!(mm.register(Arc::new(MockProvider::new("a"))).is_err());

    // Lazy load on first resolve.
    let p = mm.active().await.unwrap();
    assert!(p.is_loaded());

    mm.set_active("b").unwrap();
    assert_eq!(mm.active_id().as_deref(), Some("b"));
    assert!(mm.set_active("ghost").is_err());
}

#[tokio::test]
async fn model_manager_errors_with_no_model() {
    let mm = ModelManager::new();
    assert!(mm.active().await.is_err());
}

// ── Model lifecycle management (FR-AI-005) ─────────────────────────────────────

#[tokio::test]
async fn lifecycle_load_unload_reload_tracks_state() {
    let mm = ModelManager::new();
    mm.register(Arc::new(MockProvider::new("lp"))).unwrap();

    // Starts unloaded (lazy).
    assert_eq!(mm.provider_state("lp"), Some(false));

    mm.load_provider("lp").await.unwrap();
    assert_eq!(mm.provider_state("lp"), Some(true));

    mm.unload_provider("lp").await.unwrap();
    assert_eq!(mm.provider_state("lp"), Some(false));

    // Reload restores the loaded state.
    mm.reload_provider("lp").await.unwrap();
    assert_eq!(mm.provider_state("lp"), Some(true));

    // Idempotent: loading an already-loaded model is a no-op, not an error.
    mm.load_provider("lp").await.unwrap();
    assert_eq!(mm.provider_state("lp"), Some(true));

    // Unknown ids are rejected with a typed error.
    assert!(mm.load_provider("ghost").await.is_err());
    assert_eq!(mm.provider_state("ghost"), None);
}

#[tokio::test]
async fn lifecycle_list_with_state_reflects_load() {
    let mm = ModelManager::new();
    mm.register(Arc::new(MockProvider::new("a"))).unwrap();
    mm.register(Arc::new(MockProvider::new("b"))).unwrap();

    // Nothing loaded yet.
    let states = mm.list_with_state();
    assert_eq!(states.len(), 2);
    assert!(states.iter().all(|(_d, loaded)| !loaded));

    mm.load_provider("b").await.unwrap();
    let states = mm.list_with_state();
    let b_loaded = states
        .iter()
        .find(|(d, _)| d.id == "b")
        .map(|(_, l)| *l)
        .unwrap();
    assert!(b_loaded, "model b should be loaded");
    let a_loaded = states
        .iter()
        .find(|(d, _)| d.id == "a")
        .map(|(_, l)| *l)
        .unwrap();
    assert!(!a_loaded, "model a should remain unloaded");
}

#[tokio::test]
async fn lifecycle_wait_for_state_succeeds() {
    let mm = Arc::new(ModelManager::new());
    mm.register(Arc::new(MockProvider::new("wp"))).unwrap();
    assert_eq!(mm.provider_state("wp"), Some(false));

    // A background task loads the model shortly; wait_for_provider_state must observe it.
    let mm2 = mm.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let _ = mm2.load_provider("wp").await;
    });

    mm.wait_for_provider_state("wp", true, 2000)
        .await
        .expect("model should become loaded within timeout");
    assert_eq!(mm.provider_state("wp"), Some(true));

    // And it can wait for unload too.
    let mm3 = mm.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let _ = mm3.unload_provider("wp").await;
    });
    mm.wait_for_provider_state("wp", false, 2000)
        .await
        .expect("model should become unloaded within timeout");
    assert_eq!(mm.provider_state("wp"), Some(false));
}

#[tokio::test]
async fn lifecycle_wait_for_state_times_out() {
    let mm = ModelManager::new();
    mm.register(Arc::new(MockProvider::new("never"))).unwrap();
    // Never loaded → waiting for loaded must time out, not hang.
    let err = mm
        .wait_for_provider_state("never", true, 50)
        .await
        .unwrap_err();
    assert_eq!(err.code, "ERR_AI_MODEL_LOAD_TIMEOUT");
}

// ── Inference engine: basic + streaming ───────────────────────────────────────

#[tokio::test]
async fn inference_engine_streams_and_completes() {
    let kernel = test_kernel();
    let models = Arc::new(ModelManager::new());
    models
        .register(Arc::new(MockProvider::new("mock")))
        .unwrap();
    let tools = Arc::new(ToolRegistry::new());
    let engine = InferenceEngine::new(kernel, models, tools);

    let req = InferenceRequest::new(vec![Message::user("ping")], InferenceParams::default());
    let mut handle = engine.infer(req, Uuid::new_v4());

    let mut tokens = 0;
    let mut saw_done = false;
    while let Some(chunk) = handle.next_chunk().await {
        match chunk {
            InferenceChunk::Token(_) => tokens += 1,
            InferenceChunk::Done { .. } => saw_done = true,
            _ => {}
        }
    }
    assert!(tokens > 0, "should stream tokens");
    assert!(saw_done, "should end with Done");

    let outcome = handle.finish().await.unwrap();
    assert!(outcome.text.contains("ping"));
    assert_eq!(outcome.finish_reason, nova_ai::FinishReason::Stop);
}

// ── Cancellation ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn inference_can_be_cancelled() {
    let kernel = test_kernel();
    let models = Arc::new(ModelManager::new());
    // Delay per token so cancellation lands mid-stream.
    models
        .register(Arc::new(MockProvider::with_delay("slow", 20)))
        .unwrap();
    let engine = InferenceEngine::new(kernel, models, Arc::new(ToolRegistry::new()));

    let req = InferenceRequest::new(
        vec![Message::user("a longer message to stream slowly")],
        InferenceParams::default(),
    );
    let handle = engine.infer(req, Uuid::new_v4());
    handle.cancel();
    let outcome = handle.finish().await.unwrap();
    assert_eq!(outcome.finish_reason, nova_ai::FinishReason::Cancelled);
}

// ── Tool invocation + reasoning pipeline ──────────────────────────────────────

#[tokio::test]
async fn tool_registry_invokes_and_handles_missing() {
    let reg = ToolRegistry::new();
    reg.register(Arc::new(EchoTool)).unwrap();
    assert!(reg.contains("echo"));
    assert!(reg.register(Arc::new(EchoTool)).is_err()); // duplicate

    let ok = reg
        .invoke(&ToolCall {
            id: "1".into(),
            name: "echo".into(),
            arguments: "hi".into(),
        })
        .await;
    assert!(!ok.is_error);
    assert_eq!(ok.content, "echoed:hi");

    let missing = reg
        .invoke(&ToolCall {
            id: "2".into(),
            name: "nope".into(),
            arguments: "".into(),
        })
        .await;
    assert!(missing.is_error);
}

#[tokio::test]
async fn reasoning_pipeline_runs_tool_then_finishes() {
    let kernel = test_kernel();
    let models = Arc::new(ModelManager::new());
    models
        .register(Arc::new(MockProvider::new("mock")))
        .unwrap();
    let tools = Arc::new(ToolRegistry::new());
    tools.register(Arc::new(EchoTool)).unwrap();
    let engine = InferenceEngine::new(kernel, models, tools);

    // First turn triggers a tool call ([tool] marker); second turn (tool result present)
    // finishes with text.
    let outcome = engine
        .reason(
            vec![Message::user("please [tool] this")],
            InferenceParams::default(),
            Uuid::new_v4(),
            4,
        )
        .await
        .unwrap();
    assert!(!outcome.text.is_empty());
    assert_eq!(outcome.finish_reason, nova_ai::FinishReason::Stop);
}

// ── Kernel lifecycle + health ─────────────────────────────────────────────────

#[tokio::test]
async fn aiengine_lifecycle_and_health() {
    let kernel = test_kernel();
    let ai = Arc::new(AIEngine::new(kernel.clone()));

    assert_eq!(ai.module_id(), "ai");
    assert_eq!(ai.health().status, nova_kernel::HealthStatus::Healthy);

    ai.initialize().await.unwrap();
    ai.start().await.unwrap();

    // End-to-end chat via the high-level API (offline mock).
    ai.add_context_provider(Arc::new(StaticContext {
        name: "mem".into(),
        frags: vec![ContextFragment::new(
            ContextKind::Memory,
            "m",
            "user likes tea",
            0.8,
        )],
    }));
    let reply = ai.chat("s1", "hello there").await.unwrap();
    assert!(reply.contains("hello there"));
    // Session recorded user + assistant.
    assert_eq!(ai.conversations().get_or_create("s1").len(), 2);

    ai.stop().await.unwrap();
    ai.shutdown().await.unwrap();
}

// ── Failure recovery ──────────────────────────────────────────────────────────

#[tokio::test]
async fn inference_recovers_gracefully_on_provider_error() {
    let kernel = test_kernel();
    let models = Arc::new(ModelManager::new());
    models
        .register(Arc::new(FailingProvider {
            loaded: std::sync::atomic::AtomicBool::new(true),
        }))
        .unwrap();
    let engine = InferenceEngine::new(kernel, models, Arc::new(ToolRegistry::new()));

    let req = InferenceRequest::new(vec![Message::user("x")], InferenceParams::default());
    let mut handle = engine.infer(req, Uuid::new_v4());

    // Stream still terminates (Done emitted) rather than hanging.
    let mut saw_done = false;
    while let Some(c) = handle.next_chunk().await {
        if matches!(c, InferenceChunk::Done { .. }) {
            saw_done = true;
        }
    }
    assert!(saw_done);
    // And the outcome surfaces the error (no panic).
    assert!(handle.finish().await.is_err());
}
