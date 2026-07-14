use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use x25519_dalek::{EphemeralSecret, PublicKey};

use crate::error::SyncError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    pub version: u8,
    pub sender_id: String,
    pub receiver_id: String,
    pub payload: Vec<u8>,
    pub nonce: Vec<u8>,
    pub ephemeral_key: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPayload {
    pub data_type: SyncDataType,
    pub entries: Vec<SyncEntry>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncDataType {
    Memory,
    Config,
    Preferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEntry {
    pub id: String,
    pub action: SyncAction,
    pub data: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncAction {
    Create,
    Update,
    Delete,
}

pub struct SyncProtocol;

impl SyncProtocol {
    pub fn encrypt(payload: &[u8], receiver_public_key: &[u8]) -> Result<SyncMessage, SyncError> {
        let receiver_pk_array: [u8; 32] = receiver_public_key
            .try_into()
            .map_err(|_| SyncError::Encryption("invalid public key length".into()))?;
        let receiver_pk = PublicKey::from(receiver_pk_array);

        let ephemeral_secret = EphemeralSecret::random_from_rng(rand::thread_rng());
        let ephemeral_public = PublicKey::from(&ephemeral_secret);
        let shared_secret = ephemeral_secret.diffie_hellman(&receiver_pk);

        let key = blake3::hash(shared_secret.as_bytes());
        let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
            .map_err(|e| SyncError::Encryption(e.to_string()))?;

        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, payload)
            .map_err(|e| SyncError::Encryption(e.to_string()))?;

        Ok(SyncMessage {
            version: 1,
            sender_id: String::new(),
            receiver_id: String::new(),
            payload: ciphertext,
            nonce: nonce_bytes.to_vec(),
            ephemeral_key: ephemeral_public.to_bytes().to_vec(),
        })
    }

    pub fn decrypt(msg: &SyncMessage, our_secret_key: &[u8]) -> Result<Vec<u8>, SyncError> {
        let ephemeral_array: [u8; 32] = msg
            .ephemeral_key
            .as_slice()
            .try_into()
            .map_err(|_| SyncError::Decryption("invalid ephemeral key length".into()))?;
        let ephemeral_pk = PublicKey::from(ephemeral_array);

        let _our_secret_array: [u8; 32] = our_secret_key
            .try_into()
            .map_err(|_| SyncError::Decryption("invalid secret key length".into()))?;
        let our_secret = EphemeralSecret::random_from_rng(rand::thread_rng());
        let shared_secret = our_secret.diffie_hellman(&ephemeral_pk);

        let key = blake3::hash(shared_secret.as_bytes());
        let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
            .map_err(|e| SyncError::Decryption(e.to_string()))?;

        let nonce = Nonce::from_slice(&msg.nonce);
        let plaintext = cipher
            .decrypt(nonce, msg.payload.as_ref())
            .map_err(|e| SyncError::Decryption(e.to_string()))?;

        Ok(plaintext)
    }
}
