//! Host-owned encrypted envelopes for per-material data-encryption keys.
//!
//! The mounted bootstrap store is read-only input to this adapter. Envelopes,
//! receipts, and erasure fences are persisted only below the separate host
//! material-vault root.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use vestrace_application::{FenceReceipt, MaterialKeyVault, VaultError};
use vestrace_domain::trust::{KeyProvider, KeyReference, SecretResolutionRequest};
use vestrace_domain::{ErasureReceipt, IntentNonce, MaterialKeyId, VaultReceipt, ZeroizingDek};
use zeroize::Zeroize as _;

use super::{EnvelopeCipher, KEY_LENGTH, MountedSecretStoreKeyProvider, Sealed};

const MATERIAL_VAULT_AAD_DOMAIN: &str = "vestrace-material-vault-v1";

/// A filesystem-backed host vault. It keeps encrypted DEK envelopes and their
/// witnesses outside PostgreSQL and outside the mounted bootstrap store.
pub struct HostMaterialKeyVault {
    vault_root: PathBuf,
    bootstrap_provider: MountedSecretStoreKeyProvider,
    bootstrap_reference: KeyReference,
    bootstrap_request: SecretResolutionRequest,
    cipher: EnvelopeCipher,
}

#[derive(Deserialize, Serialize)]
struct StoredMaterialKey {
    nonce: uuid::Uuid,
    receipt: uuid::Uuid,
    envelope: Option<StoredEnvelope>,
    fence_receipt: Option<uuid::Uuid>,
    erasure_receipt: Option<uuid::Uuid>,
}

#[derive(Deserialize, Serialize)]
struct StoredEnvelope {
    nonce: [u8; 12],
    ciphertext: Vec<u8>,
}

impl HostMaterialKeyVault {
    /// Configures a vault root that is structurally separate from the mounted
    /// bootstrap store. The bootstrap reference is resolved only to unwrap a
    /// host-vault envelope; no material envelope can be written into the mount.
    pub fn new(
        vault_root: impl Into<PathBuf>,
        bootstrap_root: impl AsRef<Path>,
        bootstrap_reference: KeyReference,
        bootstrap_request: SecretResolutionRequest,
    ) -> Result<Self, VaultError> {
        let vault_root = vault_root.into();
        fs::create_dir_all(&vault_root).map_err(|_| VaultError::Unavailable)?;
        let vault_root = fs::canonicalize(vault_root).map_err(|_| VaultError::Unavailable)?;
        let bootstrap_root =
            fs::canonicalize(bootstrap_root.as_ref()).map_err(|_| VaultError::Unavailable)?;
        if vault_root.starts_with(&bootstrap_root) || bootstrap_root.starts_with(&vault_root) {
            return Err(VaultError::Unavailable);
        }

        Ok(Self {
            vault_root,
            bootstrap_provider: MountedSecretStoreKeyProvider::new(bootstrap_root),
            bootstrap_reference,
            bootstrap_request,
            cipher: EnvelopeCipher::new(),
        })
    }

    fn record_path(&self, key_id: MaterialKeyId) -> PathBuf {
        self.vault_root.join(format!("{}.json", key_id.as_uuid()))
    }

    fn read_record(&self, key_id: MaterialKeyId) -> Result<StoredMaterialKey, VaultError> {
        let bytes = fs::read(self.record_path(key_id)).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => VaultError::NotFound,
            _ => VaultError::Unavailable,
        })?;
        serde_json::from_slice(&bytes).map_err(|_| VaultError::Unavailable)
    }

    fn write_new_record(
        &self,
        key_id: MaterialKeyId,
        record: &StoredMaterialKey,
    ) -> Result<(), VaultError> {
        let bytes = serde_json::to_vec(record).map_err(|_| VaultError::Unavailable)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.record_path(key_id))
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::AlreadyExists => VaultError::Unavailable,
                _ => VaultError::Unavailable,
            })?;
        file.write_all(&bytes)
            .map_err(|_| VaultError::Unavailable)?;
        file.sync_all().map_err(|_| VaultError::Unavailable)
    }

    fn write_record(
        &self,
        key_id: MaterialKeyId,
        record: &StoredMaterialKey,
    ) -> Result<(), VaultError> {
        let bytes = serde_json::to_vec(record).map_err(|_| VaultError::Unavailable)?;
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(self.record_path(key_id))
            .map_err(|_| VaultError::Unavailable)?;
        file.write_all(&bytes)
            .map_err(|_| VaultError::Unavailable)?;
        file.sync_all().map_err(|_| VaultError::Unavailable)
    }

    fn bootstrap_key(&self) -> Result<[u8; KEY_LENGTH], VaultError> {
        let resolved = self
            .bootstrap_provider
            .resolve(&self.bootstrap_reference, &self.bootstrap_request)
            .map_err(|_| VaultError::Unavailable)?;
        let mut material = resolved.into_bytes();
        let digest = digest(&SHA256, &material);
        let mut key = [0_u8; KEY_LENGTH];
        key.copy_from_slice(digest.as_ref());
        material.zeroize();
        Ok(key)
    }

    fn associated_data(key_id: MaterialKeyId, nonce: IntentNonce) -> Vec<u8> {
        format!(
            "{MATERIAL_VAULT_AAD_DOMAIN}|{}|{}",
            key_id.as_uuid(),
            nonce.as_uuid()
        )
        .into_bytes()
    }

    fn create_record(
        &self,
        key_id: MaterialKeyId,
        nonce: IntentNonce,
    ) -> Result<StoredMaterialKey, VaultError> {
        let mut bootstrap_key = self.bootstrap_key()?;
        let mut dek = self
            .cipher
            .generate_data_key()
            .map_err(|_| VaultError::Unavailable)?;
        let sealed = self
            .cipher
            .seal(&bootstrap_key, &Self::associated_data(key_id, nonce), &dek)
            .map_err(|_| VaultError::Unavailable);
        bootstrap_key.zeroize();
        dek.zeroize();
        let sealed = sealed?;

        Ok(StoredMaterialKey {
            nonce: nonce.as_uuid(),
            receipt: VaultReceipt::new().as_uuid(),
            envelope: Some(StoredEnvelope {
                nonce: sealed.nonce,
                ciphertext: sealed.ciphertext,
            }),
            fence_receipt: None,
            erasure_receipt: None,
        })
    }
}

