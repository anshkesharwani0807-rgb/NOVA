//! At-rest encryption for the Memory Engine (Milestone 4).
//!
//! Sensitive record fields are sealed with AES-256-GCM (a pure-Rust AEAD, so the build
//! is reliable on every platform with no external crypto toolchain). The 32-byte key is
//! supplied by a [`KeyProvider`], which is the seam for future OS keychain integration
//! (Keystore on Android, DPAPI/Credential Manager on Windows). SQLCipher whole-file
//! encryption can later replace this layer behind the same abstraction without touching
//! the store or engine code.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use nova_kernel::{ErrorCategory, NovaError, Result};
use std::path::{Path, PathBuf};

const NONCE_LEN: usize = 12;

fn crypto_err(detail: &str) -> NovaError {
    NovaError::new(ErrorCategory::Storage, "ERR_MEM_CRYPTO", detail)
}

/// Supplies the 32-byte database key. Implementations must return the *same* key across
/// restarts so previously-encrypted data can be read back.
pub trait KeyProvider: Send + Sync {
    fn key(&self) -> Result<[u8; 32]>;
}

/// Interim file-backed key provider. The key is generated once (cryptographically random)
/// and persisted, so the database can be decrypted after a restart.
///
/// This is a placeholder for OS keychain storage; keeping the key in a file next to the
/// database is explicitly weaker than a hardware-backed keystore and is documented as
/// interim. The [`KeyProvider`] trait is the seam that lets the keychain replace it.
pub struct FileKeyProvider {
    path: PathBuf,
}

impl FileKeyProvider {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl KeyProvider for FileKeyProvider {
    fn key(&self) -> Result<[u8; 32]> {
        if self.path.exists() {
            let bytes = std::fs::read(&self.path).map_err(|e| {
                NovaError::new(ErrorCategory::Storage, "ERR_MEM_KEY_READ", &e.to_string())
            })?;
            if bytes.len() != 32 {
                return Err(crypto_err("key file has an invalid length"));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            Ok(key)
        } else {
            let key: [u8; 32] = rand::random();
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    NovaError::new(ErrorCategory::Storage, "ERR_MEM_KEY_WRITE", &e.to_string())
                })?;
            }
            std::fs::write(&self.path, key).map_err(|e| {
                NovaError::new(ErrorCategory::Storage, "ERR_MEM_KEY_WRITE", &e.to_string())
            })?;
            Ok(key)
        }
    }
}

/// A key provider that holds the key in memory (used for tests and ephemeral stores).
pub struct InMemoryKeyProvider {
    key: [u8; 32],
}

impl InMemoryKeyProvider {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key }
    }
}

impl KeyProvider for InMemoryKeyProvider {
    fn key(&self) -> Result<[u8; 32]> {
        Ok(self.key)
    }
}

/// AES-256-GCM sealer for individual field values. Ciphertext layout is `nonce || ct`.
pub struct Cipher {
    cipher: Aes256Gcm,
}

impl Cipher {
    pub fn new(key: &[u8; 32]) -> Self {
        let key = Key::<Aes256Gcm>::from_slice(key);
        Self {
            cipher: Aes256Gcm::new(key),
        }
    }

    /// Seal a plaintext value. Returns `nonce || ciphertext`.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let nonce_bytes: [u8; NONCE_LEN] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| crypto_err("encryption failed"))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Open a sealed value produced by [`Cipher::encrypt`].
    pub fn decrypt(&self, blob: &[u8]) -> Result<Vec<u8>> {
        if blob.len() < NONCE_LEN {
            return Err(crypto_err("ciphertext too short"));
        }
        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| crypto_err("decryption failed (wrong key or corrupt data)"))
    }

    pub fn encrypt_str(&self, s: &str) -> Result<Vec<u8>> {
        self.encrypt(s.as_bytes())
    }

    pub fn decrypt_str(&self, blob: &[u8]) -> Result<String> {
        let bytes = self.decrypt(blob)?;
        String::from_utf8(bytes).map_err(|_| crypto_err("decrypted value is not valid UTF-8"))
    }
}

/// Build a cipher from a key provider.
pub fn cipher_from(provider: &dyn KeyProvider) -> Result<Cipher> {
    Ok(Cipher::new(&provider.key()?))
}

/// Whether a key file already exists at `path` (used to reason about first-run setup).
pub fn key_exists(path: &Path) -> bool {
    path.exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_encrypt_decrypt() {
        let cipher = Cipher::new(&[7u8; 32]);
        let sealed = cipher.encrypt_str("hello world").unwrap();
        assert_ne!(sealed, b"hello world");
        assert_eq!(cipher.decrypt_str(&sealed).unwrap(), "hello world");
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let a = Cipher::new(&[1u8; 32]);
        let b = Cipher::new(&[2u8; 32]);
        let sealed = a.encrypt_str("secret").unwrap();
        assert!(b.decrypt_str(&sealed).is_err());
    }

    #[test]
    fn nonce_is_random_per_encryption() {
        let cipher = Cipher::new(&[9u8; 32]);
        let a = cipher.encrypt_str("same").unwrap();
        let b = cipher.encrypt_str("same").unwrap();
        assert_ne!(a, b, "ciphertexts must differ due to random nonces");
    }
}
