//! M16 Cross-Device Security Module (nova_security).
//!
//! This crate provides the security primitives for the NOVA ecosystem:
//! certificate management, command signing, AES-256-GCM encryption via
//! X25519 key agreement, permission tokens, and key rotation.

#![doc(html_root_url = "https://docs.rs/nova_security/0.1.0")]

use std::collections::{HashMap, HashSet};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::Engine;
use chrono::{Duration, Utc};
use ed25519_dalek::{Signature as Ed25519Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use rand::rngs::OsRng as RandOsRng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;
use uuid::Uuid;
use x25519_dalek::{PublicKey as X25519PublicKey, SharedSecret, StaticSecret};
use zeroize::ZeroizeOnDrop;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Permission to access file system operations.
pub const PERM_FILES: &str = "nova.files";
/// Permission to read/write the clipboard.
pub const PERM_CLIPBOARD: &str = "nova.clipboard";
/// Permission to execute automation scripts.
pub const PERM_AUTOMATION: &str = "nova.automation";
/// Permission to access the camera.
pub const PERM_CAMERA: &str = "nova.camera";
/// Permission to access the microphone.
pub const PERM_MICROPHONE: &str = "nova.microphone";
/// Permission to send/receive notifications.
pub const PERM_NOTIFICATIONS: &str = "nova.notifications";
/// Permission to access the device gallery.
pub const PERM_GALLERY: &str = "nova.gallery";
/// Permission to read contacts.
pub const PERM_CONTACTS: &str = "nova.contacts";
/// Permission to read/send SMS.
pub const PERM_SMS: &str = "nova.sms";
/// Permission to manage calls.
pub const PERM_CALLS: &str = "nova.calls";
/// Permission to read battery status.
pub const PERM_BATTERY: &str = "nova.battery";
/// Permission to access storage.
pub const PERM_STORAGE: &str = "nova.storage";
/// Permission to execute arbitrary commands.
pub const PERM_EXECUTE: &str = "nova.execute";
/// Permission to read/write process memory.
pub const PERM_MEMORY: &str = "nova.memory";
/// Permission to capture screenshots.
pub const PERM_SCREENSHOT: &str = "nova.screenshot";

/// All standard permission names in a static slice.
pub const ALL_PERMISSIONS: &[&str] = &[
    PERM_FILES,
    PERM_CLIPBOARD,
    PERM_AUTOMATION,
    PERM_CAMERA,
    PERM_MICROPHONE,
    PERM_NOTIFICATIONS,
    PERM_GALLERY,
    PERM_CONTACTS,
    PERM_SMS,
    PERM_CALLS,
    PERM_BATTERY,
    PERM_STORAGE,
    PERM_EXECUTE,
    PERM_MEMORY,
    PERM_SCREENSHOT,
];

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can arise during security operations.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SecurityError {
    /// The provided cryptographic signature is invalid.
    #[error("Invalid signature")]
    InvalidSignature,
    /// The certificate has expired.
    #[error("Certificate has expired")]
    CertificateExpired,
    /// The certificate has been revoked.
    #[error("Certificate has been revoked")]
    CertificateRevoked,
    /// X25519 key agreement failed.
    #[error("Key exchange failed")]
    KeyExchangeFailed,
    /// Symmetric encryption failed.
    #[error("Encryption failed")]
    EncryptionFailed,
    /// Symmetric decryption failed.
    #[error("Decryption failed")]
    DecryptionFailed,
    /// The requested permission is not granted.
    #[error("Permission denied")]
    PermissionDenied,
    /// The requested key was not found.
    #[error("Key not found")]
    KeyNotFound,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A device certificate binding a device identity to its public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCertificate {
    /// Unique identifier for this certificate.
    pub id: String,
    /// The device this certificate belongs to.
    pub device_id: String,
    /// The PEM-encoded ed25519 public key.
    pub public_key_pem: String,
    /// When the certificate was issued.
    pub issued_at: chrono::DateTime<Utc>,
    /// When the certificate expires.
    pub expires_at: chrono::DateTime<Utc>,
    /// Whether the certificate has been revoked.
    pub is_revoked: bool,
}

/// An X25519 key pair with automatic zeroing of the secret component on drop.
#[derive(ZeroizeOnDrop)]
pub struct KeyPair {
    /// The secret (private) key – zeroized on drop.
    pub secret_key: StaticSecret,
    /// The corresponding public key.
    pub public_key: X25519PublicKey,
}

impl std::fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyPair")
            .field("secret_key", &"[REDACTED]")
            .field("public_key", &self.public_key.to_bytes())
            .finish()
    }
}

