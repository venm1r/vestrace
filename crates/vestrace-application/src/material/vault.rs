//! Narrow host-vault authority for per-material data-encryption keys.
//!
//! The callback form of [`MaterialKeyVault::unwrap`] makes a DEK available for
//! one operation only. The caller receives a borrow of a zeroizing container;
//! it cannot retain the vault-owned DEK in safe Rust after the callback returns.

#[path = "commands.rs"]
pub mod commands;
#[path = "erasure.rs"]
pub mod erasure;

use vestrace_domain::{
    ContentMaterialId, EmbeddingJobId, ErasureReceipt, IntentNonce, MaterialKeyCreationIntentId,
    MaterialKeyId, VaultReceipt, WorkspaceId, ZeroizingDek,
};

/// The immutable authority for a non-ordinary embedding output key.  It is
/// deliberately a value rather than a caller-controlled "bound" flag: 14C
/// may create the provisional envelope, but cannot promote or unwrap it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingOutputKeyBinding {
    pub workspace_id: WorkspaceId,
    pub job_id: EmbeddingJobId,
    pub intent_id: MaterialKeyCreationIntentId,
    pub material_id: ContentMaterialId,
    pub key_id: MaterialKeyId,
    pub nonce: IntentNonce,
    pub output_ordinal: u64,
}

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
    #[error("material key is reserved for an embedding output")]
    Provisional,
    #[error("material key provisional binding does not match")]
    BindingMismatch,
}

/// The host-owned authority for material DEKs and their witnessed erasure fence.
pub trait MaterialKeyVault: Send + Sync {
    /// Durably binds the exact provisional output to its committed preparation.
    /// Implementations must arbitrate this against provisional retirement.
    fn bind_embedding_output(
        &self,
        _binding: &EmbeddingOutputKeyBinding,
        _preparation: crate::EmbeddingResultPreparationId,
    ) -> Result<vestrace_domain::MaterialKeyBindingReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    /// Lends a bound key only after verifying the complete binding and witness.
    fn with_bound_embedding_output_key(
        &self,
        _binding: &EmbeddingOutputKeyBinding,
        _preparation: crate::EmbeddingResultPreparationId,
        _receipt: vestrace_domain::MaterialKeyBindingReceipt,
        _use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }

    /// Creates the reserved key once, returning the original receipt on an exact replay.
    fn create_if_absent(
        &self,
        key_id: MaterialKeyId,
        nonce: IntentNonce,
    ) -> Result<VaultReceipt, VaultError>;

    /// Creates an embedding-output provisional key.  Implementations must opt
    /// in explicitly: falling back to ordinary creation would make the key
    /// unwrap-capable before the later result-marker protocol exists.
    fn create_embedding_output_if_absent(
        &self,
        _binding: &EmbeddingOutputKeyBinding,
    ) -> Result<VaultReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    /// Retires the exact provisional key after the job-owned SQL authority has
    /// committed retirement.  It is separate from ordinary erasure so a generic
    /// reconciler cannot invent an embedding-output retirement route.
    fn retire_embedding_output(
        &self,
        _binding: &EmbeddingOutputKeyBinding,
    ) -> Result<ErasureReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    /// Lends an output key only to the result-preparation sealing callback.
    ///
    /// This is deliberately separate from [`Self::unwrap`]: the output key
    /// remains provisional until the later all-output bind authority commits.
    /// Implementations must reject every binding component that differs from
    /// the immutable provisional claim.
    fn with_embedding_output_key(
        &self,
        _binding: &EmbeddingOutputKeyBinding,
        _use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }

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
