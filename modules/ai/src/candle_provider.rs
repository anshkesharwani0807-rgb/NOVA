//! Candle-based GGUF model provider for local LLM inference (Milestone 6, FR-AI-001).
//!
//! Uses HuggingFace Candle framework (pure Rust, no C++ dependencies) to run quantized
//! GGUF models offline. Supports streaming and cancellation.
//!
//! # Model file
//! Place the GGUF model file anywhere on disk and pass the path to [`CandleProvider::new`].
//! Model files are gitignored and never committed (ADR-0007).
//!
//! # Feature gate
//! This provider compiles unconditionally but `load()` will return
//! `ERR_AI_MODEL_NOT_FOUND` if the model path does not exist, giving a clear error
//! rather than a build failure. Real inference only runs when a valid GGUF is present.

use crate::provider::{
    Cancellation, ChunkSink, FinishReason, InferenceChunk, InferenceProvider, InferenceRequest,
    Message, ModelDescriptor, Role,
};
use async_trait::async_trait;
use candle_core::{quantized::gguf_file, Device, Tensor};
use candle_transformers::{generation::LogitsProcessor, models::quantized_llama::ModelWeights};
use nova_kernel::{ErrorCategory, NovaError, Result};
use parking_lot::Mutex;
use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokenizers::Tokenizer;

// EOS token id for LLaMA-family models.
const EOS_TOKEN: u32 = 2;

/// A Candle-based provider that runs a quantized GGUF model entirely on-device.
///
/// Supports LLaMA, Mistral, Phi, and any other model exported in the GGUF format.
/// Inference runs on CPU by default; GPU is available via feature flags.
pub struct CandleProvider {
    id: String,
    model_path: PathBuf,
    tokenizer_path: PathBuf,
    device: Device,
    /// Model weights + tokenizer; populated after `load()`.
    inner: Arc<Mutex<Option<LoadedModel>>>,
    loaded: AtomicBool,
    context_window: usize,
}

struct LoadedModel {
    weights: ModelWeights,
    tokenizer: Tokenizer,
    logits_processor: LogitsProcessor,
}

impl CandleProvider {
    /// Create a new provider.
    ///
    /// * `id`             — Stable model identifier (used by `ModelManager`).
    /// * `model_path`     — Path to the `.gguf` file on disk.
    /// * `tokenizer_path` — Path to the `tokenizer.json` file.
    pub fn new(
        id: impl Into<String>,
        model_path: impl Into<PathBuf>,
        tokenizer_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            id: id.into(),
            model_path: model_path.into(),
            tokenizer_path: tokenizer_path.into(),
            device: Device::Cpu,
            inner: Arc::new(Mutex::new(None)),
            loaded: AtomicBool::new(false),
            context_window: 2048,
        }
    }

    /// Override the context window size (default: 2048).
    pub fn with_context_window(mut self, size: usize) -> Self {
        self.context_window = size;
        self
    }

    /// Format messages into a LLaMA-style prompt string.
    fn format_prompt(messages: &[Message]) -> String {
        let mut out = String::new();
        for msg in messages {
            match msg.role {
                Role::System => {
                    out.push_str(&format!("<<SYS>>\n{}\n<</SYS>>\n\n", msg.content));
                }
                Role::User => {
                    out.push_str(&format!("[INST] {} [/INST] ", msg.content));
                }
                Role::Assistant => {
                    out.push_str(&msg.content);
                    out.push(' ');
                }
                Role::Tool => {
                    out.push_str(&format!("[TOOL_RESULT] {} [/TOOL_RESULT]\n", msg.content));
                }
            }
        }
        out
    }
}

