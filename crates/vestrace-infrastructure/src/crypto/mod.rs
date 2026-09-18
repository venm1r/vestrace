//! AES-256-GCM envelope encryption for workspace secrets.
//!
//! # Construction
//!
//! A master key (the KEK) wraps a per-workspace data key (the DEK); the DEK
//! encrypts individual secrets. Both layers use AES-256-GCM with a fresh random
//! 96-bit nonce per operation.
//!
//! # Custody
//!
//! This adapter reads its master key from process configuration, which for the
//! current deployment means a `.env` file. That is **local-development custody**
//! in the sense of [`CryptoCustody::LocalDevelopment`], and it does not pass
//! production qualification — [`crate::crypto::LOCAL_FILE_PROVIDER`] is the
//! provider name the qualification service refuses for production custody. A
//! process that can read its own environment can read the key, and so can
//! anyone who can read the file or the container's inspect output.
//!
//! What it does buy: the database no longer holds anything usable. A dump, a
//! replica, a backup or a SQL-injection read discloses names and sizes, not
//! secrets. That is the threat this is aimed at, and it is worth having before
//! a KMS exists — but it is not the same as a KMS.

use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};

/// The provider name recorded for keys held by this adapter.
///
/// `CryptoAdapterQualificationService` rejects this provider under any
/// production custody, so a deployment cannot present it as more than it is.
pub const LOCAL_FILE_PROVIDER: &str = "local-file";

/// The algorithm suite recorded alongside stored material, matching the default
/// declared by migration 0087.
pub const ALGORITHM_SUITE: &str = "AeadAes256GcmV1";

mod content_material_codec;
mod material_vault;
mod mounted_secret_store;
mod mounted_store_probe;

pub use content_material_codec::{
    ContentMaterialCodec, ContentMaterialCodecError, CredentialMaterialCodec,
    CredentialMaterialCodecError, CredentialMaterialContext, MAX_FRAMED_CREDENTIAL_BYTES,
    MAX_FRAMED_MATERIAL_BYTES, ValidatedContentMaterialFrame, ValidatedCredentialMaterialFrame,
};
pub use material_vault::HostMaterialKeyVault;
pub use mounted_secret_store::{
    KeyDeclaration, MOUNTED_SECRET_STORE_PROVIDER, MountedSecretStoreKeyProvider,
};
pub use mounted_store_probe::{MountedStoreCryptoProbe, discloses};

/// AES-256 takes a 256-bit key.
pub const KEY_LENGTH: usize = 32;

/// Length of an AES-GCM authentication tag.
pub const TAG_LENGTH: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// Deliberately says nothing about what was wrong with the key.
    #[error("master key is not {KEY_LENGTH} bytes of base64")]
    MalformedMasterKey,
    #[error("key version must not be blank")]
    BlankKeyVersion,
    /// Covers a wrong key, a tampered ciphertext, and a ciphertext moved to a
    /// different row. They are not distinguished, because telling a caller
    /// which of those happened tells an attacker how far they have got.
    #[error("secret could not be decrypted")]
    Undecryptable,
    #[error("random number generator unavailable")]
    RandomnessUnavailable,
    #[error("encryption failed")]
    EncryptionFailed,
}

/// 32 bytes of key material that never reaches a log.
pub struct MasterKey {
    bytes: [u8; KEY_LENGTH],
    version: String,
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The version is safe to show and is the useful half when diagnosing a
        // rotation; the material is never rendered.
        formatter
            .debug_struct("MasterKey")
            .field("bytes", &"[REDACTED]")
            .field("version", &self.version)
            .finish()
    }
}

impl Drop for MasterKey {
    fn drop(&mut self) {
        use zeroize::Zeroize as _;
        self.bytes.zeroize();
    }
}

