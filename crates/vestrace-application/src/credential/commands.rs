//! Commands for the guarded credential-key creation lifecycle.

use crate::{ApplicationError, CredentialIntentRepository, RequestContext};
use vestrace_domain::{
    CredentialKeyCreationIntent, CredentialKeyCreationIntentId, CredentialKeyCreationIntentState,
    CredentialPreparedAttachmentId, CredentialRevisionId, ErasureReceipt, IntentNonce,
    MaterialKeyBindingReceipt, MaterialKeyId, VaultReceipt,
};

/// The application-facing command surface. It delegates every transition to a
/// database-backed guarded authority; it never publishes a Candidate itself.
pub struct CredentialIntentCommands<R> {
    repository: R,
}

impl<R> CredentialIntentCommands<R>
where
    R: CredentialIntentRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn reserve(
        &self,
        context: &RequestContext,
        intent: &CredentialKeyCreationIntent,
    ) -> Result<(), ApplicationError> {
        self.repository.reserve(context, intent).await
    }

    pub async fn record_provisional_created(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        self.repository
            .record_provisional_created(context, intent_id)
            .await
    }

    pub async fn record_provisional_receipt(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        receipt: VaultReceipt,
    ) -> Result<(), ApplicationError> {
        self.repository
            .record_provisional_receipt(context, intent_id, receipt)
            .await
    }

    pub async fn prepare(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        attachment_id: CredentialPreparedAttachmentId,
        ciphertext: &[u8],
    ) -> Result<(), ApplicationError> {
        self.repository
            .prepare(context, intent_id, attachment_id, ciphertext)
            .await
    }

    pub async fn bind(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        receipt: MaterialKeyBindingReceipt,
    ) -> Result<(), ApplicationError> {
        self.repository.bind(context, intent_id, receipt).await
    }

    pub async fn finalize_candidate(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        self.repository.finalize_candidate(context, intent_id).await
    }

    pub async fn prepare_abandon(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        expected_association_version: u64,
    ) -> Result<(), ApplicationError> {
        self.repository
            .prepare_abandon(context, intent_id, expected_association_version)
            .await
    }

    pub async fn record_unbound_erasure(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        receipt: ErasureReceipt,
    ) -> Result<(), ApplicationError> {
        self.repository
            .record_unbound_erasure(context, intent_id, receipt)
            .await
    }

    pub async fn finalize_abandon(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        self.repository.finalize_abandon(context, intent_id).await
    }

    pub async fn is_candidate(
        &self,
        context: &RequestContext,
        revision_id: CredentialRevisionId,
    ) -> Result<bool, ApplicationError> {
        self.repository.is_candidate(context, revision_id).await
    }

    pub async fn snapshot(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<Option<CredentialIntentSnapshot>, ApplicationError> {
        self.repository.snapshot(context, intent_id).await
    }
}

/// What a crashed credential-key creation intent left behind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialIntentSnapshot {
    pub state: CredentialKeyCreationIntentState,
    pub material_key_id: MaterialKeyId,
    pub nonce: IntentNonce,
    pub association_version: u64,
    pub has_erasure_receipt: bool,
}

/// Where a resumption left one crashed credential intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CredentialResumptionOutcome {
    AlreadyTerminal(CredentialKeyCreationIntentState),
    Terminal(CredentialKeyCreationIntentState),
    Parked {
        state: CredentialKeyCreationIntentState,
        reason: &'static str,
    },
}

/// Resumes one crashed credential-key creation intent.
///
/// Unlike the material lifecycle, the guarded pre-live abort here admits every
/// pre-live state, so a crash before binding always has a lawful terminal.
pub struct CredentialIntentResumption<R, V> {
    commands: CredentialIntentCommands<R>,
    vault: std::sync::Arc<V>,
}

