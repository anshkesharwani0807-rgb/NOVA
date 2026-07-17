//! M16 Cross-Device Pairing module (nova_pairing).
//!
//! Provides QR-based pairing flow, 6-digit code verification, X25519 key
//! exchange, and trusted device management for the NOVA ecosystem.

#![doc(html_root_url = "https://docs.rs/nova_pairing/0.1.0")]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use base64::Engine;
use chrono::{DateTime, Utc};
use nova_security::SecurityManager;
use parking_lot::RwLock;
use qrcode::QrCode;
use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Duration (in minutes) after which a pairing session expires.
const SESSION_TIMEOUT_MINUTES: i64 = 5;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can arise during the pairing process.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PairingError {
    /// The provided 6-digit code does not match.
    #[error("Invalid pairing code")]
    InvalidCode,
    /// The pairing code or session has expired.
    #[error("Pairing code has expired")]
    CodeExpired,
    /// X25519 key agreement failed.
    #[error("Key exchange failed")]
    KeyExchangeFailed,
    /// The target device is already in the trusted store.
    #[error("Device is already trusted")]
    DeviceAlreadyTrusted,
    /// The requested device was not found in the trusted store.
    #[error("Device not found")]
    DeviceNotFound,
    /// An internal error occurred while accessing the trusted-device store.
    #[error("Failed to access device store")]
    StoreError,
    /// QR code encoding / rendering failed.
    #[error("Failed to generate QR code")]
    QrGenerationFailed,
    /// The user explicitly rejected the pairing request.
    #[error("Pairing was rejected by the user")]
    PairingRejectedByUser,
    /// The pairing request timed out before completion.
    #[error("Pairing timed out")]
    PairingTimeout,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// The lifecycle state of a pairing session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PairingState {
    /// Initial state — waiting for the 6-digit code to be verified.
    AwaitingCode,
    /// The 6-digit code was successfully verified.
    CodeVerified,
    /// Public keys have been exchanged and a shared secret derived.
    KeyExchanged,
    /// The pairing was confirmed and the device added to the trusted store.
    Trusted,
    /// The session expired before completion.
    Expired,
    /// The pairing was rejected by the user.
    Rejected,
}

/// A single pairing session between the NOVA server and a remote device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingSession {
    /// Unique identifier for this session.
    pub session_id: String,
    /// Device identifier of the device being paired.
    pub device_id: String,
    /// Human-readable name of the device.
    pub device_name: String,
    /// Device type (e.g. "android", "windows", "linux").
    pub device_type: String,
    /// The 6-digit verification code displayed on the initiator.
    pub pairing_code: String,
    /// The initiator's X25519 public key (embedded in QR data).
    pub public_key: Vec<u8>,
    /// When this session was created.
    pub created_at: DateTime<Utc>,
    /// When this session expires (created_at + 5 minutes).
    pub expires_at: DateTime<Utc>,
    /// Current state of the pairing flow.
    pub state: PairingState,
}

/// A device that has been successfully paired and is considered trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedDevice {
    /// Device identifier of the trusted device.
    pub device_id: String,
    /// Human-readable name.
    pub device_name: String,
    /// Device type.
    pub device_type: String,
    /// PEM-encoded public key for wire-format compatibility.
    pub public_key_pem: String,
    /// Raw public key bytes (X25519, 32 bytes).
    pub public_key_bytes: Vec<u8>,
    /// When the device was first paired.
    pub paired_at: DateTime<Utc>,
    /// Last time the device communicated with the server.
    pub last_seen: DateTime<Utc>,
    /// Set of granted permission names (e.g. "nova.files").
    pub permissions: HashSet<String>,
    /// Whether the device is currently trusted.
    pub is_trusted: bool,
}

/// Data embedded in the QR code for the pairing handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrPairingData {
    /// Session identifier the responder must use.
    pub session_id: String,
    /// The 6-digit verification code.
    pub code: String,
    /// Name of the device that initiated pairing.
    pub device_name: String,
    /// Type of the device that initiated pairing.
    pub device_type: String,
    /// The initiator's X25519 public key (used for key agreement).
    pub public_key: Vec<u8>,
}

