//! NOVA Memory Engine (Milestone 4) — the first persistent, encrypted, local store.
//!
//! `MemoryEngine` is a [`KernelModule`] that owns an encrypted SQLite [`Store`]. It opens
//! and migrates the database on `initialize()`, serves a synchronous CRUD/search API
//! guarded by a mutex for safe concurrent access, and closes cleanly on `shutdown()`.
//! Sensitive fields are encrypted at rest (see [`crypto`]); everything is local and
//! offline — no cloud, no network.

pub mod crypto;
pub mod events;
pub mod record;
pub mod store;

pub use crypto::{Cipher, FileKeyProvider, InMemoryKeyProvider, KeyProvider};
pub use events::{MemoryEvent, MemoryEventKind};
pub use record::{MemoryCategory, MemoryOp, MemoryRecord, Query, SearchMode, SortBy};
pub use store::{Store, SCHEMA_VERSION};

use async_trait::async_trait;
use nova_kernel::{
    get_config, log_activity, ErrorCategory, EventBus, EventMetadata, HealthStatus, Kernel,
    KernelModule, ModuleHealth, NovaError, NovaEvent, Result,
};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn engine_err(code: &'static str, detail: impl std::fmt::Display) -> NovaError {
    NovaError::new(ErrorCategory::Storage, code, &detail.to_string())
}

fn not_open() -> NovaError {
    NovaError::new(
        ErrorCategory::Storage,
        "ERR_MEM_NOT_OPEN",
        "memory engine database is not open",
    )
}

/// The persistent memory subsystem. Registered with the kernel's module registry and
/// driven through the standard lifecycle (initialize → start → stop → shutdown).
pub struct MemoryEngine {
    db_path: PathBuf,
    key_path: PathBuf,
    inner: Mutex<Option<Store>>,
    /// Event bus for publishing memory-change events (None when constructed without a
    /// kernel, e.g. in unit tests). Publishing is a no-op in that case.
    event_bus: Option<Arc<EventBus>>,
}