impl<R, V> CredentialIntentResumption<R, V>
where
    R: CredentialIntentRepository,
    V: crate::MaterialKeyVault + 'static,
{
    pub fn new(repository: R, vault: std::sync::Arc<V>) -> Self {
        Self {
            commands: CredentialIntentCommands::new(repository),
            vault,
        }
    }

    pub fn commands(&self) -> &CredentialIntentCommands<R> {
        &self.commands
    }

    pub async fn resume(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<CredentialResumptionOutcome, ApplicationError> {
        let snapshot = self
            .commands
            .snapshot(context, intent_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Storage("credential key creation intent is absent".into())
            })?;

        match snapshot.state {
            // The secret the crashed process held is gone, so a pre-live intent
            // is retired rather than completed. The guarded abort admits every
            // one of these states, which is why none of them can strand.
            CredentialKeyCreationIntentState::Reserved
            | CredentialKeyCreationIntentState::ProvisionalCreated
            | CredentialKeyCreationIntentState::ProvisionalReceipted
            | CredentialKeyCreationIntentState::CredentialPrepared => {
                self.commands
                    .prepare_abandon(context, intent_id, snapshot.association_version)
                    .await?;
                self.witness_and_finalize(context, intent_id, snapshot)
                    .await
            }
            CredentialKeyCreationIntentState::CredentialAbandonPrepared => {
                self.witness_and_finalize(context, intent_id, snapshot)
                    .await
            }
            // The bind receipt is committed, so publication is replayable.
            CredentialKeyCreationIntentState::Bound => {
                self.commands.finalize_candidate(context, intent_id).await?;
                Ok(CredentialResumptionOutcome::Terminal(
                    CredentialKeyCreationIntentState::Candidate,
                ))
            }
            terminal @ (CredentialKeyCreationIntentState::Candidate
            | CredentialKeyCreationIntentState::Abandoned
            | CredentialKeyCreationIntentState::ErasurePrepared
            | CredentialKeyCreationIntentState::Destroyed) => {
                Ok(CredentialResumptionOutcome::AlreadyTerminal(terminal))
            }
        }
    }

    async fn witness_and_finalize(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        snapshot: CredentialIntentSnapshot,
    ) -> Result<CredentialResumptionOutcome, ApplicationError> {
        if !snapshot.has_erasure_receipt {
            let receipt = self
                .erase_unbound_key(snapshot.material_key_id, snapshot.nonce)
                .await?;
            self.commands
                .record_unbound_erasure(context, intent_id, receipt)
                .await?;
        }
        self.commands.finalize_abandon(context, intent_id).await?;
        Ok(CredentialResumptionOutcome::Terminal(
            CredentialKeyCreationIntentState::Abandoned,
        ))
    }

    /// Erases the reserved key outside every database transaction.
    ///
    /// A crash may have landed on either side of the vault create, and the
    /// reconciler cannot tell which. It resolves that by creating the key
    /// first: `create_if_absent` is idempotent on the exact `(key_id, nonce)`
    /// pair the intent reserved, so it returns the original key when one
    /// already exists and never mints a second. Erasing then has something
    /// definite to witness — the alternative would be inventing a receipt for
    /// a key nobody can prove the state of.
    ///
    /// The material lifecycle deliberately does not do this: it only reaches
    /// erasure from ContentPrepared, where a vault receipt is already
    /// committed, so an absent key there is a real inconsistency.
    async fn erase_unbound_key(
        &self,
        material_key_id: MaterialKeyId,
        nonce: IntentNonce,
    ) -> Result<ErasureReceipt, ApplicationError> {
        let vault = std::sync::Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || {
            vault
                .create_if_absent(material_key_id, nonce)
                .map_err(|error| ApplicationError::Unavailable(error.to_string()))?;
            match vault.prepare_erasure(material_key_id) {
                Ok(_) | Err(crate::VaultError::ErasurePrepared) => {}
                Err(error) => return Err(ApplicationError::Unavailable(error.to_string())),
            }
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
