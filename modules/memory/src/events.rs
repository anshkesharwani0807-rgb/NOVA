//! Memory change events published on the kernel Event Bus (Milestone 5 integration).
//!
//! The Memory Engine publishes a [`MemoryEvent`] whenever a record is created, updated,
//! or deleted, so downstream modules (e.g. the Universal Search index) can react without
//! manual synchronization (ADR-0004). Publishing is a no-op when the engine was created
//! without a kernel (e.g. in unit tests via `MemoryEngine::with_paths`).

use crate::record::MemoryRecord;

/// The kind of change that occurred to a memory record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryEventKind {
    Created,
    Updated,
    Deleted,
}

/// A change notification for a single memory record.
///
/// For `Created` and `Updated`, `record` carries the full record so subscribers can index
/// it without calling back into the engine. For `Deleted`, only `record_id` is provided.
#[derive(Debug, Clone)]
pub struct MemoryEvent {
    pub kind: MemoryEventKind,
    pub record_id: String,
    pub record: Option<MemoryRecord>,
}

impl MemoryEvent {
    /// The stable action string used as the event's `causing_action`.
    pub fn action(&self) -> &'static str {
        match self.kind {
            MemoryEventKind::Created => "memory.created",
            MemoryEventKind::Updated => "memory.updated",
            MemoryEventKind::Deleted => "memory.deleted",
        }
    }
}
