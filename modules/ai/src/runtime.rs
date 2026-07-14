//! The inference engine for the AI Runtime (Milestone 6).
//!
//! Orchestrates a single inference: it acquires a slot from the request queue (bounding
//! concurrent/background work), resolves + lazily loads the active model, streams chunks
//! back through an [`InferenceHandle`], honours cancellation, publishes lifecycle events,
//! and records latency. The [`InferenceEngine::reason`] method adds the multi-step tool
//! loop (reasoning pipeline). The engine holds no model itself — only the [`ModelManager`]
//! and [`ToolRegistry`] seams — so it stays backend-agnostic.

use crate::events;
use crate::model_manager::ModelManager;
use crate::provider::{
    Accumulator, Cancellation, ChunkSink, FinishReason, InferenceChunk, InferenceParams,
    InferenceRequest, Message,
};
use crate::tool::{ToolCall, ToolRegistry};
use crate::uncertainty::{UncertaintyConfig, UncertaintyScorer};
use nova_kernel::{ErrorCategory, Kernel, NovaError, Result};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinHandle;
use uuid::Uuid;

/// The aggregated result of one inference.
#[derive(Debug, Clone)]
pub struct InferenceOutcome {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
    pub latency_ms: u128,
    /// Estimated confidence in [0.0, 1.0]. Set by [`UncertaintyScorer`] after generation.
    pub confidence: f32,
    /// True when confidence is below threshold or the model explicitly said it doesn't know.
    pub uncertainty_flagged: bool,
}

/// A handle to a running inference: a live chunk stream plus cancellation and completion.
pub struct InferenceHandle {
    stream: mpsc::UnboundedReceiver<InferenceChunk>,
    cancel: Cancellation,
    task: JoinHandle<Result<InferenceOutcome>>,
}

impl InferenceHandle {
    /// Request cancellation of the running inference.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// A clone of the cancellation token (e.g. to cancel from elsewhere).
    pub fn cancellation(&self) -> Cancellation {
        self.cancel.clone()
    }

    /// Await the next streamed chunk, or `None` when the stream closes.
    pub async fn next_chunk(&mut self) -> Option<InferenceChunk> {
        self.stream.recv().await
    }

    /// Await final completion. The background task accumulates the full outcome, so this
    /// works whether or not the stream was consumed.
    pub async fn finish(self) -> Result<InferenceOutcome> {
        // Dropping the receiver is safe: the sender is unbounded and never blocks.
        drop(self.stream);
        match self.task.await {
            Ok(result) => result,
            Err(e) => Err(NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_TASK_JOIN",
                &format!("inference task failed to join: {e}"),
            )),
        }
    }
}

/// Runs inferences against the active model.
pub struct InferenceEngine {
    kernel: Arc<Kernel>,
    models: Arc<ModelManager>,
    tools: Arc<ToolRegistry>,
    queue: Arc<Semaphore>,
    /// Scores generated text for uncertainty and rewrites if below threshold (FR-AI-003).
    scorer: UncertaintyScorer,
}

impl InferenceEngine {
    /// Create an engine with a default request-queue depth (concurrent inferences).
    pub fn new(kernel: Arc<Kernel>, models: Arc<ModelManager>, tools: Arc<ToolRegistry>) -> Self {
        Self::with_concurrency(kernel, models, tools, 2)
    }

    pub fn with_concurrency(
        kernel: Arc<Kernel>,
        models: Arc<ModelManager>,
        tools: Arc<ToolRegistry>,
        max_concurrent: usize,
    ) -> Self {
        Self {
            kernel,
            models,
            tools,
            queue: Arc::new(Semaphore::new(max_concurrent.max(1))),
            scorer: UncertaintyScorer::default(),
        }
    }

    /// Replace the uncertainty scorer (e.g. to adjust threshold or disable).
    pub fn with_uncertainty_config(mut self, config: UncertaintyConfig) -> Self {
        self.scorer = UncertaintyScorer::new(config);
        self
    }

    pub fn tools(&self) -> &Arc<ToolRegistry> {
        &self.tools
    }

