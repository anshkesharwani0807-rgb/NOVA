use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub enabled: bool,
    pub auto_sync: bool,
    pub sync_interval_secs: u64,
    pub max_batch_size: usize,
    pub encryption_algorithm: EncryptionAlgorithm,
    pub sync_window_sla_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EncryptionAlgorithm {
    X25519EcdhAesGcm,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_sync: false,
            sync_interval_secs: 300,
            max_batch_size: 100,
            encryption_algorithm: EncryptionAlgorithm::X25519EcdhAesGcm,
            sync_window_sla_secs: 60,
        }
    }
}
