//! Candle-based BERT embedding provider for local semantic embedding generation
//! (Milestone 6, FR-AI-002).
//!
//! Generates sentence embeddings entirely on-device using a BERT-family model
//! (e.g. `all-MiniLM-L6-v2`). No network required after model download.
//! Embeddings feed directly into the [`nova_search`] vector index.
//!
//! # How it works
//! 1. Load safetensors model weights + `config.json` + `tokenizer.json` from disk.
//! 2. Tokenize the input text.
//! 3. Run a BERT forward pass → sequence of hidden states [batch, seq, hidden].
//! 4. Mean-pool over the sequence dimension → [hidden] vector.
//! 5. L2-normalise → unit vector suitable for cosine similarity.
//!
//! # Model files
//! Place them in `.nova-runtime/models/embedder/` (gitignored). Any BERT-compatible
//! model in safetensors format works. Recommended: `all-MiniLM-L6-v2` (22M params,
//! 384-dim, fast on CPU).

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig, DTYPE};
use nova_kernel::{ErrorCategory, NovaError, Result};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

/// Embedding dimensionality produced by this provider.
/// Must match the hidden_size in the loaded model's config.json.
/// For all-MiniLM-L6-v2 this is 384.
pub const DEFAULT_EMBEDDING_DIM: usize = 384;

/// A loaded BERT model + tokenizer, held behind a Mutex.
struct LoadedEmbedder {
    model: BertModel,
    tokenizer: Tokenizer,
    hidden_size: usize,
}

/// On-device BERT-based sentence embedder (pure Rust, no native runtime DLL).
///
/// Produces normalised float vectors suitable for cosine-similarity search.
pub struct CandleEmbedder {
    /// Directory containing `model.safetensors`, `config.json`, `tokenizer.json`.
    model_dir: PathBuf,
    device: Device,
    inner: Arc<Mutex<Option<LoadedEmbedder>>>,
    loaded: AtomicBool,
}

impl CandleEmbedder {
    /// Create a new embedder pointing at `model_dir`.
    pub fn new(model_dir: impl Into<PathBuf>) -> Self {
        Self {
            model_dir: model_dir.into(),
            device: Device::Cpu,
            inner: Arc::new(Mutex::new(None)),
            loaded: AtomicBool::new(false),
        }
    }

    /// Lazily load model, config, and tokenizer from disk.
    pub fn load(&self) -> Result<()> {
        if self.loaded.load(Ordering::SeqCst) {
            return Ok(());
        }

        let weights_path = self.model_dir.join("model.safetensors");
        let config_path = self.model_dir.join("config.json");
        let tokenizer_path = self.model_dir.join("tokenizer.json");

        for (_label, path) in [
            ("model.safetensors", &weights_path),
            ("config.json", &config_path),
            ("tokenizer.json", &tokenizer_path),
        ] {
            if !path.exists() {
                return Err(NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_EMBEDDER_NOT_FOUND",
                    "embedder model file not found — place model files in the configured directory",
                ));
            }
        }

        tracing::info!("Loading BERT embedder from {:?}", self.model_dir);

