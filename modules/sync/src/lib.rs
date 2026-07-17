use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::info;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Clone, Debug, PartialEq, Eq)]
pub enum SyncError {
    #[error("Conflict detected between local and remote versions")]
    ConflictDetected,

    #[error("Merge failed")]
    MergeFailed,

    #[error("Item not found")]
    ItemNotFound,

    #[error("Storage error")]
    StoreError,

    #[error("Serialization error")]
    SerializationError,

    #[error("Operation timed out")]
    Timeout,
}

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum SyncEvent {
    MemorySynced {
        memory_id: String,
        source_device: String,
    },
    ClipboardSynced {
        content: String,
        source_device: String,
        timestamp: i64,
    },
    ActivityTrailSynced {
        entry_id: String,
        source_device: String,
        action: String,
    },
    AutomationSynced {
        workflow_id: String,
        source_device: String,
        action: String,
    },
    DeviceStatusSynced {
        device_id: String,
        status: String,
    },
    ConflictResolved {
        item_id: String,
        resolution: String,
    },
}

impl fmt::Display for SyncEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncEvent::MemorySynced {
                memory_id,
                source_device,
            } => {
                write!(
                    f,
                    "MemorySynced[memory={memory_id}, device={source_device}]"
                )
            }
            SyncEvent::ClipboardSynced {
                content,
                source_device,
                timestamp,
            } => {
                write!(
                    f,
                    "ClipboardSynced[len={}, device={source_device}, ts={timestamp}]",
                    content.len()
                )
            }
            SyncEvent::ActivityTrailSynced {
                entry_id,
                source_device,
                action,
            } => {
                write!(f, "ActivityTrailSynced[entry={entry_id}, device={source_device}, action={action}]")
            }
            SyncEvent::AutomationSynced {
                workflow_id,
                source_device,
                action,
            } => {
                write!(f, "AutomationSynced[workflow={workflow_id}, device={source_device}, action={action}]")
            }
            SyncEvent::DeviceStatusSynced { device_id, status } => {
                write!(f, "DeviceStatusSynced[device={device_id}, status={status}]")
            }
            SyncEvent::ConflictResolved {
                item_id,
                resolution,
            } => {
                write!(
                    f,
                    "ConflictResolved[item={item_id}, resolution={resolution}]"
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncItem {
    pub id: String,
    pub device_id: String,
    pub item_type: String,
    pub data: Vec<u8>,
    pub timestamp: i64,
    pub version: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SharedMemoryEntry {
    pub key: String,
    pub value: Vec<u8>,
    pub device_id: String,
    pub timestamp: i64,
    pub version: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub id: String,
    pub device_id: String,
    pub action: String,
    pub details: String,
    pub timestamp: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConflictResolution {
    KeepLocal,
    KeepRemote,
    KeepNewest,
    KeepOldest,
    Manual,
}

// ---------------------------------------------------------------------------
// Event bus
// ---------------------------------------------------------------------------

pub struct EventBus {
    tx: broadcast::Sender<SyncEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx }
    }

    pub fn publish(&self, event: SyncEvent) {
        let _ = self.tx.send(event.clone());
        info!("Event published: {event}");
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Clipboard store
// ---------------------------------------------------------------------------

pub struct ClipboardStore {
    inner: RwLock<Option<(String, String, i64)>>,
}

impl ClipboardStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(None),
        }
    }

    pub fn store(&self, content: String, device_id: String) {
        let timestamp = Utc::now().timestamp();
        *self.inner.write() = Some((content, device_id, timestamp));
    }

    pub fn get(&self) -> Option<(String, String, i64)> {
        self.inner.read().clone()
    }

    pub fn clear(&self) {
        *self.inner.write() = None;
    }
}

impl Default for ClipboardStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Shared memory store
// ---------------------------------------------------------------------------

pub struct SharedMemoryStore {
    entries: RwLock<HashMap<String, SharedMemoryEntry>>,
}

impl SharedMemoryStore {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn store(&self, key: &str, value: Vec<u8>, device_id: &str) {
        let timestamp = Utc::now().timestamp();
        let mut map = self.entries.write();
        let version = map.get(key).map(|e| e.version + 1).unwrap_or(1);
        map.insert(
            key.to_string(),
            SharedMemoryEntry {
                key: key.to_string(),
                value,
                device_id: device_id.to_string(),
                timestamp,
                version,
            },
        );
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.entries.read().get(key).map(|e| e.value.clone())
    }

    pub fn get_with_meta(&self, key: &str) -> Option<SharedMemoryEntry> {
        self.entries.read().get(key).cloned()
    }

    pub fn list(&self, device_id: Option<&str>) -> Vec<String> {
        self.entries
            .read()
            .iter()
            .filter(|(_, e)| device_id.as_ref().is_none_or(|d| e.device_id == *d))
            .map(|(k, _)| k.clone())
            .collect()
    }

    pub fn remove(&self, key: &str) {
        self.entries.write().remove(key);
    }

    pub fn clear(&self) {
        self.entries.write().clear();
    }

    pub fn resolve_conflict(&self, key: &str, resolution: ConflictResolution) -> Option<Vec<u8>> {
        use ConflictResolution::*;
        let map = self.entries.read();
        let entry = map.get(key)?;
        match resolution {
            KeepLocal | KeepNewest | KeepOldest => Some(entry.value.clone()),
            KeepRemote => {
                // With a single store we return the stored value as a fallback;
                // a full implementation would merge two copies.
                Some(entry.value.clone())
            }
            Manual => None,
        }
    }
}

impl Default for SharedMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Conflict resolver trait and implementations
// ---------------------------------------------------------------------------

pub trait ConflictResolver: Send + Sync {
    fn resolve(&self, local: &SyncItem, remote: &SyncItem) -> SyncItem;
}

pub struct TimestampBasedResolver;

impl ConflictResolver for TimestampBasedResolver {
    fn resolve(&self, local: &SyncItem, remote: &SyncItem) -> SyncItem {
        if local.timestamp >= remote.timestamp {
            local.clone()
        } else {
            remote.clone()
        }
    }
}

pub struct DevicePriorityResolver {
    priorities: HashMap<String, u32>,
}

impl DevicePriorityResolver {
    pub fn new(priorities: HashMap<String, u32>) -> Self {
        Self { priorities }
    }
}

impl ConflictResolver for DevicePriorityResolver {
    fn resolve(&self, local: &SyncItem, remote: &SyncItem) -> SyncItem {
        let lp = self.priorities.get(&local.device_id).copied().unwrap_or(0);
        let rp = self.priorities.get(&remote.device_id).copied().unwrap_or(0);
        if lp >= rp {
            local.clone()
        } else {
            remote.clone()
        }
    }
}

// ---------------------------------------------------------------------------
// Sync manager
// ---------------------------------------------------------------------------

pub struct SyncManager {
    clipboard_store: ClipboardStore,
    activity_trail: RwLock<Vec<ActivityEntry>>,
    shared_memory: SharedMemoryStore,
    automation_queue: RwLock<Vec<SyncItem>>,
    event_bus: RwLock<Option<Arc<EventBus>>>,
}

impl SyncManager {
    pub fn new() -> Self {
        Self {
            clipboard_store: ClipboardStore::new(),
            activity_trail: RwLock::new(Vec::new()),
            shared_memory: SharedMemoryStore::new(),
            automation_queue: RwLock::new(Vec::new()),
            event_bus: RwLock::new(None),
        }
    }

    pub fn set_event_bus(&self, bus: Arc<EventBus>) {
        *self.event_bus.write() = Some(bus);
    }

    fn publish(&self, event: SyncEvent) {
        if let Some(bus) = self.event_bus.read().as_ref() {
            bus.publish(event);
        }
    }

    pub fn sync_memory(&self, memory_id: &str, data: &[u8], source_device: &str) {
        self.shared_memory
            .store(memory_id, data.to_vec(), source_device);
        self.publish(SyncEvent::MemorySynced {
            memory_id: memory_id.to_string(),
            source_device: source_device.to_string(),
        });
    }

    pub fn sync_clipboard(&self, content: &str, source_device: &str) {
        let timestamp = Utc::now().timestamp();
        self.clipboard_store
            .store(content.to_string(), source_device.to_string());
        self.publish(SyncEvent::ClipboardSynced {
            content: content.to_string(),
            source_device: source_device.to_string(),
            timestamp,
        });
    }

    pub fn get_clipboard(&self) -> Option<String> {
        self.clipboard_store.get().map(|(c, _, _)| c)
    }

    pub fn sync_activity_trail(
        &self,
        entry_id: &str,
        action: &str,
        source_device: &str,
        details: &str,
    ) {
        let entry = ActivityEntry {
            id: entry_id.to_string(),
            device_id: source_device.to_string(),
            action: action.to_string(),
            details: details.to_string(),
            timestamp: Utc::now().timestamp(),
        };
        self.activity_trail.write().push(entry);
        self.publish(SyncEvent::ActivityTrailSynced {
            entry_id: entry_id.to_string(),
            source_device: source_device.to_string(),
            action: action.to_string(),
        });
    }

    pub fn get_activity_trail(&self, limit: usize) -> Vec<ActivityEntry> {
        let trail = self.activity_trail.read();
        trail.iter().rev().take(limit).cloned().collect()
    }

    pub fn sync_automation(&self, workflow_id: &str, action: &str, source_device: &str) {
        self.publish(SyncEvent::AutomationSynced {
            workflow_id: workflow_id.to_string(),
            source_device: source_device.to_string(),
            action: action.to_string(),
        });
    }

    pub fn sync_device_status(&self, device_id: &str, status: &str) {
        self.publish(SyncEvent::DeviceStatusSynced {
            device_id: device_id.to_string(),
            status: status.to_string(),
        });
    }

    pub fn queue_item(&self, item: SyncItem) {
        self.automation_queue.write().push(item);
    }

    pub fn process_queue(&self) -> Vec<SyncItem> {
        let mut queue = self.automation_queue.write();
        std::mem::take(&mut *queue)
    }

    pub fn queue_size(&self) -> usize {
        self.automation_queue.read().len()
    }
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_clipboard_stores_and_retrieves() {
        let sm = SyncManager::new();
        assert!(sm.get_clipboard().is_none());

        sm.sync_clipboard("hello world", "phone-1");
        assert_eq!(sm.get_clipboard(), Some("hello world".to_string()));

        sm.sync_clipboard("second paste", "laptop-1");
        assert_eq!(sm.get_clipboard(), Some("second paste".to_string()));
    }

    #[test]
    fn sync_activity_trail_stores_and_lists() {
        let sm = SyncManager::new();
        sm.sync_activity_trail("e1", "file_open", "phone-1", "opened report.pdf");
        sm.sync_activity_trail("e2", "app_launch", "laptop-1", "launched vscode");

        let trail = sm.get_activity_trail(10);
        assert_eq!(trail.len(), 2);
        assert_eq!(trail[1].id, "e1");
        assert_eq!(trail[1].device_id, "phone-1");
        assert_eq!(trail[1].action, "file_open");
    }

    #[test]
    fn sync_memory_publishes_event() {
        let bus = Arc::new(EventBus::new());
        let mut rx = bus.subscribe();

        let sm = SyncManager::new();
        sm.set_event_bus(Arc::clone(&bus));

        sm.sync_memory("mem_001", b"some data", "phone-1");

        let event = rx.try_recv().expect("should receive event");
        match event {
            SyncEvent::MemorySynced {
                memory_id,
                source_device,
            } => {
                assert_eq!(memory_id, "mem_001");
                assert_eq!(source_device, "phone-1");
            }
            other => panic!("unexpected event: {other}"),
        }
    }

    #[test]
    fn shared_memory_store_get_round_trip() {
        let store = SharedMemoryStore::new();
        assert!(store.get("key1").is_none());

        store.store("key1", b"value1".to_vec(), "device-a");
        let val = store.get("key1");
        assert_eq!(val, Some(b"value1".to_vec()));

        let meta = store.get_with_meta("key1");
        assert!(meta.is_some());
        let meta = meta.unwrap();
        assert_eq!(meta.key, "key1");
        assert_eq!(meta.device_id, "device-a");
        assert_eq!(meta.version, 1);
    }

    #[test]
    fn shared_memory_list_by_device() {
        let store = SharedMemoryStore::new();
        store.store("a", b"1".to_vec(), "dev1");
        store.store("b", b"2".to_vec(), "dev1");
        store.store("c", b"3".to_vec(), "dev2");

        let all = store.list(None);
        assert_eq!(all.len(), 3);

        let dev1_keys = store.list(Some("dev1"));
        assert_eq!(dev1_keys.len(), 2);
        assert!(dev1_keys.contains(&"a".to_string()));
        assert!(dev1_keys.contains(&"b".to_string()));

        let dev2_keys = store.list(Some("dev2"));
        assert_eq!(dev2_keys, vec!["c".to_string()]);
    }

    #[test]
    fn shared_memory_remove() {
        let store = SharedMemoryStore::new();
        store.store("k", b"v".to_vec(), "d1");
        assert!(store.get("k").is_some());

        store.remove("k");
        assert!(store.get("k").is_none());
    }

    #[test]
    fn conflict_resolution_timestamp_based() {
        let resolver = TimestampBasedResolver;
        let local = SyncItem {
            id: "item_1".into(),
            device_id: "phone".into(),
            item_type: "note".into(),
            data: b"local data".to_vec(),
            timestamp: 100,
            version: 1,
        };
        let remote = SyncItem {
            id: "item_1".into(),
            device_id: "laptop".into(),
            item_type: "note".into(),
            data: b"remote data".to_vec(),
            timestamp: 200,
            version: 2,
        };

        let result = resolver.resolve(&local, &remote);
        assert_eq!(result.timestamp, 200);
        assert_eq!(result.data, b"remote data");

        // tie goes to local
        let remote_tied = SyncItem {
            timestamp: 100,
            ..remote.clone()
        };
        let result = resolver.resolve(&local, &remote_tied);
        assert_eq!(result.data, b"local data");
    }

    #[test]
    fn conflict_resolution_device_priority() {
        let mut priorities = HashMap::new();
        priorities.insert("desktop".to_string(), 10);
        priorities.insert("phone".to_string(), 5);

        let resolver = DevicePriorityResolver::new(priorities);

        let desktop = SyncItem {
            id: "x".into(),
            device_id: "desktop".into(),
            item_type: "note".into(),
            data: b"from desktop".to_vec(),
            timestamp: 100,
            version: 1,
        };
        let phone = SyncItem {
            id: "x".into(),
            device_id: "phone".into(),
            item_type: "note".into(),
            data: b"from phone".to_vec(),
            timestamp: 200,
            version: 2,
        };

        // Desktop has higher priority (10 > 5), so desktop wins
        let result = resolver.resolve(&desktop, &phone);
        assert_eq!(result.data, b"from desktop");

        // Reverse order should still pick desktop
        let result = resolver.resolve(&phone, &desktop);
        assert_eq!(result.data, b"from desktop");
    }

    #[test]
    fn queue_item_and_process() {
        let sm = SyncManager::new();
        assert_eq!(sm.queue_size(), 0);

        let item1 = SyncItem {
            id: "q1".into(),
            device_id: "phone".into(),
            item_type: "automation".into(),
            data: b"cmd1".to_vec(),
            timestamp: 1,
            version: 1,
        };
        let item2 = SyncItem {
            id: "q2".into(),
            device_id: "laptop".into(),
            item_type: "automation".into(),
            data: b"cmd2".to_vec(),
            timestamp: 2,
            version: 1,
        };

        sm.queue_item(item1);
        sm.queue_item(item2);
        assert_eq!(sm.queue_size(), 2);

        let processed = sm.process_queue();
        assert_eq!(processed.len(), 2);
        assert_eq!(processed[0].id, "q1");
        assert_eq!(processed[1].id, "q2");
        assert_eq!(sm.queue_size(), 0);
    }

    #[test]
    fn queue_size_tracking() {
        let sm = SyncManager::new();
        assert_eq!(sm.queue_size(), 0);

        for i in 0..5 {
            sm.queue_item(SyncItem {
                id: format!("q{i}"),
                device_id: "test".into(),
                item_type: "test".into(),
                data: vec![],
                timestamp: i,
                version: 1,
            });
        }
        assert_eq!(sm.queue_size(), 5);

        sm.process_queue();
        assert_eq!(sm.queue_size(), 0);
    }

    #[test]
    fn clipboard_clear() {
        let store = ClipboardStore::new();
        store.store("data".to_string(), "dev1".to_string());
        assert!(store.get().is_some());

        store.clear();
        assert!(store.get().is_none());
    }

    #[test]
    fn activity_trail_limit() {
        let sm = SyncManager::new();
        for i in 0..10 {
            sm.sync_activity_trail(&format!("e{i}"), "test", "phone", &format!("detail {i}"));
        }

        let all = sm.get_activity_trail(100);
        assert_eq!(all.len(), 10);

        let limited = sm.get_activity_trail(3);
        assert_eq!(limited.len(), 3);
        // Most recent entries come first (reversed order)
        assert_eq!(limited[0].id, "e9");
        assert_eq!(limited[1].id, "e8");
        assert_eq!(limited[2].id, "e7");
    }

    #[test]
    fn entries_contain_correct_source_device() {
        let sm = SyncManager::new();

        sm.sync_clipboard("clip test", "device-alpha");
        let clip = sm.clipboard_store.get();
        assert!(clip.is_some());
        let (_, device, _) = clip.unwrap();
        assert_eq!(device, "device-alpha");

        sm.sync_activity_trail("act_1", "login", "device-beta", "logged in");
        let trail = sm.get_activity_trail(1);
        assert_eq!(trail[0].device_id, "device-beta");
        assert_eq!(trail[0].action, "login");
        assert_eq!(trail[0].details, "logged in");

        let store = SharedMemoryStore::new();
        store.store("sk", b"sv".to_vec(), "device-gamma");
        let meta = store.get_with_meta("sk").unwrap();
        assert_eq!(meta.device_id, "device-gamma");
    }
}
