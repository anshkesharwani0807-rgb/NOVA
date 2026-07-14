//! The modular context system for the AI Runtime (Milestone 6).
//!
//! Context is assembled from independent [`ContextProvider`]s — memory, search, the current
//! task, system state, plugin results, and future sources (OCR, gallery, calendar,
//! contacts). The runtime depends on none of them directly; providers are supplied by their
//! owning module or the composition root, keeping the AI layer decoupled (BRAIN §3: no
//! `AI → Memory/Search` crate edge yet). Fragments are gathered, ranked, and rendered
//! deterministically into prompt context.

use async_trait::async_trait;
use nova_kernel::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// The origin/kind of a context fragment. Open-ended via [`ContextKind::Other`] so future
/// sources need no enum change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextKind {
    Memory,
    Search,
    Conversation,
    Task,
    SystemState,
    Plugin,
    Other(String),
}

impl ContextKind {
    pub fn label(&self) -> &str {
        match self {
            ContextKind::Memory => "memory",
            ContextKind::Search => "search",
            ContextKind::Conversation => "conversation",
            ContextKind::Task => "task",
            ContextKind::SystemState => "system",
            ContextKind::Plugin => "plugin",
            ContextKind::Other(s) => s,
        }
    }
}

/// A single unit of retrieved context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextFragment {
    pub kind: ContextKind,
    pub source: String,
    pub text: String,
    /// Relevance in [0, 1]; higher ranks earlier.
    pub score: f32,
}

impl ContextFragment {
    pub fn new(
        kind: ContextKind,
        source: impl Into<String>,
        text: impl Into<String>,
        score: f32,
    ) -> Self {
        Self {
            kind,
            source: source.into(),
            text: text.into(),
            score,
        }
    }
}

/// A modular source of context. Implemented outside the runtime.
#[async_trait]
pub trait ContextProvider: Send + Sync {
    fn name(&self) -> &str;

    /// Return up to `limit` fragments relevant to `query`.
    async fn provide(&self, query: &str, limit: usize) -> Result<Vec<ContextFragment>>;
}

/// The assembled context for one request.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BuiltContext {
    pub fragments: Vec<ContextFragment>,
}

impl BuiltContext {
    pub fn is_empty(&self) -> bool {
        self.fragments.is_empty()
    }

    pub fn len(&self) -> usize {
        self.fragments.len()
    }

    /// Render fragments into a deterministic, human-readable block for the prompt.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (i, f) in self.fragments.iter().enumerate() {
            out.push_str(&format!(
                "[{}] ({}/{}) {}\n",
                i + 1,
                f.kind.label(),
                f.source,
                f.text.trim()
            ));
        }
        out
    }
}

/// Aggregates registered providers into a single ranked context.
pub struct ContextBuilder {
    providers: RwLock<Vec<Arc<dyn ContextProvider>>>,
    per_provider_limit: usize,
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self {
            providers: RwLock::new(Vec::new()),
            per_provider_limit: 5,
        }
    }
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fix the per-provider fragment cap.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            providers: RwLock::new(Vec::new()),
            per_provider_limit: limit.max(1),
        }
    }

    /// Register a context provider (order-independent; results are ranked by score).
    pub fn add_provider(&self, provider: Arc<dyn ContextProvider>) {
        self.providers.write().push(provider);
    }

    pub fn provider_count(&self) -> usize {
        self.providers.read().len()
    }

    pub fn provider_names(&self) -> Vec<String> {
        self.providers
            .read()
            .iter()
            .map(|p| p.name().to_string())
            .collect()
    }

    /// Build ranked context for `query`. A failing provider is skipped (best-effort) rather
    /// than failing the whole request — context is an accelerant, never a precondition.
    pub async fn build(&self, query: &str) -> BuiltContext {
        let providers = self.providers.read().clone();
        let mut fragments: Vec<ContextFragment> = Vec::new();
        for p in providers {
            match p.provide(query, self.per_provider_limit).await {
                Ok(mut fs) => fragments.append(&mut fs),
                Err(e) => {
                    tracing::warn!("[ai] context provider '{}' failed: {}", p.name(), e);
                }
            }
        }
        fragments.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        BuiltContext { fragments }
    }
}
