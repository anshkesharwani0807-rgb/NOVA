use nova_kernel::{ErrorCategory, NovaError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("device not paired")]
    NotPaired,
    #[error("device already paired")]
    AlreadyPaired,
    #[error("pairing rejected")]
    PairingRejected,
    #[error("encryption error: {0}")]
    Encryption(String),
    #[error("decryption error: {0}")]
    Decryption(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("sync in progress")]
    SyncInProgress,
    #[error("sync disabled")]
    SyncDisabled,
}

impl From<SyncError> for NovaError {
    fn from(e: SyncError) -> Self {
        let category = match &e {
            SyncError::Encryption(_) | SyncError::Decryption(_) => ErrorCategory::Internal,
            SyncError::NotPaired | SyncError::AlreadyPaired | SyncError::PairingRejected => {
                ErrorCategory::ConfigInvalid
            }
            _ => ErrorCategory::Internal,
        };
        NovaError::new(category, "ERR_SYNC", &e.to_string())
    }
}