// ---------------------------------------------------------------------------
// TrustedDeviceStore
// ---------------------------------------------------------------------------

/// In-memory store of trusted devices, backed by a `parking_lot::RwLock`.
pub struct TrustedDeviceStore {
    devices: RwLock<Vec<TrustedDevice>>,
}

impl TrustedDeviceStore {
    /// Create an empty trusted-device store.
    pub fn new() -> Self {
        Self {
            devices: RwLock::new(Vec::new()),
        }
    }

    /// Add a device to the store.
    pub fn add(&self, device: TrustedDevice) {
        self.devices.write().push(device);
    }

    /// Remove a device by its identifier.
    pub fn remove(&self, device_id: &str) {
        self.devices.write().retain(|d| d.device_id != device_id);
    }

    /// Retrieve a device by its identifier.
    pub fn get(&self, device_id: &str) -> Option<TrustedDevice> {
        self.devices
            .read()
            .iter()
            .find(|d| d.device_id == device_id)
            .cloned()
    }

    /// Return a copy of all trusted devices.
    pub fn list(&self) -> Vec<TrustedDevice> {
        self.devices.read().clone()
    }

    /// Check whether a device is currently trusted.
    pub fn is_trusted(&self, device_id: &str) -> bool {
        self.devices
            .read()
            .iter()
            .any(|d| d.device_id == device_id && d.is_trusted)
    }

    /// Update the `last_seen` timestamp for a given device.
    pub fn update_last_seen(&self, device_id: &str, timestamp: DateTime<Utc>) {
        let mut devices = self.devices.write();
        if let Some(device) = devices.iter_mut().find(|d| d.device_id == device_id) {
            device.last_seen = timestamp;
        }
    }

    /// Return the number of devices in the store.
    pub fn count(&self) -> usize {
        self.devices.read().len()
    }
}

impl Default for TrustedDeviceStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// PairingManager
// ---------------------------------------------------------------------------

/// High-level manager for QR-code pairing, code verification, key exchange,
/// and trusted-device lifecycle.
pub struct PairingManager {
    security_manager: Arc<SecurityManager>,
    sessions: RwLock<HashMap<String, PairingSession>>,
    peer_public_keys: RwLock<HashMap<String, Vec<u8>>>,
    store: Arc<TrustedDeviceStore>,
}

impl PairingManager {
    /// Create a new `PairingManager` that uses the given `SecurityManager`
    /// for X25519 key agreement and shared-secret derivation.
    pub fn new(security_manager: Arc<SecurityManager>) -> Self {
        Self {
            security_manager,
            sessions: RwLock::new(HashMap::new()),
            peer_public_keys: RwLock::new(HashMap::new()),
            store: Arc::new(TrustedDeviceStore::new()),
        }
    }

    // -- Internal helpers --------------------------------------------------

    /// Remove all sessions that have exceeded their 5-minute timeout.
    fn evict_expired_sessions(
        sessions: &mut HashMap<String, PairingSession>,
        peer_keys: &mut HashMap<String, Vec<u8>>,
    ) {
        let now = Utc::now();
        sessions.retain(|id, s| {
            if now > s.expires_at && s.state != PairingState::Trusted {
                s.state = PairingState::Expired;
                peer_keys.remove(id);
                return false;
            }
            true
        });
    }

    // -- Public API --------------------------------------------------------

    /// Start a new pairing session for the given device.
    ///
    /// A random 6-digit code is generated, the initiator's X25519 public key
    /// is captured from the `SecurityManager`, and the session is stored with
    /// a 5-minute expiry.
    pub fn initiate_pairing(
        &self,
        device_id: &str,
        device_name: &str,
        device_type: &str,
    ) -> PairingSession {
        let session_id = Uuid::new_v4().to_string();
        let pairing_code = Self::generate_pairing_code();
        let public_key = self.security_manager.x25519_public_key_bytes().to_vec();
        let now = Utc::now();

        let session = PairingSession {
            session_id: session_id.clone(),
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            device_type: device_type.to_string(),
            pairing_code,
            public_key,
            created_at: now,
            expires_at: now + chrono::Duration::minutes(SESSION_TIMEOUT_MINUTES),
            state: PairingState::AwaitingCode,
        };

        {
            let mut sessions = self.sessions.write();
            let mut peer_keys = self.peer_public_keys.write();
            Self::evict_expired_sessions(&mut sessions, &mut peer_keys);
            sessions.insert(session_id, session.clone());
        }

        tracing::info!(
            "Pairing session initiated: device={} name={} type={}",
            device_id,
            device_name,
            device_type
        );

        session
    }

