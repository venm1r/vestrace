//! Two-phase, one-way material destruction.
//!
//! The repository methods are deliberately separate short transactions. The
//! synchronous host-vault call is run after phase one's commit and before the
//! finalizer's new transaction, so no PostgreSQL lock can span the vault call.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    ApplicationError, MaterialErasurePreparation, MaterialErasureRepository, MaterialKeyVault,
    RequestContext,
};
use vestrace_domain::{
    ContentMaterialId, CredentialKeyCreationIntentId, ErasureReceipt, MaterialKeyId,
};

/// Phase one for a content material an embedding corpus may have been computed
/// from.
///
/// This exists because phase one is not the same transaction for such a source.
/// A corpus computed from the material has to stop answering from it before its
/// ciphertext can be destroyed, and the two facts have to commit together. The
/// implementation lives in `crate::embedding::erasure`; this port is here so
/// the two-phase authority can delegate to it without depending on the
/// embedding module.
///
/// `Ok(None)` means nothing embedded depends on this material and the ordinary
/// path owns it.
#[async_trait]
pub trait EmbeddingSourceErasurePropagator: Send + Sync {
    async fn propagate(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<Option<PropagatedErasure>, ApplicationError>;
}

/// What the propagation left for the two-phase authority to finish.
///
/// A vector of erased content is that content in another representation, so
/// destroying the source and keeping its embeddings would be the whole failure
/// this path exists to prevent. The propagation retires the projections that
/// named these materials, which is what makes them erasable at all, and this
/// carries them out for the caller holding the vault.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropagatedErasure {
    /// Every retired projection's own ciphertext, in the order the propagation
    /// recorded them. Each still needs its ordinary two-phase erasure.
    pub vectors: Vec<ContentMaterialId>,
    /// The source's own preparation, already made by the propagation.
    pub source: MaterialErasurePreparation,
}

/// Orchestrates the host-vault fence/erase sequence around short guarded
/// database transactions. It holds neither a database connection nor a
/// transaction while the vault is called.
pub struct MaterialErasureService<R, V> {
    repository: R,
    vault: Arc<V>,
    embedding: Option<Arc<dyn EmbeddingSourceErasurePropagator>>,
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
            embedding: None,
        }
    }

    /// Route content phase one through embedding erasure propagation.
    ///
    /// Deliberately opt-in rather than required. Without it the service still
    /// refuses to erase a source a live corpus depends on -- migration 0193
    /// records a nonterminal blocker per embedding delivery source and
    /// `vestrace_prepare_content_material_erasure` refuses while one exists --
    /// so an unwired deployment fails closed rather than erasing something out
    /// from under a corpus.
    pub fn with_embedding_propagation(
        mut self,
        propagator: Arc<dyn EmbeddingSourceErasurePropagator>,
    ) -> Self {
        self.embedding = Some(propagator);
        self
    }

    pub async fn erase_content(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<ErasureReceipt, ApplicationError> {
        // The propagator prepares inside its own transaction, so this must not
        // also call `prepare_content`: two preparations of one material is a
        // conflict the database is right to refuse.
        let propagated = match &self.embedding {
            Some(propagator) => propagator.propagate(context, material_id).await?,
            None => None,
        };
        let source = match propagated {
            Some(propagated) => {
                // The vectors go first. Both orders leave a window in which one
                // of the two still exists, and a vector of content that is
                // already gone is the worse of the two to be left holding.
                //
                // Their phase one is the ordinary one. The propagation did not
                // make it, because a vector only becomes erasable once its
                // projection is retired, and that is what the propagation just
                // did.
                for vector in propagated.vectors {
                    let prepared = self.repository.prepare_content(context, vector).await?;
                    self.finish_content(context, prepared).await?;
                }
                propagated.source
            }
            None => {
                self.repository
                    .prepare_content(context, material_id)
                    .await?
            }
        };
        self.finish_content(context, source).await
    }

    /// The vault sequence for one prepared content material.
    ///
    /// A preparation that already carries a receipt is a completed replay and
    /// must not reach the vault again: erasure is one-way and the receipt is
    /// the record that it happened once.
    async fn finish_content(
        &self,
        context: &RequestContext,
        preparation: MaterialErasurePreparation,
    ) -> Result<ErasureReceipt, ApplicationError> {
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
