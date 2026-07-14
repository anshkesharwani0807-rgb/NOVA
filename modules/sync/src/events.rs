use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncEventPayload {
    DevicePaired {
        device_id: String,
        device_name: String,
    },
    DeviceUnpaired {
        device_id: String,
    },
    DeviceTrusted {
        device_id: String,
    },
    SyncStarted {
        device_id: String,
        batch_size: usize,
    },
    SyncCompleted {
        device_id: String,
        items_synced: usize,
        duration_ms: u64,
    },
    SyncFailed {
        device_id: String,
        error: String,
    },
    SyncDisabled,
    SyncEnabled,
    IncomingData {
        device_id: String,
        data_type: String,
        size_bytes: usize,
    },
}