    /// Verify the 6-digit pairing code for a session.
    ///
    /// Returns `Ok(())` when the code matches and the session has not
    /// expired.
    pub fn verify_code(&self, session_id: &str, code: &str) -> Result<(), PairingError> {
        let mut sessions = self.sessions.write();
        let mut peer_keys = self.peer_public_keys.write();

        let expired = sessions
            .get(session_id)
            .is_some_and(|s| Utc::now() > s.expires_at);
        Self::evict_expired_sessions(&mut sessions, &mut peer_keys);
        if expired {
            return Err(PairingError::CodeExpired);
        }

        let session = sessions
            .get_mut(session_id)
            .ok_or(PairingError::InvalidCode)?;

        if session.pairing_code != code {
            return Err(PairingError::InvalidCode);
        }

        session.state = PairingState::CodeVerified;
        tracing::info!("Pairing code verified for session {}", session_id);
        Ok(())
    }

    /// Exchange X25519 public keys and derive a shared secret.
    ///
    /// Stores `peer_public_key` for the session, computes the X25519+HKDF
    /// shared secret, and returns `(our_public_key, shared_secret)`.
    pub fn exchange_keys(
        &self,
        session_id: &str,
        peer_public_key: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>), PairingError> {
        let mut sessions = self.sessions.write();
        let mut peer_keys = self.peer_public_keys.write();

        let expired = sessions
            .get(session_id)
            .is_some_and(|s| Utc::now() > s.expires_at);
        Self::evict_expired_sessions(&mut sessions, &mut peer_keys);
        if expired {
            return Err(PairingError::CodeExpired);
        }

        let session = sessions
            .get_mut(session_id)
            .ok_or(PairingError::InvalidCode)?;

        if session.state != PairingState::CodeVerified
            && session.state != PairingState::AwaitingCode
        {
            return Err(PairingError::KeyExchangeFailed);
        }

        let shared_secret = self
            .security_manager
            .generate_shared_secret(peer_public_key)
            .map_err(|_| PairingError::KeyExchangeFailed)?;

        let my_public_key = self.security_manager.x25519_public_key_bytes();

        peer_keys.insert(session_id.to_string(), peer_public_key.to_vec());
        session.state = PairingState::KeyExchanged;

        tracing::info!("Keys exchanged for session {}", session_id);

        Ok((my_public_key.to_vec(), shared_secret.to_vec()))
    }

    /// Finalise a pairing session by adding the device to the trusted store.
    ///
    /// The session must be in the `KeyExchanged` state.  Returns the newly
    /// created `TrustedDevice`.
    pub fn confirm_pairing(&self, session_id: &str) -> Result<TrustedDevice, PairingError> {
        let mut sessions = self.sessions.write();
        let mut peer_keys = self.peer_public_keys.write();

        let expired = sessions
            .get(session_id)
            .is_some_and(|s| Utc::now() > s.expires_at);
        Self::evict_expired_sessions(&mut sessions, &mut peer_keys);
        if expired {
            return Err(PairingError::CodeExpired);
        }

        let session = sessions
            .get_mut(session_id)
            .ok_or(PairingError::InvalidCode)?;

        if session.state != PairingState::KeyExchanged {
            return Err(PairingError::PairingRejectedByUser);
        }

        if self.store.is_trusted(&session.device_id) {
            return Err(PairingError::DeviceAlreadyTrusted);
        }

        let peer_key = peer_keys.get(session_id).cloned().unwrap_or_default();
        let pem = encode_pem(&peer_key);

        let device = TrustedDevice {
            device_id: session.device_id.clone(),
            device_name: session.device_name.clone(),
            device_type: session.device_type.clone(),
            public_key_pem: pem,
            public_key_bytes: peer_key,
            paired_at: Utc::now(),
            last_seen: Utc::now(),
            permissions: HashSet::new(),
            is_trusted: true,
        };

        self.store.add(device.clone());
        session.state = PairingState::Trusted;

        tracing::info!(
            "Device paired and trusted: {} ({})",
            session.device_id,
            session.device_name
        );

        Ok(device)
    }