/// A signed message produced by a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    /// The original message data.
    pub data: Vec<u8>,
    /// Timestamp of when the signature was created.
    pub timestamp: chrono::DateTime<Utc>,
    /// The device that created the signature.
    pub device_id: String,
    /// The ed25519 signature bytes.
    pub signature_bytes: Vec<u8>,
}

/// A time-limited permission token issued to a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionToken {
    /// Unique token identifier.
    pub token_id: String,
    /// The target device.
    pub device_id: String,
    /// The set of granted permission names.
    pub permissions: HashSet<String>,
    /// When the token was issued.
    pub issued_at: chrono::DateTime<Utc>,
    /// When the token expires.
    pub expires_at: chrono::DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// SecurityManager
// ---------------------------------------------------------------------------

/// Central security manager that handles signing, encryption, certificates,
/// permission tokens, and key rotation.
///
/// # Example
///
/// ```ignore
/// use nova_security::SecurityManager;
/// let mut mgr = SecurityManager::new("device-01");
/// let sig = mgr.sign(b"hello");
/// assert!(mgr.verify(b"hello", &sig, mgr.public_key_bytes()));
/// ```
pub struct SecurityManager {
    device_id: String,
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
    x25519_secret: StaticSecret,
    x25519_public: X25519PublicKey,
    old_signing_key: Option<(SigningKey, VerifyingKey)>,
    old_x25519_secret: Option<StaticSecret>,
    revoked_devices: HashSet<String>,
}

impl SecurityManager {
    /// Create a new `SecurityManager` that generates fresh cryptographic keys.
    pub fn new(device_id: &str) -> Self {
        let mut rng = RandOsRng;
        let mut seed = [0u8; 32];
        use rand::RngCore;
        rng.fill_bytes(&mut seed);
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let x25519_secret = StaticSecret::random_from_rng(rng);
        let x25519_public = X25519PublicKey::from(&x25519_secret);

        Self {
            device_id: device_id.to_string(),
            signing_key,
            verifying_key,
            x25519_secret,
            x25519_public,
            old_signing_key: None,
            old_x25519_secret: None,
            revoked_devices: HashSet::new(),
        }
    }

    // -- Accessors --------------------------------------------------------

    /// Return the owning device identifier.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Return the current ed25519 public key as a byte array.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    /// Return the current x25519 public key as a byte array.
    pub fn x25519_public_key_bytes(&self) -> [u8; 32] {
        self.x25519_public.to_bytes()
    }

    /// Return the old ed25519 public key (if any) during the grace period.
    pub fn old_public_key_bytes(&self) -> Option<[u8; 32]> {
        self.old_signing_key.as_ref().map(|(_, vk)| vk.to_bytes())
    }

    // -- Signing ----------------------------------------------------------