impl MasterKey {
    /// Decode a base64-encoded 256-bit key.
    ///
    /// A key of any other length is refused rather than stretched or truncated:
    /// silently padding a short key would make a weak key look like a strong
    /// one, which is the failure mode that matters here.
    pub fn from_base64(encoded: &str, version: impl Into<String>) -> Result<Self, CryptoError> {
        use base64::Engine as _;

        let version = version.into();
        if version.trim().is_empty() {
            return Err(CryptoError::BlankKeyVersion);
        }
        let mut decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| CryptoError::MalformedMasterKey)?;
        // The intermediate buffer holds the key too, so it is wiped on every
        // path out of here — including the error path, where a wrong-length key
        // is still key material.
        let bytes: Result<[u8; KEY_LENGTH], _> = decoded.as_slice().try_into();
        {
            use zeroize::Zeroize as _;
            decoded.zeroize();
        }
        let bytes = bytes.map_err(|_| CryptoError::MalformedMasterKey)?;
        Ok(Self { bytes, version })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// Crate-internal on purpose: the key material must not become reachable
    /// from the HTTP or application layers, where it could be logged or
    /// serialised. Only the envelope adapter in this crate needs it.
    pub(crate) fn material(&self) -> &[u8; KEY_LENGTH] {
        &self.bytes
    }
}

/// A sealed value: ciphertext with the nonce it was produced under.
#[derive(Clone, Eq, PartialEq)]
pub struct Sealed {
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
}

impl std::fmt::Debug for Sealed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Ciphertext is not secret, but printing it invites someone to log a
        // whole record "for debugging" and then relax about the plaintext too.
        formatter
            .debug_struct("Sealed")
            .field("ciphertext_len", &self.ciphertext.len())
            .finish()
    }
}

/// Seals and opens values under AES-256-GCM.
///
/// # Nonce discipline
///
/// Every seal draws a fresh 96-bit nonce from the system CSPRNG. Reusing a
/// nonce under GCM leaks the XOR of the two plaintexts and, worse, allows
/// forgery of further ciphertexts — so nonces are never derived from a counter
/// that could restart, from a timestamp, or from the value being encrypted.
///
/// Random 96-bit nonces collide with non-negligible probability only after
/// roughly 2^32 encryptions under one key. The envelope keeps each data key
/// scoped to a single workspace's secrets, which stays many orders of magnitude
/// below that bound.
pub struct EnvelopeCipher {
    random: SystemRandom,
}

impl Default for EnvelopeCipher {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvelopeCipher {
    pub fn new() -> Self {
        Self {
            random: SystemRandom::new(),
        }
    }

    fn fresh_nonce(&self) -> Result<[u8; NONCE_LEN], CryptoError> {
        let mut nonce = [0u8; NONCE_LEN];
        self.random
            .fill(&mut nonce)
            .map_err(|_| CryptoError::RandomnessUnavailable)?;
        Ok(nonce)
    }

    /// Generate a fresh 256-bit data key.
    pub fn generate_data_key(&self) -> Result<[u8; KEY_LENGTH], CryptoError> {
        let mut key = [0u8; KEY_LENGTH];
        self.random
            .fill(&mut key)
            .map_err(|_| CryptoError::RandomnessUnavailable)?;
        Ok(key)
    }

    /// Encrypt `plaintext` under `key`, authenticating `associated_data`.
    ///
    /// The associated data is not stored in the ciphertext — it is reconstructed
    /// from the row on the way back in, so a ciphertext moved to a different
    /// workspace, secret or purpose fails to open instead of decrypting into the
    /// wrong context.
    pub fn seal(
        &self,
        key: &[u8; KEY_LENGTH],
        associated_data: &[u8],
        plaintext: &[u8],
    ) -> Result<Sealed, CryptoError> {
        let unbound =
            UnboundKey::new(&AES_256_GCM, key).map_err(|_| CryptoError::EncryptionFailed)?;
        let sealing_key = LessSafeKey::new(unbound);
        let nonce = self.fresh_nonce()?;

        let mut buffer = plaintext.to_vec();
        sealing_key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(associated_data),
                &mut buffer,
            )
            .map_err(|_| CryptoError::EncryptionFailed)?;