    /// Reject a pending pairing session, moving it to the `Rejected` state.
    pub fn reject_pairing(&self, session_id: &str) {
        let mut sessions = self.sessions.write();
        let mut peer_keys = self.peer_public_keys.write();
        if let Some(session) = sessions.get_mut(session_id) {
            session.state = PairingState::Rejected;
            tracing::info!("Pairing rejected for session {}", session_id);
        }
        peer_keys.remove(session_id);
    }

    /// Return the list of all trusted devices.
    pub fn get_trusted_devices(&self) -> Vec<TrustedDevice> {
        self.store.list()
    }

    /// Check whether a device identifier is currently trusted.
    pub fn is_device_trusted(&self, device_id: &str) -> bool {
        self.store.is_trusted(device_id)
    }

    /// Remove a device from the trusted store.
    pub fn remove_trusted_device(&self, device_id: &str) {
        self.store.remove(device_id);
        tracing::info!("Trusted device removed: {}", device_id);
    }

    /// Retrieve a specific trusted device by its identifier.
    pub fn get_trusted_device(&self, device_id: &str) -> Option<TrustedDevice> {
        self.store.get(device_id)
    }

    /// Update the `last_seen` timestamp for a trusted device to now.
    pub fn update_last_seen(&self, device_id: &str) {
        self.store.update_last_seen(device_id, Utc::now());
    }

    /// Generate a QR code PNG image containing the pairing data from a
    /// session.
    ///
    /// The encoded payload is a JSON-serialized [`QrPairingData`] struct.
    pub fn generate_qr_code(&self, session: &PairingSession) -> Result<Vec<u8>, PairingError> {
        let qr_data = QrPairingData {
            session_id: session.session_id.clone(),
            code: session.pairing_code.clone(),
            device_name: session.device_name.clone(),
            device_type: session.device_type.clone(),
            public_key: session.public_key.clone(),
        };

        let json = serde_json::to_string(&qr_data).map_err(|_| PairingError::QrGenerationFailed)?;

        let qr_code = QrCode::new(json.as_bytes()).map_err(|_| PairingError::QrGenerationFailed)?;

        let img = qr_code
            .render::<image::Luma<u8>>()
            .min_dimensions(300, 300)
            .build();

        let dyn_img = image::DynamicImage::ImageLuma8(img);
        let mut png_bytes = std::io::Cursor::new(Vec::new());
        dyn_img
            .write_to(&mut png_bytes, image::ImageFormat::Png)
            .map_err(|_| PairingError::QrGenerationFailed)?;

        Ok(png_bytes.into_inner())
    }

    /// Decode a QR code payload back into [`QrPairingData`].
    ///
    /// Supports two input formats:
    /// - Raw JSON bytes (useful for programmatic testing / non-QR channels).
    /// - PNG image bytes (requires a QR scanner library; currently falls
    ///   back to an error).
    pub fn decode_qr_code(data: &[u8]) -> Result<QrPairingData, PairingError> {
        if let Ok(qr_data) = serde_json::from_slice::<QrPairingData>(data) {
            return Ok(qr_data);
        }

        let _img = image::load_from_memory(data).map_err(|_| PairingError::QrGenerationFailed)?;

        Err(PairingError::QrGenerationFailed)
    }