    /// Sign `data` with the current ed25519 signing key.
    ///
    /// Returns the raw 64-byte signature.
    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        let sig: Ed25519Signature = self.signing_key.sign(data);
        sig.to_bytes().to_vec()
    }

    /// Verify an ed25519 `signature` against `data` using the provided
    /// `public_key` (32 bytes).
    pub fn verify(&self, data: &[u8], signature: &[u8], public_key: &[u8]) -> bool {
        let pk_bytes: &[u8; 32] = match public_key.try_into() {
            Ok(b) => b,
            Err(_) => return false,
        };
        let pk = match VerifyingKey::from_bytes(pk_bytes) {
            Ok(k) => k,
            Err(_) => return false,
        };
        let sig = match Ed25519Signature::from_slice(signature) {
            Ok(s) => s,
            Err(_) => return false,
        };
        pk.verify(data, &sig).is_ok()
    }

    // -- Encryption -------------------------------------------------------

    /// AES-256-GCM encrypt `plaintext` for a peer identified by their X25519
    /// public key.  Returns `(nonce || ciphertext)` on success.
    pub fn encrypt(
        &self,
        plaintext: &[u8],
        peer_public_key: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        let shared = self.generate_shared_secret(peer_public_key)?;
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&shared);
        let cipher = Aes256Gcm::new(key);

        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| SecurityError::EncryptionFailed)?;

        let mut out = Vec::with_capacity(12 + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypt a message produced by [`encrypt`](Self::encrypt).
    ///
    /// Expects `ciphertext` to be `(12-byte nonce || encrypted payload)`.
    pub fn decrypt(
        &self,
        ciphertext: &[u8],
        peer_public_key: &[u8],
    ) -> Result<Vec<u8>, SecurityError> {
        if ciphertext.len() < 13 {
            return Err(SecurityError::DecryptionFailed);
        }
        let shared = self.generate_shared_secret(peer_public_key)?;
        let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&shared);
        let cipher = Aes256Gcm::new(key);

        let (nonce_bytes, ct) = ciphertext.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        cipher
            .decrypt(nonce, ct)
            .map_err(|_| SecurityError::DecryptionFailed)
    }

    /// Derive a 32-byte shared secret via X25519 Diffie-Hellman followed by
    /// HKDF-SHA256 key expansion.
    pub fn generate_shared_secret(&self, peer_public: &[u8]) -> Result<[u8; 32], SecurityError> {
        let peer_bytes: &[u8; 32] = peer_public
            .try_into()
            .map_err(|_| SecurityError::KeyExchangeFailed)?;
        let peer_pub = X25519PublicKey::from(*peer_bytes);

        let shared: SharedSecret = self.x25519_secret.diffie_hellman(&peer_pub);

        let hkdf = Hkdf::<Sha256>::new(None, shared.as_bytes());
        let mut okm = [0u8; 32];
        hkdf.expand(b"nova-x25519-key", &mut okm)
            .map_err(|_| SecurityError::KeyExchangeFailed)?;
        Ok(okm)
    }

    // -- Certificates -----------------------------------------------------

    /// Create a self-signed `DeviceCertificate` for the given `device_id`
    /// that is valid for `validity_days`.
    pub fn create_certificate(&self, device_id: &str, validity_days: u64) -> DeviceCertificate {
        let pem = encode_pem(&self.verifying_key.to_bytes());
        DeviceCertificate {
            id: Uuid::new_v4().to_string(),
            device_id: device_id.to_string(),
            public_key_pem: pem,
            issued_at: Utc::now(),
            expires_at: Utc::now() + Duration::days(validity_days as i64),
            is_revoked: false,
        }
    }

    /// Verify that a certificate is not expired and has not been revoked.
    pub fn verify_certificate(&self, cert: &DeviceCertificate) -> Result<(), SecurityError> {
        if Utc::now() > cert.expires_at {
            return Err(SecurityError::CertificateExpired);
        }
        if cert.is_revoked || self.revoked_devices.contains(&cert.device_id) {
            return Err(SecurityError::CertificateRevoked);
        }
        Ok(())
    }

    /// Revoke all certificates belonging to `device_id`.
    pub fn revoke_certificate(&mut self, device_id: &str) {
        self.revoked_devices.insert(device_id.to_string());
    }

    // -- Permission tokens ------------------------------------------------

    /// Issue a `PermissionToken` for `device_id` with the given permissions
    /// that expires after `validity_hours`.
    pub fn issue_permission_token(
        &self,
        device_id: &str,
        permissions: HashSet<String>,
        validity_hours: u64,
    ) -> PermissionToken {
        PermissionToken {
            token_id: Uuid::new_v4().to_string(),
            device_id: device_id.to_string(),
            permissions,
            issued_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(validity_hours as i64),
        }
    }

    /// Verify that `token` grants the required permission and has not
    /// expired.
    pub fn verify_permission(
        &self,
        token: &PermissionToken,
        required: &str,
    ) -> Result<(), SecurityError> {
        if Utc::now() > token.expires_at {
            return Err(SecurityError::CertificateExpired);
        }
        if !token.permissions.contains(required) {
            return Err(SecurityError::PermissionDenied);
        }
        Ok(())
    }

    // -- Key rotation -----------------------------------------------------

    /// Rotate the cryptographic keys.
    ///
    /// The previous keys are retained so that signatures created before the
    /// rotation can still be verified (grace period).  Call
    /// [`old_public_key_bytes`](Self::old_public_key_bytes) to retrieve the
    /// previous ed25519 public key.
    pub fn rotate_keys(&mut self) {
        let mut rng = RandOsRng;
        use rand::RngCore;

        // Rotate ed25519
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);
        let old_signing = std::mem::replace(&mut self.signing_key, SigningKey::from_bytes(&seed));
        let old_verifying = self.verifying_key;
        self.old_signing_key = Some((old_signing, old_verifying));
        self.verifying_key = self.signing_key.verifying_key();

        // Rotate x25519
        let old_x25519 =
            std::mem::replace(&mut self.x25519_secret, StaticSecret::random_from_rng(rng));
        self.old_x25519_secret = Some(old_x25519);
        self.x25519_public = X25519PublicKey::from(&self.x25519_secret);
    }
}