        Ok(Sealed {
            nonce,
            ciphertext: buffer,
        })
    }

    /// Decrypt, verifying both the tag and the associated data.
    pub fn open(
        &self,
        key: &[u8; KEY_LENGTH],
        associated_data: &[u8],
        sealed: &Sealed,
    ) -> Result<Vec<u8>, CryptoError> {
        let unbound = UnboundKey::new(&AES_256_GCM, key).map_err(|_| CryptoError::Undecryptable)?;
        let opening_key = LessSafeKey::new(unbound);

        let mut buffer = sealed.ciphertext.clone();
        let plaintext = opening_key
            .open_in_place(
                Nonce::assume_unique_for_key(sealed.nonce),
                Aad::from(associated_data),
                &mut buffer,
            )
            .map_err(|_| CryptoError::Undecryptable)?;
        Ok(plaintext.to_vec())
    }
}

/// Associated data binding a wrapped data key to its workspace and key version.
pub fn data_key_associated_data(workspace_id: uuid::Uuid, kek_version: &str) -> Vec<u8> {
    format!("vestrace:kek:v1|{workspace_id}|{kek_version}").into_bytes()
}

/// Associated data binding a secret's ciphertext to its identity.
///
/// Includes the purpose so a secret cannot be repurposed by updating one column:
/// changing `purpose` in the row makes the ciphertext unopenable.
pub fn secret_associated_data(
    workspace_id: uuid::Uuid,
    secret_id: uuid::Uuid,
    purpose: &str,
) -> Vec<u8> {
    format!("vestrace:secret:v1|{workspace_id}|{secret_id}|{purpose}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> [u8; KEY_LENGTH] {
        EnvelopeCipher::new().generate_data_key().unwrap()
    }

    #[test]
    fn a_sealed_value_opens_to_what_went_in() {
        let cipher = EnvelopeCipher::new();
        let key = key();
        let aad = b"context";
        let sealed = cipher.seal(&key, aad, b"sk-provider-secret").unwrap();
        assert_eq!(
            cipher.open(&key, aad, &sealed).unwrap(),
            b"sk-provider-secret"
        );
    }

    #[test]
    fn ciphertext_does_not_contain_the_plaintext() {
        let cipher = EnvelopeCipher::new();
        let sealed = cipher
            .seal(&key(), b"context", b"sk-provider-secret")
            .unwrap();
        assert!(
            !sealed
                .ciphertext
                .windows(6)
                .any(|window| window == b"sk-pro")
        );
    }

    #[test]
    fn every_seal_draws_a_fresh_nonce() {
        // A repeated nonce under GCM leaks the XOR of the plaintexts and enables
        // forgery, so this is the property the whole construction rests on.
        let cipher = EnvelopeCipher::new();
        let key = key();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..256 {
            let sealed = cipher
                .seal(&key, b"context", b"identical plaintext")
                .unwrap();
            assert!(seen.insert(sealed.nonce), "nonce reused");
        }
    }

    #[test]
    fn identical_plaintext_seals_to_different_ciphertext() {
        let cipher = EnvelopeCipher::new();
        let key = key();
        let first = cipher.seal(&key, b"context", b"same").unwrap();
        let second = cipher.seal(&key, b"context", b"same").unwrap();
        assert_ne!(first.ciphertext, second.ciphertext);
    }

    #[test]
    fn a_tampered_ciphertext_does_not_open() {
        let cipher = EnvelopeCipher::new();
        let key = key();
        let mut sealed = cipher
            .seal(&key, b"context", b"sk-provider-secret")
            .unwrap();
        sealed.ciphertext[0] ^= 0x01;
        assert!(matches!(
            cipher.open(&key, b"context", &sealed),
            Err(CryptoError::Undecryptable)
        ));
    }

    #[test]
    fn a_ciphertext_moved_to_another_context_does_not_open() {
        // This is what the associated data is for: relabelling the row must not
        // silently re-home the secret.
        let cipher = EnvelopeCipher::new();
        let key = key();
        let sealed = cipher
            .seal(&key, b"workspace-a", b"sk-provider-secret")
            .unwrap();
        assert!(matches!(
            cipher.open(&key, b"workspace-b", &sealed),
            Err(CryptoError::Undecryptable)
        ));
    }

    #[test]
    fn the_wrong_key_does_not_open() {
        let cipher = EnvelopeCipher::new();
        let sealed = cipher
            .seal(&key(), b"context", b"sk-provider-secret")
            .unwrap();
        assert!(matches!(
            cipher.open(&key(), b"context", &sealed),
            Err(CryptoError::Undecryptable)
        ));
    }

    #[test]
    fn associated_data_separates_workspaces_and_purposes() {
        let workspace = uuid::Uuid::from_u128(1);
        let other_workspace = uuid::Uuid::from_u128(2);
        let secret = uuid::Uuid::from_u128(3);
        assert_ne!(
            secret_associated_data(workspace, secret, "provider-api-key"),
            secret_associated_data(other_workspace, secret, "provider-api-key")
        );
        assert_ne!(
            secret_associated_data(workspace, secret, "provider-api-key"),
            secret_associated_data(workspace, secret, "export-signing")
        );
    }

    #[test]
    fn a_master_key_of_the_wrong_length_is_refused() {
        use base64::Engine as _;
        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 16]);
        assert!(matches!(
            MasterKey::from_base64(&short, "v1"),
            Err(CryptoError::MalformedMasterKey)
        ));
        let long = base64::engine::general_purpose::STANDARD.encode([0u8; 64]);
        assert!(matches!(
            MasterKey::from_base64(&long, "v1"),
            Err(CryptoError::MalformedMasterKey)
        ));
    }

    #[test]
    fn a_valid_master_key_decodes_and_never_prints_its_material() {
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode([7u8; KEY_LENGTH]);
        let master = MasterKey::from_base64(&encoded, "v1").unwrap();
        assert_eq!(master.version(), "v1");
        let rendered = format!("{master:?}");
        assert!(rendered.contains("[REDACTED]"));
        assert!(rendered.contains("v1"));
        assert!(!rendered.contains(&encoded));
    }

    #[test]
    fn a_blank_key_version_is_refused() {
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode([7u8; KEY_LENGTH]);
        assert!(matches!(
            MasterKey::from_base64(&encoded, "   "),
            Err(CryptoError::BlankKeyVersion)
        ));
    }

    #[test]
    fn errors_never_quote_the_material_they_failed_on() {
        let rendered = CryptoError::Undecryptable.to_string();
        assert_eq!(rendered, "secret could not be decrypted");
    }

    #[test]
    fn a_data_key_is_not_all_zeroes_and_differs_each_time() {
        let cipher = EnvelopeCipher::new();
        let first = cipher.generate_data_key().unwrap();
        let second = cipher.generate_data_key().unwrap();
        assert_ne!(first, [0u8; KEY_LENGTH]);
        assert_ne!(first, second);
    }

    #[test]
    fn a_wrapped_data_key_is_the_length_the_schema_constrains() {
        // Migration 0133 checks octet_length(wrapped_dek) = 48. If this changes,
        // that constraint rejects every write.
        let cipher = EnvelopeCipher::new();
        let sealed = cipher
            .seal(
                &key(),
                &data_key_associated_data(uuid::Uuid::nil(), "v1"),
                &key(),
            )
            .unwrap();
        assert_eq!(sealed.ciphertext.len(), KEY_LENGTH + TAG_LENGTH);
        assert_eq!(sealed.nonce.len(), NONCE_LEN);
    }
}