    /// Start an inference in the background, returning a handle for streaming/cancellation.
    /// The work is queued (bounded by the engine's concurrency) and runs on a spawned task.
    pub fn infer(&self, req: InferenceRequest, correlation_id: Uuid) -> InferenceHandle {
        let (tx, rx) = mpsc::unbounded_channel();
        let cancel = Cancellation::new();
        let acc = Arc::new(Mutex::new(Accumulator::default()));
        let sink = ChunkSink::new(tx, acc.clone());

        let kernel = self.kernel.clone();
        let models = self.models.clone();
        let queue = self.queue.clone();
        let cancel_task = cancel.clone();
        let scorer = self.scorer.clone();

        let task = tokio::spawn(async move {
            // Request queue: wait for a free slot before doing real work.
            let _permit = queue.acquire_owned().await.map_err(|e| {
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_QUEUE_CLOSED",
                    &format!("inference queue closed: {e}"),
                )
            })?;

            let start = Instant::now();
            events::publish(
                &kernel,
                events::AI_REQUEST_STARTED,
                correlation_id,
                format!("messages={}", req.messages.len()),
            );

            let provider = match models.active().await {
                Ok(p) => p,
                Err(e) => {
                    // Graceful failure: mark the stream done and emit a failure event.
                    sink.emit(InferenceChunk::Done {
                        finish_reason: FinishReason::Error,
                    });
                    events::publish(
                        &kernel,
                        events::AI_INFERENCE_FAILED,
                        correlation_id,
                        format!("no model: {e}"),
                    );
                    return Err(e);
                }
            };

            let result = provider.infer(&req, &cancel_task, &sink).await;
            let latency_ms = start.elapsed().as_millis();

            match result {
                Ok(()) => {
                    let acc = acc.lock();
                    let finish_reason = acc.finish_reason.unwrap_or(FinishReason::Stop);

                    // Apply uncertainty scoring (FR-AI-003): may rewrite text with a
                    // prefix when the model's response signals low confidence.
                    let uncertainty = scorer.apply(&acc.text);

                    let outcome = InferenceOutcome {
                        text: uncertainty.text,
                        tool_calls: acc.tool_calls.clone(),
                        finish_reason,
                        latency_ms,
                        confidence: uncertainty.confidence,
                        uncertainty_flagged: uncertainty.flagged,
                    };
                    events::publish(
                        &kernel,
                        events::AI_RESPONSE_GENERATED,
                        correlation_id,
                        format!(
                            "chars={} tool_calls={} reason={:?}",
                            outcome.text.len(),
                            outcome.tool_calls.len(),
                            outcome.finish_reason
                        ),
                    );
                    events::publish(
                        &kernel,
                        events::AI_REQUEST_FINISHED,
                        correlation_id,
                        format!("latency_ms={latency_ms}"),
                    );
                    Ok(outcome)
                }
                Err(e) => {
                    sink.emit(InferenceChunk::Done {
                        finish_reason: FinishReason::Error,
                    });
                    events::publish(
                        &kernel,
                        events::AI_INFERENCE_FAILED,
                        correlation_id,
                        format!("error={e}"),
                    );
                    Err(e)
                }
            }
        });

        InferenceHandle {
            stream: rx,
            cancel,
            task,
        }
    }

    /// Convenience: run to completion and return the outcome.
    pub async fn infer_collect(
        &self,
        req: InferenceRequest,
        correlation_id: Uuid,
    ) -> Result<InferenceOutcome> {
        self.infer(req, correlation_id).finish().await
    }

    /// The reasoning pipeline: infer, and if the model requests tools, run them, feed the
    /// results back, and infer again — up to `max_steps` rounds. Publishes `ToolInvoked`
    /// per tool. Returns the final outcome (tool-free, or the last round at the step cap).
    pub async fn reason(
        &self,
        mut messages: Vec<Message>,
        params: InferenceParams,
        correlation_id: Uuid,
        max_steps: usize,
    ) -> Result<InferenceOutcome> {
        let mut last: Option<InferenceOutcome> = None;
        for _ in 0..max_steps.max(1) {
            let req = InferenceRequest::new(messages.clone(), params.clone())
                .with_tools(self.tools.specs());
            let outcome = self.infer_collect(req, correlation_id).await?;

            if outcome.tool_calls.is_empty() {
                return Ok(outcome);
            }

            // Record the assistant's tool-call turn, then run each tool.
            let names: Vec<String> = outcome.tool_calls.iter().map(|c| c.name.clone()).collect();
            messages.push(Message::assistant(format!(
                "[tool_calls: {}]",
                names.join(", ")
            )));
            for call in &outcome.tool_calls {
                let result = self.tools.invoke(call).await;
                events::publish(
                    &self.kernel,
                    events::TOOL_INVOKED,
                    correlation_id,
                    format!("tool={} error={}", result.name, result.is_error),
                );
                messages.push(Message::tool(result.content));
            }
            last = Some(outcome);
        }

        last.ok_or_else(|| {
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_NO_OUTCOME",
                "reasoning pipeline produced no outcome",
            )
        })
    }
}
