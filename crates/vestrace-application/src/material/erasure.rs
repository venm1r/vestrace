//! Two-phase, one-way material destruction.
//!
//! The repository methods are deliberately separate short transactions. The
//! synchronous host-vault call is run after phase one's commit and before the
//! finalizer's new transaction, so no PostgreSQL lock can span the vault call.

use std::sync::Arc;

use crate::{ApplicationError, MaterialErasureRepository, MaterialKeyVault, RequestContext};
use vestrace_domain::{
    ContentMaterialId, CredentialKeyCreationIntentId, ErasureReceipt, MaterialKeyId,
};

/// Orchestrates the host-vault fence/erase sequence around short guarded
/// database transactions. It holds neither a database connection nor a
/// transaction while the vault is called.
pub struct MaterialErasureService<R, V> {
    repository: R,
    vault: Arc<V>,
}

impl<R, V> MaterialErasureService<R, V>
where
    R: MaterialErasureRepository,
    V: MaterialKeyVault + 'static,
{
    pub fn new(repository: R, vault: V) -> Self {
        Self {
            repository,
            vault: Arc::new(vault),
        }
    }

    pub async fn erase_content(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<ErasureReceipt, ApplicationError> {
        let preparation = self
            .repository
            .prepare_content(context, material_id)
            .await?;
        if let Some(receipt) = preparation.finalized_receipt() {
            return Ok(receipt);
        }
        let fence = self.prepare_vault(preparation.material_key_id()).await?;
        self.repository
            .record_fence(context, preparation.id(), fence)
            .await?;
        let receipt = self.erase_vault(preparation.material_key_id()).await?;
        self.repository
            .finalize_content(context, preparation.id(), receipt)
            .await
    }

    pub async fn erase_credential(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<ErasureReceipt, ApplicationError> {
        let preparation = self
            .repository
            .prepare_credential(context, intent_id)
            .await?;
        if let Some(receipt) = preparation.finalized_receipt() {
            return Ok(receipt);
        }
        let fence = self.prepare_vault(preparation.material_key_id()).await?;
        self.repository
            .record_fence(context, preparation.id(), fence)
            .await?;
        let receipt = self.erase_vault(preparation.material_key_id()).await?;
        self.repository
            .finalize_credential(context, preparation.id(), receipt)
            .await
    }

    async fn prepare_vault(
        &self,
        material_key_id: MaterialKeyId,
    ) -> Result<crate::FenceReceipt, ApplicationError> {
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || {
            vault
                .prepare_erasure(material_key_id)
                .map_err(|error| ApplicationError::Unavailable(error.to_string()))
        })
        .await
        .map_err(|error| {
            ApplicationError::Internal(format!("material vault task failed: {error}"))
        })?
    }

    async fn erase_vault(
        &self,
        material_key_id: MaterialKeyId,
    ) -> Result<ErasureReceipt, ApplicationError> {
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || {
            vault
                .erase(material_key_id)
                .map_err(|error| ApplicationError::Unavailable(error.to_string()))
        })
        .await
        .map_err(|error| {
            ApplicationError::Internal(format!("material vault task failed: {error}"))
        })?
    }
}