impl MaterialKeyVault for HostMaterialKeyVault {
    fn create_if_absent(
        &self,
        key_id: MaterialKeyId,
        nonce: IntentNonce,
    ) -> Result<VaultReceipt, VaultError> {
        match self.read_record(key_id) {
            Ok(record) => {
                if record.nonce == nonce.as_uuid() {
                    Ok(VaultReceipt::from_uuid(record.receipt))
                } else {
                    Err(VaultError::NonceMismatch)
                }
            }
            Err(VaultError::NotFound) => {
                let record = self.create_record(key_id, nonce)?;
                match self.write_new_record(key_id, &record) {
                    Ok(()) => Ok(VaultReceipt::from_uuid(record.receipt)),
                    Err(VaultError::Unavailable) => {
                        let replay = self.read_record(key_id)?;
                        if replay.nonce == nonce.as_uuid() {
                            Ok(VaultReceipt::from_uuid(replay.receipt))
                        } else {
                            Err(VaultError::NonceMismatch)
                        }
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    fn unwrap(
        &self,
        key_id: MaterialKeyId,
        use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        let record = self.read_record(key_id)?;
        if record.erasure_receipt.is_some() {
            return Err(VaultError::Erased);
        }
        if record.fence_receipt.is_some() {
            return Err(VaultError::ErasurePrepared);
        }
        let envelope = record.envelope.ok_or(VaultError::Erased)?;

        let mut bootstrap_key = self.bootstrap_key()?;
        let opened = self.cipher.open(
            &bootstrap_key,
            &Self::associated_data(key_id, IntentNonce::from_uuid(record.nonce)),
            &Sealed {
                nonce: envelope.nonce,
                ciphertext: envelope.ciphertext,
            },
        );
        bootstrap_key.zeroize();
        let mut plaintext = opened.map_err(|_| VaultError::Unavailable)?;
        let dek_bytes: Result<[u8; KEY_LENGTH], _> = plaintext.as_slice().try_into();
        plaintext.zeroize();
        let mut dek_bytes = dek_bytes.map_err(|_| VaultError::Unavailable)?;
        let dek = ZeroizingDek::new(dek_bytes);
        dek_bytes.zeroize();
        use_dek(&dek);
        drop(dek);
        Ok(())
    }

    fn prepare_erasure(&self, key_id: MaterialKeyId) -> Result<FenceReceipt, VaultError> {
        let mut record = self.read_record(key_id)?;
        if let Some(receipt) = record.fence_receipt {
            return Ok(FenceReceipt::from_uuid(receipt));
        }

        let receipt = FenceReceipt::new();
        record.fence_receipt = Some(receipt.as_uuid());
        self.write_record(key_id, &record)?;
        Ok(receipt)
    }

    fn erase(&self, key_id: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        let mut record = self.read_record(key_id)?;
        if let Some(receipt) = record.erasure_receipt {
            return Ok(ErasureReceipt::from_uuid(receipt));
        }
        if record.fence_receipt.is_none() {
            return Err(VaultError::ErasureNotPrepared);
        }

        if let Some(mut envelope) = record.envelope.take() {
            envelope.ciphertext.zeroize();
        }
        let receipt = ErasureReceipt::new();
        record.erasure_receipt = Some(receipt.as_uuid());
        self.write_record(key_id, &record)?;
        Ok(receipt)
    }
}
