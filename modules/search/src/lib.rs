use async_trait::async_trait;
use nova_kernel::{EventMetadata, Kernel, KernelModule, NovaEvent, NovaResponse, Result};
use nova_memory::{MemoryEvent, MemoryEventKind, MemoryRecord};
use std::sync::Arc;
use tokio::sync::broadcast;

pub mod document;
pub mod engine;
pub mod parser;
pub mod vector;

pub use document::{Combine, IndexDocument, MatchMode, SearchQuery, SearchResult};
pub use engine::{IndexStats, SearchEngine, SCHEMA_VERSION};

/// The Universal Search module — a KernelModule that owns a SearchEngine and
/// subscribes to MemoryEngine events to keep the index in sync automatically.
pub struct UniversalSearch {
    kernel: Arc<Kernel>,
    engine: Arc<parking_lot::Mutex<Option<SearchEngine>>>,
    /// Receiver for memory change events from the event bus.
    event_rx: Arc<parking_lot::Mutex<Option<broadcast::Receiver<NovaEvent>>>>,
}

impl UniversalSearch {
    /// Construct a new UniversalSearch module bound to the kernel.
    pub fn new(kernel: Arc<Kernel>) -> Self {
        Self {
            kernel,
            engine: Arc::new(parking_lot::Mutex::new(None)),
            event_rx: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    /// Get a reference to the search engine (initializes if needed).
    fn get_engine(&self) -> Result<SearchEngine> {
        let mut guard = self.engine.lock();
        if guard.is_none() {
            let cfg = nova_kernel::get_config();
            let db_path = self.kernel.config_dir.join(&cfg.memory.db_path);
            let search_path = db_path.with_file_name("search.db");
            *guard = Some(SearchEngine::open(&search_path)?);
        }
        Ok(guard.as_ref().unwrap().clone())
    }

    /// Initialize the event subscription to memory change events.
    fn subscribe_to_memory_events(&self) -> Result<()> {
        let rx = self.kernel.event_bus.subscribe();
        *self.event_rx.lock() = Some(rx);
        Ok(())
    }

    /// Process a single memory event and update the search index accordingly.
    fn handle_memory_event(&self, event: &MemoryEvent) -> Result<()> {
        let engine = self.get_engine()?;
        match event.kind {
            MemoryEventKind::Created | MemoryEventKind::Updated => {
                if let Some(record) = &event.record {
                    let doc = IndexDocument::from_memory(record);
                    engine.insert(&doc)?;
                }
            }
            MemoryEventKind::Deleted => {
                engine.delete("memory", &event.record_id)?;
            }
        }
        Ok(())
    }

    /// Run the event listener loop (spawned as a background task).
    async fn run_event_listener(&self) {
        let mut rx = {
            let mut guard = self.event_rx.lock();
            guard.take().expect("event listener not initialized")
        };

        tracing::info!("UniversalSearch event listener started.");
        while let Ok(event) = rx.recv().await {
            // Only process events originating from the memory module
            if event.metadata.origin_module != "memory" {
                continue;
            }

            // Downcast the payload to MemoryEvent
            if let Some(mem_event) = event.payload.downcast_ref::<MemoryEvent>() {
                if let Err(e) = self.handle_memory_event(mem_event) {
                    tracing::error!("[UniversalSearch] Failed to handle memory event: {}", e);
                    nova_kernel::log_activity(
                        "search",
                        "search.event_failed",
                        &format!("event={:?} error={}", mem_event.kind, e),
                        Some(event.metadata.correlation_id),
                    );
                }
            }
        }
        tracing::warn!("UniversalSearch event listener stopped (channel closed).");
    }

    // ─── Public Search API ────────────────────────────────────────────────

    /// Index a memory record (insert or update).
    pub fn index_memory(&self, record: &MemoryRecord) -> Result<()> {
        let engine = self.get_engine()?;
        let doc = IndexDocument::from_memory(record);
        engine.insert(&doc)
    }

    /// Remove a memory record from the index.
    pub fn remove_memory(&self, record_id: &str) -> Result<()> {
        let engine = self.get_engine()?;
        engine.delete("memory", record_id)
    }

    /// Search the index with a full SearchQuery.
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let engine = self.get_engine()?;
        engine.search(query)
    }

    /// Convenience: search by text with default options.
    pub fn search_text(&self, text: &str, limit: Option<usize>) -> Result<Vec<SearchResult>> {
        let query = SearchQuery::partial(text).limit(limit.unwrap_or(50));
        self.search(&query)
    }

    /// Convenience: run a natural-language query string (supports `tag:`, `#tag`,
    /// `source:`, `category:` filters and `"quoted phrases"`).
    pub fn search_nl(&self, input: &str, limit: Option<usize>) -> Result<Vec<SearchResult>> {
        let mut query = SearchQuery::parse(input);
        query.limit = Some(limit.unwrap_or(50));
        self.search(&query)
    }

    /// Convenience: search by tag.
    pub fn search_by_tag(&self, tag: &str, limit: Option<usize>) -> Result<Vec<SearchResult>> {
        let query = SearchQuery::new().tag(tag).limit(limit.unwrap_or(50));
        self.search(&query)
    }

    /// Convenience: search by date range.
    pub fn search_by_date(
        &self,
        from: Option<i64>,
        to: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<SearchResult>> {
        let query = SearchQuery::new()
            .date_range(from, to)
            .limit(limit.unwrap_or(50));
        self.search(&query)
    }

    /// Rebuild the entire index from a slice of memory records.
    pub fn rebuild(&self, records: &[MemoryRecord]) -> Result<usize> {
        let mut engine = self.get_engine()?;
        let docs: Vec<IndexDocument> = records.iter().map(IndexDocument::from_memory).collect();
        engine.rebuild(&docs)
    }

    /// Clear the entire index.
    pub fn clear(&self) -> Result<()> {
        let engine = self.get_engine()?;
        engine.clear()
    }

    /// Get index statistics.
    pub fn stats(&self) -> Result<IndexStats> {
        let engine = self.get_engine()?;
        engine.stats()
    }

    /// Health check for the search module.
    pub fn health(&self) -> Result<IndexStats> {
        self.stats()
    }
}

#[async_trait]
impl KernelModule for UniversalSearch {
    fn module_id(&self) -> &'static str {
        "search"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn initialize(&self) -> Result<()> {
        // Initialize the search engine (opens the database)
        self.get_engine()?;
        // Subscribe to memory events
        self.subscribe_to_memory_events()?;
        tracing::info!("UniversalSearch initialized (search index ready).");
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        // Start the request handler for search:query
        let mut rx = self
            .kernel
            .event_bus
            .register_request_handler("search:query", 64)?;

        // Spawn the event listener for memory changes
        let search_self = self.clone();
        tokio::spawn(async move {
            search_self.run_event_listener().await;
        });

        // Spawn the request handler
        let search_self2 = self.clone();
        tokio::spawn(async move {
            tracing::info!("UniversalSearch request handler started.");
            while let Some(req) = rx.recv().await {
                tracing::debug!(
                    "[UniversalSearch] Processing query request: {:?}",
                    req.metadata
                );

                let res_meta = EventMetadata::child_of(
                    &req.metadata,
                    "UniversalSearch",
                    Some("search_query_response".to_string()),
                );

                // Parse the query from the request payload
                let query: SearchQuery = match req.payload.downcast_ref::<SearchQuery>() {
                    Some(q) => q.clone(),
                    None => {
                        // Try to parse from JSON string
                        if let Some(s) = req.payload.downcast_ref::<String>() {
                            match serde_json::from_str(s) {
                                Ok(q) => q,
                                Err(e) => {
                                    let err = format!("Invalid search query: {}", e);
                                    let _ = req.response_tx.send(Err(nova_kernel::NovaError::new(
                                        nova_kernel::ErrorCategory::ConfigInvalid,
                                        "ERR_SEARCH_INVALID_QUERY",
                                        &err,
                                    )));
                                    continue;
                                }
                            }
                        } else {
                            let _ = req.response_tx.send(Err(nova_kernel::NovaError::new(
                                nova_kernel::ErrorCategory::ConfigInvalid,
                                "ERR_SEARCH_INVALID_QUERY",
                                "Search query payload must be SearchQuery or JSON string",
                            )));
                            continue;
                        }
                    }
                };

                // Execute the search
                let results = match search_self2.search(&query) {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = req.response_tx.send(Err(e));
                        continue;
                    }
                };

                let payload: Arc<dyn std::any::Any + Send + Sync> = Arc::new(results);
                let response = NovaResponse {
                    metadata: res_meta,
                    payload,
                };

                let _ = req.response_tx.send(Ok(response));
            }
        });

        tracing::info!("UniversalSearch started (request handler + event listener).");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        tracing::info!("UniversalSearch stopping.");
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        // Close the search engine
        if let Some(engine) = self.engine.lock().take() {
            drop(engine); // Connection closes on drop
        }
        tracing::info!("UniversalSearch shut down (search index closed).");
        Ok(())
    }

    fn health(&self) -> nova_kernel::ModuleHealth {
        match self.health() {
            Ok(stats) => nova_kernel::ModuleHealth {
                status: nova_kernel::HealthStatus::Healthy,
                detail: format!("{} documents indexed", stats.total),
            },
            Err(e) => nova_kernel::ModuleHealth::unhealthy(format!("search index error: {}", e)),
        }
    }
}

// Need Clone for spawning the event listener
impl Clone for UniversalSearch {
    fn clone(&self) -> Self {
        Self {
            kernel: self.kernel.clone(),
            engine: self.engine.clone(),
            event_rx: self.event_rx.clone(), // Share the same subscription
        }
    }
}
