#![allow(dead_code)]

use crate::error::SyncError;
use crate::protocol::SyncMessage;
use async_trait::async_trait;

#[async_trait]
pub trait SyncTransport: Send + Sync {
    async fn send(&self, device_id: &str, msg: SyncMessage) -> Result<(), SyncError>;
    async fn receive(&self, device_id: &str) -> Result<Option<SyncMessage>, SyncError>;
    async fn handshake(&self, device_id: &str) -> Result<bool, SyncError>;
}

pub struct LocalNetworkTransport;

#[async_trait]
impl SyncTransport for LocalNetworkTransport {
    async fn send(&self, _device_id: &str, _msg: SyncMessage) -> Result<(), SyncError> {
        Err(SyncError::Transport("LAN transport not implemented".into()))
    }

    async fn receive(&self, _device_id: &str) -> Result<Option<SyncMessage>, SyncError> {
        Err(SyncError::Transport("LAN transport not implemented".into()))
    }

    async fn handshake(&self, _device_id: &str) -> Result<bool, SyncError> {
        Err(SyncError::Transport("LAN transport not implemented".into()))
    }
}
