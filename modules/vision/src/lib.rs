pub mod cache;
pub mod caption;
pub mod color;
pub mod config;
pub mod context_builder;
pub mod decoder;
pub mod detection;
pub mod embedding;
pub mod engine;
pub mod error;
pub mod events;
pub mod face;
pub mod hashing;
pub mod image_loader;
pub mod manager;
pub mod metadata;
pub mod ocr;
pub mod permission;
pub mod preprocessor;
pub mod providers;
pub mod quality;
pub mod scene;
pub mod screenshot;
pub mod search;
pub mod tags;
pub mod thumbnail;
pub mod tools;

pub use config::VisionConfig;
pub use context_builder::{VisionContext, VisionContextBuilder};
pub use engine::{AnalysisResult, VisionEngine};
pub use events::{VisionEvent, VisionEventPayload};
pub use permission::{VisionCapability, VisionPermissionManager};
pub use preprocessor::{ImagePreprocessor, PreprocessedImage, ResizeMode};
pub use providers::VisionProvider;
pub use screenshot::{ScreenshotAnalysis, ScreenshotAnalyzer, UiElement, UiElementType};
pub use search::VisualSearch;
pub use tools::VisionToolkit;

use async_trait::async_trait;
use nova_ai::tool::ToolRegistry;
use nova_kernel::{HealthStatus, Kernel, KernelModule, ModuleHealth, Result};
use parking_lot::RwLock;
use std::sync::Arc;

pub struct VisionSystem {
    #[allow(dead_code)]
    kernel: Arc<Kernel>,
    engine: Arc<VisionEngine>,
    manager: Arc<manager::VisionManager>,
    cache: Arc<cache::VisionCache>,
    search: Arc<RwLock<search::VisualSearch>>,
    permissions: Arc<VisionPermissionManager>,
    config: Arc<RwLock<VisionConfig>>,
    toolkit: Arc<VisionToolkit>,
    audit: Arc<RwLock<Vec<VisionEvent>>>,
}

impl VisionSystem {
    pub fn new(kernel: Arc<Kernel>) -> Self {
        let config = Arc::new(RwLock::new(VisionConfig::default()));
        let permissions = Arc::new(VisionPermissionManager::new());
        let provider =
            Arc::new(providers::mock::MockVisionProvider::new()) as Arc<dyn VisionProvider>;
        let engine = Arc::new(VisionEngine::new(provider.clone()));
        let cache = Arc::new(cache::VisionCache::new(
            config.read().max_cache_entries,
            config.read().cache_ttl_secs,
            config.read().memory_budget_bytes,
        ));
        let manager = Arc::new(manager::VisionManager::new(engine.clone()));
        let search = Arc::new(RwLock::new(search::VisualSearch::new(engine.clone())));
        let audit = Arc::new(RwLock::new(Vec::new()));
        let toolkit = Arc::new(VisionToolkit::new(
            engine.clone(),
            permissions.clone(),
            audit.clone(),
        ));

        Self {
            kernel,
            engine,
            manager,
            cache,
            search,
            permissions,
            config,
            toolkit,
            audit,
        }
    }

    pub fn engine(&self) -> &Arc<VisionEngine> {
        &self.engine
    }

    pub fn permissions(&self) -> &Arc<VisionPermissionManager> {
        &self.permissions
    }

    pub fn config(&self) -> &Arc<RwLock<VisionConfig>> {
        &self.config
    }

    pub fn toolkit(&self) -> &Arc<VisionToolkit> {
        &self.toolkit
    }

    pub fn cache(&self) -> &Arc<cache::VisionCache> {
        &self.cache
    }

    pub fn search(&self) -> &Arc<RwLock<search::VisualSearch>> {
        &self.search
    }

    pub fn audit_log(&self) -> Vec<VisionEvent> {
        self.audit.read().clone()
    }

    pub fn screenshot_analyzer(&self) -> &Arc<dyn ScreenshotAnalyzer> {
        &self.engine.screenshot
    }

    pub fn context_builder(&self) -> &Arc<VisionContextBuilder> {
        &self.engine.context_builder
    }

    pub fn preprocessor(&self) -> &Arc<ImagePreprocessor> {
        &self.engine.preprocessor
    }

    pub fn update_config(&self, new_config: VisionConfig) {
        *self.config.write() = new_config;
    }

    pub fn register_tools(&self, tool_registry: &ToolRegistry) -> Result<()> {
        for tool in &self.toolkit.tools {
            tool_registry.register(tool.clone())?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn publish_event(&self, event: VisionEvent, causing_action: Option<String>) {
        let action = event.action_name().to_string();
        let desc = event.description();
        nova_kernel::log_activity("vision", &action, &desc, Some(event.correlation_id));
        let meta = nova_kernel::EventMetadata::new("vision", causing_action);
        let _ = self.kernel.event_bus.publish(nova_kernel::NovaEvent {
            metadata: meta,
            payload: std::sync::Arc::new(event.clone()),
        });
        self.audit.write().push(event);
    }
}

#[async_trait]
impl KernelModule for VisionSystem {
    fn module_id(&self) -> &'static str {
        "vision"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn initialize(&self) -> Result<()> {
        tracing::info!(
            "VisionSystem initialized ({} tools, {} providers, {} capabilities)",
            self.toolkit.count(),
            1,
            self.permissions.count(),
        );
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        tracing::info!("VisionSystem started (offline vision pipeline ready)");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.cache.clear();
        self.search.write().clear();
        self.manager.clear_queue();
        tracing::info!("VisionSystem stopped");
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("VisionSystem shut down");
        Ok(())
    }

    fn health(&self) -> ModuleHealth {
        let cache_stats = self.cache.stats();
        ModuleHealth {
            status: HealthStatus::Healthy,
            detail: format!(
                "{} tools, {} cached thumbnails, {} queued analyses",
                self.toolkit.count(),
                cache_stats.thumbnails,
                self.manager.pending_count(),
            ),
        }
    }
}