#[async_trait]
impl InferenceProvider for CandleProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn describe(&self) -> ModelDescriptor {
        ModelDescriptor {
            id: self.id.clone(),
            provider: "candle-gguf".to_string(),
            context_window: self.context_window,
            local: true,
            loaded: self.is_loaded(),
        }
    }

    async fn load(&self) -> Result<()> {
        if self.loaded.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Validate paths exist before attempting any I/O.
        if !self.model_path.exists() {
            return Err(NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_MODEL_NOT_FOUND",
                "GGUF model file not found; place the model file at the configured path",
            ));
        }
        if !self.tokenizer_path.exists() {
            return Err(NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_TOKENIZER_NOT_FOUND",
                "tokenizer.json not found; place the tokenizer file at the configured path",
            ));
        }

        tracing::info!("Loading GGUF model from {:?}", self.model_path);

        // Load the tokenizer.
        let tokenizer = Tokenizer::from_file(&self.tokenizer_path).map_err(|e| {
            let msg = format!("failed to load tokenizer: {e}");
            tracing::error!("{}", msg);
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_LOAD_TOKENIZER",
                "failed to load tokenizer — check tokenizer.json is valid",
            )
        })?;

        // Open and parse the GGUF file.
        let mut file = File::open(&self.model_path).map_err(|e| {
            let msg = format!("failed to open model file: {e}");
            tracing::error!("{}", msg);
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_LOAD_MODEL",
                "failed to open GGUF model file",
            )
        })?;

        let gguf_content = gguf_file::Content::read(&mut file).map_err(|e| {
            let msg = format!("failed to parse GGUF content: {e}");
            tracing::error!("{}", msg);
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_LOAD_MODEL",
                "failed to parse GGUF file — file may be corrupt or unsupported format",
            )
        })?;

        let weights =
            ModelWeights::from_gguf(gguf_content, &mut file, &self.device).map_err(|e| {
                let msg = format!("failed to load model weights: {e}");
                tracing::error!("{}", msg);
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_LOAD_MODEL",
                    "failed to load model weights from GGUF",
                )
            })?;

        // Greedy sampling by default (temperature = 0 → argmax).
        let logits_processor = LogitsProcessor::new(42, None, None);

        *self.inner.lock() = Some(LoadedModel {
            weights,
            tokenizer,
            logits_processor,
        });
        self.loaded.store(true, Ordering::SeqCst);

        tracing::info!("GGUF model loaded successfully: {}", self.id);
        Ok(())
    }

    async fn unload(&self) -> Result<()> {
        *self.inner.lock() = None;
        self.loaded.store(false, Ordering::SeqCst);
        tracing::info!("GGUF model unloaded: {}", self.id);
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
        if !self.is_loaded() {
            return Err(NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_MODEL_NOT_LOADED",
                "model not loaded; call load() first",
            ));
        }

        let prompt = Self::format_prompt(&req.messages);

        // Tokenize the prompt — requires the lock only briefly.
        let tokens: Vec<u32> = {
            let guard = self.inner.lock();
            let loaded = guard.as_ref().unwrap(); // safe: checked is_loaded above
            loaded
                .tokenizer
                .encode(prompt.as_str(), true)
                .map_err(|e| {
                    let msg = format!("tokenization failed: {e}");
                    tracing::error!("{}", msg);
                    NovaError::new(
                        ErrorCategory::Internal,
                        "ERR_AI_TOKENIZE",
                        "tokenization failed",
                    )
                })?
                .get_ids()
                .to_vec()
        };

        let max_tokens = req.params.max_tokens;
        let mut context = tokens.clone();

        // Auto-regressive decode loop. The model lock is held across the entire
        // generation to keep the KV-cache coherent; this is safe because inference
        // is already on a Tokio blocking thread via `infer()`.
        let mut guard = self.inner.lock();
        let loaded = guard.as_mut().unwrap();

        let mut generated = 0usize;

        loop {
            if cancel.is_cancelled() {
                sink.emit(InferenceChunk::Done {
                    finish_reason: FinishReason::Cancelled,
                });
                return Ok(());
            }

            if generated >= max_tokens {
                sink.emit(InferenceChunk::Done {
                    finish_reason: FinishReason::Length,
                });
                return Ok(());
            }

            // Build input tensor from context.
            let input = Tensor::new(context.as_slice(), &self.device).map_err(|e| {
                tracing::error!("tensor creation failed: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_TENSOR",
                    "failed to create input tensor",
                )
            })?;
            let input = input.unsqueeze(0).map_err(|e| {
                tracing::error!("unsqueeze failed: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_TENSOR",
                    "failed to shape tensor",
                )
            })?;

            // Forward pass.
            let logits = loaded.weights.forward(&input, generated).map_err(|e| {
                tracing::error!("forward pass failed: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_FORWARD",
                    "forward pass failed",
                )
            })?;

            // Extract last-token logits and sample.
            let logits = logits.squeeze(0).map_err(|e| {
                tracing::error!("squeeze failed: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_FORWARD",
                    "failed to extract logits",
                )
            })?;

            let next_token = loaded.logits_processor.sample(&logits).map_err(|e| {
                tracing::error!("sampling failed: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_SAMPLE",
                    "token sampling failed",
                )
            })?;

            // EOS → stop cleanly.
            if next_token == EOS_TOKEN {
                sink.emit(InferenceChunk::Done {
                    finish_reason: FinishReason::Stop,
                });
                return Ok(());
            }

            // Decode the token to text and stream it.
            let text = loaded.tokenizer.decode(&[next_token], false).map_err(|e| {
                tracing::error!("detokenization failed: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_DETOKENIZE",
                    "detokenization failed",
                )
            })?;

            sink.emit(InferenceChunk::Token(text));

            context = vec![next_token]; // only feed the new token each step
            generated += 1;
        }
    }
}