    /// Generate a random 6-digit verification code (as a zero-padded string).
    pub fn generate_pairing_code() -> String {
        let code: u32 = rand::thread_rng().gen_range(100_000..1_000_000);
        format!("{:06}", code)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// PEM-encode a raw public key (32 bytes without a header).
fn encode_pem(key_bytes: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(key_bytes);
    let mut pem = String::from("-----BEGIN PUBLIC KEY-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        pem.push_str(&String::from_utf8_lossy(chunk));
        pem.push('\n');
    }
    pem.push_str("-----END PUBLIC KEY-----");
    pem
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use nova_security::SecurityManager;

    fn make_manager() -> PairingManager {
        let sec = Arc::new(SecurityManager::new("test-server"));
        PairingManager::new(sec)
    }

    fn expire_session(mgr: &PairingManager, session_id: &str) {
        let mut sessions = mgr.sessions.write();
        if let Some(s) = sessions.get_mut(session_id) {
            s.expires_at = Utc::now() - chrono::Duration::seconds(1);
        }
    }

    #[test]
    fn initiate_pairing_creates_session_with_valid_6_digit_code() {
        let mgr = make_manager();
        let session = mgr.initiate_pairing("dev-1", "Test Phone", "android");

        assert_eq!(session.device_id, "dev-1");
        assert_eq!(session.device_name, "Test Phone");
        assert_eq!(session.device_type, "android");
        assert_eq!(session.pairing_code.len(), 6);
        assert!(session.pairing_code.chars().all(|c| c.is_ascii_digit()));
        assert_eq!(session.state, PairingState::AwaitingCode);
        assert!(!session.public_key.is_empty());
    }

    #[test]
    fn verify_code_with_correct_code_succeeds() {
        let mgr = make_manager();
        let session = mgr.initiate_pairing("dev-1", "Test", "android");

        let result = mgr.verify_code(&session.session_id, &session.pairing_code);
        assert!(result.is_ok());
    }

    #[test]
    fn verify_code_with_wrong_code_fails() {
        let mgr = make_manager();
        let session = mgr.initiate_pairing("dev-1", "Test", "android");

        let result = mgr.verify_code(&session.session_id, "000000");
        assert_eq!(result, Err(PairingError::InvalidCode));
    }

    #[test]
    fn expired_session_rejected() {
        let mgr = make_manager();
        let session = mgr.initiate_pairing("dev-1", "Test", "android");

        expire_session(&mgr, &session.session_id);

        let result = mgr.verify_code(&session.session_id, &session.pairing_code);
        assert_eq!(result, Err(PairingError::CodeExpired));
    }

    #[test]
    fn key_exchange_round_trip() {
        let server_sec = Arc::new(SecurityManager::new("server"));
        let client_sec = SecurityManager::new("client");
        let mgr = PairingManager::new(server_sec.clone());

        let session = mgr.initiate_pairing("dev-1", "Test", "android");
        mgr.verify_code(&session.session_id, &session.pairing_code)
            .unwrap();

        let client_pub = client_sec.x25519_public_key_bytes();
        let (server_pub, shared_secret) =
            mgr.exchange_keys(&session.session_id, &client_pub).unwrap();

        assert_eq!(server_pub, server_sec.x25519_public_key_bytes().to_vec());
        assert_eq!(shared_secret.len(), 32);

        let client_shared = client_sec.generate_shared_secret(&server_pub).unwrap();
        assert_eq!(shared_secret, client_shared.to_vec());
    }

    #[test]
    fn confirm_pairing_adds_to_trusted_store() {
        let sec = Arc::new(SecurityManager::new("server"));
        let mgr = PairingManager::new(sec.clone());

        let session = mgr.initiate_pairing("dev-1", "Test", "android");
        mgr.verify_code(&session.session_id, &session.pairing_code)
            .unwrap();

        let peer_sec = SecurityManager::new("peer");
        let peer_pub = peer_sec.x25519_public_key_bytes();
        mgr.exchange_keys(&session.session_id, &peer_pub).unwrap();

        let device = mgr.confirm_pairing(&session.session_id).unwrap();
        assert_eq!(device.device_id, "dev-1");
        assert!(device.is_trusted);
        assert_eq!(device.public_key_bytes, peer_pub.to_vec());
        assert!(device
            .public_key_pem
            .starts_with("-----BEGIN PUBLIC KEY-----"));

        assert_eq!(mgr.get_trusted_devices().len(), 1);
    }

    #[test]
    fn reject_pairing_marks_session_rejected() {
        let mgr = make_manager();
        let session = mgr.initiate_pairing("dev-1", "Test", "android");

        mgr.reject_pairing(&session.session_id);

        let sessions = mgr.sessions.read();
        let s = sessions.get(&session.session_id).unwrap();
        assert_eq!(s.state, PairingState::Rejected);
    }

    #[test]
    fn trusted_device_store_add_remove_list() {
        let store = TrustedDeviceStore::new();
        assert_eq!(store.count(), 0);

        let device = TrustedDevice {
            device_id: "d1".to_string(),
            device_name: "Alpha".to_string(),
            device_type: "android".to_string(),
            public_key_pem: "pem-data".to_string(),
            public_key_bytes: vec![1, 2, 3],
            paired_at: Utc::now(),
            last_seen: Utc::now(),
            permissions: HashSet::new(),
            is_trusted: true,
        };

        store.add(device);
        assert_eq!(store.count(), 1);

        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].device_id, "d1");

        assert!(store.get("d1").is_some());
        assert!(store.get("missing").is_none());

        store.remove("d1");
        assert_eq!(store.count(), 0);
        assert!(store.list().is_empty());
    }