impl MemoryEngine {
    /// Construct using the kernel's configured database path (resolved under the kernel's
    /// runtime directory so all data stays local to the instance).
    pub fn new(kernel: Arc<Kernel>) -> Self {
        let cfg = get_config();
        let base = kernel
            .config_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| kernel.config_dir.clone());
        let db_path = base.join(&cfg.memory.db_path);
        let key_path = db_path.with_extension("key");
        let mut engine = Self::with_paths(db_path, key_path);
        engine.event_bus = Some(kernel.event_bus.clone());
        engine
    }

    /// Construct with explicit database and key-file paths (used by the demo and tests).
    pub fn with_paths(db_path: impl Into<PathBuf>, key_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            key_path: key_path.into(),
            inner: Mutex::new(None),
            event_bus: None,
        }
    }

    /// Publish a memory-change event (no-op if there is no event bus).
    fn publish(&self, kind: MemoryEventKind, record_id: &str, record: Option<MemoryRecord>) {
        if let Some(bus) = &self.event_bus {
            let event = MemoryEvent {
                kind,
                record_id: record_id.to_string(),
                record,
            };
            let metadata = EventMetadata::new("memory", Some(event.action().to_string()));
            let payload: Arc<dyn std::any::Any + Send + Sync> = Arc::new(event);
            let _ = bus.publish(NovaEvent { metadata, payload });
        }
    }

    /// The database file path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Open (and migrate) the database. Idempotent.
    pub fn open(&self) -> Result<()> {
        let mut guard = self.inner.lock();
        if guard.is_some() {
            return Ok(());
        }
        let provider = FileKeyProvider::new(self.key_path.clone());
        *guard = Some(Store::open(&self.db_path, &provider)?);
        Ok(())
    }

    /// Close the database connection.
    pub fn close(&self) {
        *self.inner.lock() = None;
    }

    /// Whether the database is currently open.
    pub fn is_open(&self) -> bool {
        self.inner.lock().is_some()
    }

    fn with_store<T>(&self, f: impl FnOnce(&Store) -> Result<T>) -> Result<T> {
        let guard = self.inner.lock();
        let store = guard.as_ref().ok_or_else(not_open)?;
        f(store)
    }

    fn with_store_mut<T>(&self, f: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
        let mut guard = self.inner.lock();
        let store = guard.as_mut().ok_or_else(not_open)?;
        f(store)
    }

    pub fn insert(&self, rec: &MemoryRecord) -> Result<()> {
        self.with_store(|s| s.insert(rec))?;
        self.publish(MemoryEventKind::Created, &rec.id, Some(rec.clone()));
        Ok(())
    }

    pub fn update(&self, rec: &MemoryRecord) -> Result<()> {
        self.with_store(|s| s.update(rec))?;
        self.publish(MemoryEventKind::Updated, &rec.id, Some(rec.clone()));
        Ok(())
    }

    /// Soft-delete (recoverable).
    pub fn delete(&self, id: &str) -> Result<()> {
        self.with_store(|s| s.soft_delete(id))?;
        self.publish(MemoryEventKind::Deleted, id, None);
        Ok(())
    }

    /// Restore a soft-deleted record.
    pub fn restore_record(&self, id: &str) -> Result<()> {
        self.with_store(|s| s.restore(id))?;
        // Re-publish as an update so subscribers re-index the now-active record.
        let record = self.find_by_id(id)?;
        self.publish(MemoryEventKind::Updated, id, record);
        Ok(())
    }

    /// Permanently remove a single record.
    pub fn purge(&self, id: &str) -> Result<()> {
        self.with_store(|s| s.purge(id))?;
        self.publish(MemoryEventKind::Deleted, id, None);
        Ok(())
    }

    /// Permanently remove all soft-deleted records.
    pub fn purge_deleted(&self) -> Result<usize> {
        self.with_store(|s| s.purge_deleted())
    }

    pub fn find(&self, query: &Query) -> Result<Vec<MemoryRecord>> {
        self.with_store(|s| s.query(query))
    }

    /// Alias for [`MemoryEngine::find`] emphasising text/tag search intent.
    pub fn search(&self, query: &Query) -> Result<Vec<MemoryRecord>> {
        self.with_store(|s| s.query(query))
    }

    pub fn find_by_id(&self, id: &str) -> Result<Option<MemoryRecord>> {
        self.with_store(|s| s.find_by_id(id))
    }

    pub fn count(&self, query: &Query) -> Result<usize> {
        self.with_store(|s| s.count(query))
    }

    pub fn exists(&self, id: &str) -> Result<bool> {
        self.with_store(|s| s.exists(id))
    }

    /// Apply a batch of operations atomically.
    pub fn transaction(&self, ops: &[MemoryOp]) -> Result<()> {
        self.with_store_mut(|s| s.transaction(ops))
    }

    /// Write a consistent backup copy to `dest`.
    pub fn backup(&self, dest: &Path) -> Result<()> {
        self.with_store(|s| s.backup(dest))
    }

    /// Reclaim space and defragment the database.
    pub fn vacuum(&self) -> Result<()> {
        self.with_store(|s| s.vacuum())
    }

    /// Replace the live database with the contents of a backup file, then reopen.
    pub fn restore(&self, src: &Path) -> Result<()> {
        let mut guard = self.inner.lock();
        *guard = None; // close the current connection first
        for ext in ["-wal", "-shm"] {
            let side = PathBuf::from(format!("{}{ext}", self.db_path.display()));
            let _ = std::fs::remove_file(side);
        }
        std::fs::copy(src, &self.db_path).map_err(|e| engine_err("ERR_MEM_RESTORE", e))?;
        let provider = FileKeyProvider::new(self.key_path.clone());
        *guard = Some(Store::open(&self.db_path, &provider)?);
        log_activity(
            "memory",
            "memory.restore_db",
            &format!("src={}", src.display()),
            None,
        );
        Ok(())
    }

    /// Total number of records (including soft-deleted).
    pub fn total(&self) -> Result<usize> {
        self.with_store(|s| s.total())
    }
}

#[async_trait]
impl KernelModule for MemoryEngine {
    fn module_id(&self) -> &'static str {
        "memory"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn initialize(&self) -> Result<()> {
        self.open()?;
        tracing::info!("MemoryEngine database opened at {}", self.db_path.display());
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        tracing::info!("MemoryEngine started (persistent store ready).");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        self.close();
        tracing::info!("MemoryEngine database closed.");
        Ok(())
    }

    fn health(&self) -> ModuleHealth {
        let guard = self.inner.lock();
        match guard.as_ref() {
            None => ModuleHealth::unhealthy("database not open"),
            Some(store) => match store.total() {
                Ok(n) => ModuleHealth {
                    status: HealthStatus::Healthy,
                    detail: format!("{n} records"),
                },
                Err(e) => ModuleHealth::unhealthy(format!("database error: {e}")),
            },
        }
    }
}
