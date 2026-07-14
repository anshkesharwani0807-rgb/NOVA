use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::SyncError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: String,
    pub device_name: String,
    pub device_type: DeviceType,
    pub public_key: Vec<u8>,
    pub paired_at: DateTime<Utc>,
    pub last_seen: Option<DateTime<Utc>>,
    pub is_trusted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceType {
    Windows,
    Android,
    Linux,
    MacOS,
    IOS,
    Other(String),
}

pub struct DevicePairing {
    devices: HashMap<String, PairedDevice>,
    our_keypair: Option<(Vec<u8>, Vec<u8>)>,
}

impl Default for DevicePairing {
    fn default() -> Self {
        Self::new()
    }
}

impl DevicePairing {
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
            our_keypair: None,
        }
    }

    pub fn generate_keypair(&mut self) -> Result<(), SyncError> {
        let mut secret_bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut secret_bytes);
        let secret = ed25519_dalek::SigningKey::from_bytes(&secret_bytes);
        let public = secret.verifying_key();
        self.our_keypair = Some((secret.to_bytes().to_vec(), public.to_bytes().to_vec()));
        Ok(())
    }

    pub fn pair_device(
        &mut self,
        device_id: String,
        device_name: String,
        device_type: DeviceType,
        public_key: Vec<u8>,
    ) -> Result<(), SyncError> {
        if self.devices.contains_key(&device_id) {
            return Err(SyncError::AlreadyPaired);
        }
        let device = PairedDevice {
            device_id: device_id.clone(),
            device_name,
            device_type,
            public_key,
            paired_at: Utc::now(),
            last_seen: None,
            is_trusted: false,
        };
        self.devices.insert(device_id, device);
        Ok(())
    }

    pub fn unpair_device(&mut self, device_id: &str) -> Result<(), SyncError> {
        self.devices.remove(device_id).ok_or(SyncError::NotPaired)?;
        Ok(())
    }

    pub fn trust_device(&mut self, device_id: &str) -> Result<(), SyncError> {
        let device = self
            .devices
            .get_mut(device_id)
            .ok_or(SyncError::NotPaired)?;
        device.is_trusted = true;
        Ok(())
    }

    pub fn list_devices(&self) -> Vec<PairedDevice> {
        self.devices.values().cloned().collect()
    }

    pub fn is_paired(&self, device_id: &str) -> bool {
        self.devices.contains_key(device_id)
    }

    pub fn our_public_key(&self) -> Option<&[u8]> {
        self.our_keypair.as_ref().map(|(_, pk)| pk.as_slice())
    }
}