// ---------------------------------------------------------------------------
// PermissionManager
// ---------------------------------------------------------------------------

/// Manages device-level permission assignments independently of the
/// cryptographic token system.
///
/// # Example
///
/// ```ignore
/// use nova_security::PermissionManager;
/// let mut pm = PermissionManager::new();
/// let perms = [("nova.files".into(), true)].into();
/// pm.set_device_permissions("device-01", perms);
/// assert!(pm.check_permission("device-01", "nova.files"));
/// ```
pub struct PermissionManager {
    store: HashMap<String, HashMap<String, bool>>,
}

impl PermissionManager {
    /// Create an empty permission store.
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }

    /// Set (or replace) the permissions for a device.
    pub fn set_device_permissions(&mut self, device_id: &str, permissions: HashMap<String, bool>) {
        self.store.insert(device_id.to_string(), permissions);
    }

    /// Check whether a device has a specific permission.
    ///
    /// Returns `false` if the device is unknown or the permission is not
    /// explicitly granted.
    pub fn check_permission(&self, device_id: &str, permission: &str) -> bool {
        self.store
            .get(device_id)
            .and_then(|perms| perms.get(permission))
            .copied()
            .unwrap_or(false)
    }

    /// Remove all permissions for a device.
    pub fn revoke_device(&mut self, device_id: &str) {
        self.store.remove(device_id);
    }

    /// Return a copy of the permissions for a device (empty map if unknown).
    pub fn list_permissions(&self, device_id: &str) -> HashMap<String, bool> {
        self.store.get(device_id).cloned().unwrap_or_default()
    }

    /// Return the list of all device IDs that currently have permissions.
    pub fn all_devices(&self) -> Vec<String> {
        self.store.keys().cloned().collect()
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// PEM-encode a 32-byte public key.
fn encode_pem(key_bytes: &[u8; 32]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(key_bytes);
    let mut pem = String::from("-----BEGIN PUBLIC KEY-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        pem.push_str(&String::from_utf8_lossy(chunk));
        pem.push('\n');
    }
    pem.push_str("-----END PUBLIC KEY-----");
    pem
}

/// PEM-decode a public key.
#[allow(dead_code)]
fn decode_pem(pem: &str) -> Result<[u8; 32], SecurityError> {
    let inner = pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect::<Vec<_>>()
        .concat();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(inner)
        .map_err(|_| SecurityError::KeyNotFound)?;
    let arr: [u8; 32] = bytes.try_into().map_err(|_| SecurityError::KeyNotFound)?;
    Ok(arr)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_constants_are_non_empty() {
        for &p in ALL_PERMISSIONS {
            assert!(!p.is_empty(), "constant {:?} must not be empty", p);
        }
    }

    #[allow(clippy::needless_borrows_for_generic_args)]
    #[test]
    fn key_generation_creates_distinct_keys() {
        let mut rng = RandOsRng;
        let sk1 = StaticSecret::random_from_rng(&mut rng);
        let pk1 = X25519PublicKey::from(&sk1);
        let sk2 = StaticSecret::random_from_rng(&mut rng);
        let pk2 = X25519PublicKey::from(&sk2);

        assert_ne!(sk1.to_bytes(), sk2.to_bytes());
        assert_ne!(pk1.to_bytes(), pk2.to_bytes());
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let mgr = SecurityManager::new("test-device");
        let data = b"NOVA cross-device protocol";
        let sig = mgr.sign(data);
        assert!(mgr.verify(data, &sig, &mgr.public_key_bytes()));
    }

    #[test]
    fn wrong_key_fails_verification() {
        let alice = SecurityManager::new("alice");
        let bob = SecurityManager::new("bob");
        let data = b"secret message";
        let sig = alice.sign(data);
        assert!(!bob.verify(data, &sig, &bob.public_key_bytes()));
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let alice = SecurityManager::new("alice");
        let bob = SecurityManager::new("bob");

        let plaintext = b"Hello from Alice to Bob!";
        let ct = alice
            .encrypt(plaintext, &bob.x25519_public_key_bytes())
            .expect("encryption should succeed");
        let decrypted = bob
            .decrypt(&ct, &alice.x25519_public_key_bytes())
            .expect("decryption should succeed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_key_fails_decrypt() {
        let alice = SecurityManager::new("alice");
        let bob = SecurityManager::new("bob");
        let eve = SecurityManager::new("eve");

        let plaintext = b"intercepted message";
        let ct = alice
            .encrypt(plaintext, &bob.x25519_public_key_bytes())
            .expect("encryption should succeed");

        // Eve tries to decrypt – should fail.
        let result = eve.decrypt(&ct, &alice.x25519_public_key_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn certificate_creation_and_verification() {
        let mgr = SecurityManager::new("device-42");
        let cert = mgr.create_certificate("device-42", 30);
        assert_eq!(cert.device_id, "device-42");
        assert!(mgr.verify_certificate(&cert).is_ok());
    }

    #[test]
    fn expired_certificate_rejected() {
        let mgr = SecurityManager::new("device-x");
        let mut cert = mgr.create_certificate("device-x", 30);
        // Artificially expire the certificate.
        cert.expires_at = Utc::now() - Duration::seconds(1);
        assert_eq!(
            mgr.verify_certificate(&cert),
            Err(SecurityError::CertificateExpired)
        );
    }

    #[test]
    fn revoked_certificate_rejected() {
        let mut mgr = SecurityManager::new("device-y");
        let cert = mgr.create_certificate("device-y", 30);
        mgr.revoke_certificate("device-y");
        assert_eq!(
            mgr.verify_certificate(&cert),
            Err(SecurityError::CertificateRevoked)
        );
    }

    #[test]
    fn permission_token_issue_and_verify() {
        let mgr = SecurityManager::new("issuer");
        let mut perms = HashSet::new();
        perms.insert(PERM_FILES.to_string());
        perms.insert(PERM_CLIPBOARD.to_string());

        let token = mgr.issue_permission_token("device-target", perms, 24);
        assert!(mgr.verify_permission(&token, PERM_FILES).is_ok());
        assert!(mgr.verify_permission(&token, PERM_CLIPBOARD).is_ok());
    }

    #[test]
    fn expired_token_rejected() {
        let mgr = SecurityManager::new("issuer");
        let mut perms = HashSet::new();
        perms.insert(PERM_AUTOMATION.to_string());

        let mut token = mgr.issue_permission_token("device-target", perms, 1);
        token.expires_at = Utc::now() - Duration::seconds(1);
        assert_eq!(
            mgr.verify_permission(&token, PERM_AUTOMATION),
            Err(SecurityError::CertificateExpired)
        );
    }

    #[test]
    fn permission_denied_returns_error() {
        let mgr = SecurityManager::new("issuer");
        let perms = HashSet::new(); // empty
        let token = mgr.issue_permission_token("device-target", perms, 24);
        assert_eq!(
            mgr.verify_permission(&token, PERM_FILES),
            Err(SecurityError::PermissionDenied)
        );
    }

    #[test]
    fn permission_manager_set_and_check() {
        let mut pm = PermissionManager::new();
        let mut perms = HashMap::new();
        perms.insert(PERM_FILES.to_string(), true);
        perms.insert(PERM_CAMERA.to_string(), false);

        pm.set_device_permissions("phone", perms);
        assert!(pm.check_permission("phone", PERM_FILES));
        assert!(!pm.check_permission("phone", PERM_CAMERA));
    }

    #[test]
    fn revoke_device_removes_permissions() {
        let mut pm = PermissionManager::new();
        let mut perms = HashMap::new();
        perms.insert(PERM_STORAGE.to_string(), true);
        pm.set_device_permissions("tablet", perms);

        assert!(pm.check_permission("tablet", PERM_STORAGE));
        pm.revoke_device("tablet");
        assert!(!pm.check_permission("tablet", PERM_STORAGE));
        assert!(pm.list_permissions("tablet").is_empty());
    }

    #[test]
    fn key_rotation_generates_new_keys() {
        let mut mgr = SecurityManager::new("rotator");
        let old_pk = mgr.public_key_bytes();
        let old_xpk = mgr.x25519_public_key_bytes();

        mgr.rotate_keys();

        assert_ne!(mgr.public_key_bytes(), old_pk);
        assert_ne!(mgr.x25519_public_key_bytes(), old_xpk);
    }

    #[test]
    fn key_rotation_still_verifies_old_signatures_during_grace_period() {
        let mut mgr = SecurityManager::new("grace");
        let data = b"hello from the past";

        // Sign with the original key.
        let sig = mgr.sign(data);
        let old_pk = mgr.public_key_bytes();

        // Rotate keys.
        mgr.rotate_keys();

        // The old signature should still verify with the old public key.
        assert!(mgr.verify(data, &sig, &old_pk));

        // The new key / old signature combination should NOT verify.
        assert!(!mgr.verify(data, &sig, &mgr.public_key_bytes()));
    }
}
