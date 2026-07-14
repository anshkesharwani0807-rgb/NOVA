use crate::pairing::{DevicePairing, PairedDevice};

pub struct SyncManager {
    pub pairing: DevicePairing,
    pub sync_in_progress: bool,
    pub total_synced: u64,
    pub last_sync_time: Option<i64>,
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncManager {
    pub fn new() -> Self {
        Self {
            pairing: DevicePairing::new(),
            sync_in_progress: false,
            total_synced: 0,
            last_sync_time: None,
        }
    }

    pub fn paired_devices(&self) -> &[PairedDevice] {
        &[]
    }

    pub fn sync_stats(&self) -> String {
        format!(
            "SyncManager{{ paired: {}, synced: {}, in_progress: {} }}",
            self.pairing.list_devices().len(),
            self.total_synced,
            self.sync_in_progress,
        )
    }

    pub fn can_sync(&self) -> bool {
        !self.pairing.list_devices().is_empty() && !self.sync_in_progress
    }
}