        // Parse config.json.
        let config_bytes = std::fs::read(&config_path).map_err(|e| {
            tracing::error!("failed to read config.json: {e}");
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_EMBEDDER_CONFIG",
                "failed to read embedder config.json",
            )
        })?;
        let config: BertConfig = serde_json::from_slice(&config_bytes).map_err(|e| {
            tracing::error!("failed to parse config.json: {e}");
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_EMBEDDER_CONFIG",
                "failed to parse embedder config.json — ensure it is a valid BERT config",
            )
        })?;
        let hidden_size = config.hidden_size;

        // Load model weights via VarBuilder.
        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&[&weights_path], DTYPE, &self.device) }
                .map_err(|e| {
                tracing::error!("failed to mmap model weights: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_EMBEDDER_WEIGHTS",
                    "failed to load embedder model weights from safetensors",
                )
            })?;

        let model = BertModel::load(vb, &config).map_err(|e| {
            tracing::error!("failed to initialise BERT model: {e}");
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_EMBEDDER_INIT",
                "failed to initialise BERT model from weights",
            )
        })?;

        // Load tokenizer and configure padding/truncation for batch inference.
        let mut tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            tracing::error!("failed to load tokenizer: {e}");
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_EMBEDDER_TOKENIZER",
                "failed to load embedder tokenizer.json",
            )
        })?;

        // Pad to the longest sequence in the batch; truncate at 512.
        tokenizer
            .with_padding(Some(PaddingParams::default()))
            .with_truncation(Some(TruncationParams {
                max_length: 512,
                ..Default::default()
            }))
            .map_err(|e| {
                tracing::error!("failed to configure tokenizer: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_EMBEDDER_TOKENIZER",
                    "failed to configure tokenizer padding/truncation",
                )
            })?;

        *self.inner.lock() = Some(LoadedEmbedder {
            model,
            tokenizer,
            hidden_size,
        });
        self.loaded.store(true, Ordering::SeqCst);

        tracing::info!("BERT embedder loaded (hidden_size={})", hidden_size);
        Ok(())
    }

    /// Release model resources.
    pub fn unload(&self) {
        *self.inner.lock() = None;
        self.loaded.store(false, Ordering::SeqCst);
        tracing::info!("BERT embedder unloaded.");
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded.load(Ordering::SeqCst)
    }

    /// Embed a single text string. Returns a normalised float vector.
    ///
    /// Will auto-load if not already loaded.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        if !self.is_loaded() {
            self.load()?;
        }
        self.embed_batch(&[text])
            .map(|mut v| v.pop().unwrap_or_default())
    }

    /// Embed a batch of texts. Returns one normalised vector per input, in order.
    ///
    /// Batching amortises tokenizer and model overhead; use this when embedding
    /// multiple documents at once (e.g. on memory-capture events).
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        if !self.is_loaded() {
            self.load()?;
        }

        let guard = self.inner.lock();
        let loaded = guard.as_ref().ok_or_else(|| {
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_EMBEDDER_NOT_LOADED",
                "embedder not loaded",
            )
        })?;

        // Tokenize.
        let encodings = loaded
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| {
                tracing::error!("batch tokenization failed: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_EMBED_TOKENIZE",
                    "batch tokenization failed",
                )
            })?;

        let batch_size = encodings.len();
        let seq_len = encodings[0].get_ids().len();

        // Build input_ids [batch, seq].
        let input_ids_flat: Vec<u32> = encodings
            .iter()
            .flat_map(|e| e.get_ids().iter().copied())
            .collect();
        let input_ids = Tensor::from_vec(input_ids_flat, (batch_size, seq_len), &self.device)
            .map_err(|e| {
                tracing::error!("failed to create input_ids tensor: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_EMBED_TENSOR",
                    "failed to create input tensor",
                )
            })?;

        // Build token_type_ids [batch, seq] — all zeros for single-sentence tasks.
        let token_type_ids = Tensor::zeros((batch_size, seq_len), DType::U32, &self.device)
            .map_err(|e| {
                tracing::error!("failed to create token_type_ids: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_EMBED_TENSOR",
                    "failed to create token_type_ids tensor",
                )
            })?;

        // Build attention_mask [batch, seq].
        let attention_mask_flat: Vec<u32> = encodings
            .iter()
            .flat_map(|e| e.get_attention_mask().iter().copied())
            .collect();
        let attention_mask =
            Tensor::from_vec(attention_mask_flat, (batch_size, seq_len), &self.device).map_err(
                |e| {
                    tracing::error!("failed to create attention_mask: {e}");
                    NovaError::new(
                        ErrorCategory::Internal,
                        "ERR_AI_EMBED_TENSOR",
                        "failed to create attention_mask tensor",
                    )
                },
            )?;

        // Forward pass → [batch, seq, hidden].
        let sequence_output = loaded
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask))
            .map_err(|e| {
                tracing::error!("BERT forward pass failed: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_EMBED_FORWARD",
                    "BERT forward pass failed",
                )
            })?;

        // Mean-pool over the sequence dimension (masked — don't include padding tokens).
        // attention_mask shape: [batch, seq] → expand to [batch, seq, 1] for broadcasting.
        let mask_f32 = attention_mask
            .to_dtype(DType::F32)
            .and_then(|m| m.unsqueeze(2))
            .map_err(|e| {
                tracing::error!("mask conversion failed: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_EMBED_POOL",
                    "failed to convert attention mask for pooling",
                )
            })?;

        // Masked sum [batch, hidden], then divide by number of non-padding tokens.
        let masked = (sequence_output * &mask_f32).map_err(|e| {
            tracing::error!("masked multiplication failed: {e}");
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_EMBED_POOL",
                "failed to apply attention mask",
            )
        })?;

        let sum = masked.sum(1).map_err(|e| {
            tracing::error!("sum pooling failed: {e}");
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_EMBED_POOL",
                "failed to sum-pool embeddings",
            )
        })?; // [batch, hidden]

        let token_counts = mask_f32.sum(1).map_err(|e| {
            tracing::error!("token count sum failed: {e}");
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_EMBED_POOL",
                "failed to count non-padding tokens",
            )
        })?; // [batch, 1]

        let mean = (sum / token_counts).map_err(|e| {
            tracing::error!("mean pooling division failed: {e}");
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_EMBED_POOL",
                "mean pooling division failed",
            )
        })?; // [batch, hidden]

        // L2-normalise each embedding so cosine similarity == dot product.
        let norm = mean
            .sqr()
            .and_then(|s| s.sum(1))
            .and_then(|s| s.sqrt())
            .and_then(|n| n.unsqueeze(1))
            .map_err(|e| {
                tracing::error!("L2 norm failed: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_EMBED_NORM",
                    "L2 normalisation failed",
                )
            })?;

        let normalised = (mean / norm).map_err(|e| {
            tracing::error!("normalisation division failed: {e}");
            NovaError::new(
                ErrorCategory::Internal,
                "ERR_AI_EMBED_NORM",
                "embedding normalisation failed",
            )
        })?;

        // Extract to Vec<Vec<f32>>.
        let hidden_size = loaded.hidden_size;
        let flat: Vec<f32> = normalised
            .to_vec2::<f32>()
            .map_err(|e| {
                tracing::error!("failed to extract embedding to vec: {e}");
                NovaError::new(
                    ErrorCategory::Internal,
                    "ERR_AI_EMBED_EXTRACT",
                    "failed to extract embedding tensor to vector",
                )
            })?
            .into_iter()
            .flatten()
            .collect();

        // Chunk flat vec into per-document vectors.
        Ok(flat.chunks(hidden_size).map(|c| c.to_vec()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_kernel::ErrorCategory;
    use uuid::Uuid;

    #[test]
    fn default_embedding_dimension_is_384() {
        // all-MiniLM-L6-v2 hidden size; must match the loaded model config.
        assert_eq!(DEFAULT_EMBEDDING_DIM, 384);
    }

    #[test]
    fn embed_batch_of_empty_input_returns_empty() {
        let dir = std::env::temp_dir().join(format!("nova-embed-empty-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let emb = CandleEmbedder::new(dir);
        let out = emb.embed_batch(&[]).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn embed_fails_gracefully_when_model_files_absent() {
        // Offline-first: without weights the embedder must surface a typed error,
        // never panic or fabricate a vector.
        let dir = std::env::temp_dir().join(format!("nova-embed-missing-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let emb = CandleEmbedder::new(dir);
        let err = emb.embed("hello world").unwrap_err();
        assert_eq!(err.code, "ERR_AI_EMBEDDER_NOT_FOUND");
        assert_eq!(err.category, ErrorCategory::Internal);
        assert!(!emb.is_loaded());
    }

    #[test]
    fn embedder_load_is_idempotent() {
        // Loading twice (when files are present) is a no-op; without files it errors
        // both times rather than entering a broken loaded state.
        let dir = std::env::temp_dir().join(format!("nova-embed-idem-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let emb = CandleEmbedder::new(dir);
        assert!(emb.load().is_err());
        assert!(!emb.is_loaded());
        assert!(emb.load().is_err());
    }
}
