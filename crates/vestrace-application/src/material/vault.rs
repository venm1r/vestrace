//! Narrow host-vault authority for per-material data-encryption keys.
//!
//! The callback form of [`MaterialKeyVault::unwrap`] makes a DEK available for
//! one operation only. The caller receives a borrow of a zeroizing container;
//! it cannot retain the vault-owned DEK in safe Rust after the callback returns.

#[path = "commands.rs"]
pub mod commands;
#[path = "erasure.rs"]
pub mod erasure;

use vestrace_domain::{ErasureReceipt, IntentNonce, MaterialKeyId, VaultReceipt, ZeroizingDek};

/// Durable host-vault witness that a material key has crossed the erasure fence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FenceReceipt(uuid::Uuid);

impl FenceReceipt {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    pub const fn from_uuid(value: uuid::Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}

impl Default for FenceReceipt {
    fn default() -> Self {
        Self::new()
    }
}

/// Refusals and availability failures from the host material-key vault.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum VaultError {
    #[error("material key is absent from the vault")]
    NotFound,
    #[error("material key is already reserved for a different nonce")]
    NonceMismatch,
    #[error("material key is erasure prepared")]
    ErasurePrepared,
    #[error("material key has been erased")]
    Erased,
    #[error("material key erase requires a prior erasure preparation")]
    ErasureNotPrepared,
    #[error("material vault is unavailable")]
    Unavailable,
}

/// The host-owned authority for material DEKs and their witnessed erasure fence.
pub trait MaterialKeyVault: Send + Sync {
    /// Creates the reserved key once, returning the original receipt on an exact replay.
    fn create_if_absent(
        &self,
        key_id: MaterialKeyId,
        nonce: IntentNonce,
    ) -> Result<VaultReceipt, VaultError>;

    /// Borrows a live DEK only for the supplied operation.
    ///
    /// An erasure-prepared key is refused before the adapter resolves bootstrap
    /// material, so the fence remains independent of PostgreSQL availability.
    fn unwrap(
        &self,
        key_id: MaterialKeyId,
        use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError>;

    /// Durably records the one-way erasure fence and returns its witness.
    fn prepare_erasure(&self, key_id: MaterialKeyId) -> Result<FenceReceipt, VaultError>;

    /// Physically removes the envelope only after the durable fence exists.
    fn erase(&self, key_id: MaterialKeyId) -> Result<ErasureReceipt, VaultError>;
}