    #[test]
    fn is_device_trusted_works() {
        let sec = Arc::new(SecurityManager::new("server"));
        let mgr = PairingManager::new(sec.clone());

        assert!(!mgr.is_device_trusted("nonexistent"));

        let session = mgr.initiate_pairing("dev-1", "Test", "android");
        mgr.verify_code(&session.session_id, &session.pairing_code)
            .unwrap();

        let peer_sec = SecurityManager::new("peer");
        let peer_pub = peer_sec.x25519_public_key_bytes();
        mgr.exchange_keys(&session.session_id, &peer_pub).unwrap();
        mgr.confirm_pairing(&session.session_id).unwrap();

        assert!(mgr.is_device_trusted("dev-1"));
        assert!(!mgr.is_device_trusted("nonexistent"));

        mgr.remove_trusted_device("dev-1");
        assert!(!mgr.is_device_trusted("dev-1"));
    }

    #[test]
    fn qr_code_generation_returns_bytes() {
        let mgr = make_manager();
        let session = mgr.initiate_pairing("dev-1", "Test", "android");

        let qr_bytes = mgr.generate_qr_code(&session).unwrap();
        assert!(!qr_bytes.is_empty());

        // PNG header
        assert_eq!(qr_bytes[..8], [137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn pairing_code_is_always_6_digits() {
        for _ in 0..200 {
            let code = PairingManager::generate_pairing_code();
            assert_eq!(code.len(), 6, "code was: {code}");
            assert!(code.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn confirm_pairing_rejects_unverified_session() {
        let mgr = make_manager();
        let session = mgr.initiate_pairing("dev-1", "Test", "android");

        let result = mgr.confirm_pairing(&session.session_id);
        assert_eq!(result, Err(PairingError::PairingRejectedByUser));
    }

    #[test]
    fn exchange_keys_fails_for_expired_session() {
        let mgr = make_manager();
        let session = mgr.initiate_pairing("dev-1", "Test", "android");

        expire_session(&mgr, &session.session_id);

        let peer_pub = [0u8; 32];
        let result = mgr.exchange_keys(&session.session_id, &peer_pub);
        assert_eq!(result, Err(PairingError::CodeExpired));
    }

    #[test]
    fn qr_decode_accepts_raw_json() {
        let data = QrPairingData {
            session_id: "sess-1".into(),
            code: "123456".into(),
            device_name: "Phone".into(),
            device_type: "android".into(),
            public_key: vec![0xAB; 32],
        };

        let json = serde_json::to_vec(&data).unwrap();
        let decoded = PairingManager::decode_qr_code(&json).unwrap();
        assert_eq!(decoded.session_id, data.session_id);
        assert_eq!(decoded.code, data.code);
        assert_eq!(decoded.device_name, data.device_name);
        assert_eq!(decoded.public_key, data.public_key);
    }
}
