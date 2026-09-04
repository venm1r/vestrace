//! Commands for the material-key creation lifecycle.

use crate::{ApplicationError, MaterialIntentRepository, RequestContext};
use vestrace_domain::{
    ContentMaterialId, ErasureReceipt, IntentNonce, MaterialKeyBindingReceipt,
    MaterialKeyCreationIntent, MaterialKeyCreationIntentId, MaterialKeyCreationIntentState,
    MaterialKeyId, PreparedMaterialAttachmentId, SizeClass, VaultReceipt,
};

/// The one application-facing command surface for provisional content
/// material. Its repository port owns the database transition authority.
pub struct MaterialIntentCommands<R> {
    repository: R,
}

impl<R> MaterialIntentCommands<R>
where
    R: MaterialIntentRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn reserve(
        &self,
        context: &RequestContext,
        intent: &MaterialKeyCreationIntent,
    ) -> Result<(), ApplicationError> {
        self.repository.reserve(context, intent).await
    }

    pub async fn record_provisional_created(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        self.repository
            .record_provisional_created(context, intent_id)
            .await
    }

    pub async fn record_provisional_receipt(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        receipt: VaultReceipt,
    ) -> Result<(), ApplicationError> {
        self.repository
            .record_provisional_receipt(context, intent_id, receipt)
            .await
    }

    pub async fn prepare_content(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        attachment_id: PreparedMaterialAttachmentId,
        ciphertext: &[u8],
        size_class: SizeClass,
    ) -> Result<(), ApplicationError> {
        self.repository
            .prepare_content(context, intent_id, attachment_id, ciphertext, size_class)
            .await
    }

    pub async fn prepare_result(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        attachment_id: PreparedMaterialAttachmentId,
        ciphertext: &[u8],
        size_class: SizeClass,
    ) -> Result<(), ApplicationError> {
        self.repository
            .prepare_result(context, intent_id, attachment_id, ciphertext, size_class)
            .await
    }

    pub async fn bind(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        receipt: MaterialKeyBindingReceipt,
    ) -> Result<(), ApplicationError> {
        self.repository.bind(context, intent_id, receipt).await
    }

    pub async fn finalize_bound(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        self.repository.finalize_bound(context, intent_id).await
    }

    pub async fn prepare_content_abandon(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        self.repository
            .prepare_content_abandon(context, intent_id)
            .await
    }

    pub async fn prepare_pre_prepared_abandon(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        self.repository
            .prepare_pre_prepared_abandon(context, intent_id)
            .await
    }

    pub async fn record_unbound_erasure(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        receipt: ErasureReceipt,
    ) -> Result<(), ApplicationError> {
        self.repository
            .record_unbound_erasure(context, intent_id, receipt)
            .await
    }

    pub async fn finalize_abandon(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError> {
        self.repository.finalize_abandon(context, intent_id).await
    }

    pub async fn is_live(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<bool, ApplicationError> {
        self.repository.is_live(context, material_id).await
    }

    pub async fn snapshot(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<Option<MaterialIntentSnapshot>, ApplicationError> {
        self.repository.snapshot(context, intent_id).await
    }
}

/// What a crashed material-key creation intent left behind.
///
/// Every field is read back from the persisted row. A resumption cannot ask the
/// process that died what it held, and the plaintext it was encrypting is gone
/// with it, so completing a creation is possible only where the ciphertext was
/// already committed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaterialIntentSnapshot {
    pub state: MaterialKeyCreationIntentState,
    pub material_key_id: MaterialKeyId,
    pub nonce: IntentNonce,
    pub has_erasure_receipt: bool,
}

/// Where a resumption left one crashed intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResumptionOutcome {
    /// The intent was already terminal; the resumption changed nothing.
    AlreadyTerminal(MaterialKeyCreationIntentState),
    /// The resumption drove the intent to a lawful terminal.
    Terminal(MaterialKeyCreationIntentState),
    /// No lawful transition leads out of this state. The intent is parked, and
    /// `reason` says why rather than leaving a caller to infer success.
    Parked {
        state: MaterialKeyCreationIntentState,
        reason: &'static str,
    },
}

/// Resumes one crashed material-key creation intent.
///
/// This is the reconciler half of the crash contract: it re-drives the same
/// guarded transitions, never a private path, so it cannot create a second
/// identity or a second vault key. `create_if_absent` and the witnessed erase
/// are idempotent on their own identities, which is what makes replay safe.
pub struct MaterialIntentResumption<R, V> {
    commands: MaterialIntentCommands<R>,
    vault: std::sync::Arc<V>,
}

impl<R, V> MaterialIntentResumption<R, V>
where
    R: MaterialIntentRepository,
    V: crate::MaterialKeyVault + 'static,
{
    pub fn new(repository: R, vault: std::sync::Arc<V>) -> Self {
        Self {
            commands: MaterialIntentCommands::new(repository),
            vault,
        }
    }

    pub fn commands(&self) -> &MaterialIntentCommands<R> {
        &self.commands
    }

    pub async fn resume(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<ResumptionOutcome, ApplicationError> {
        let snapshot = self
            .commands
            .snapshot(context, intent_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::Storage("material key creation intent is absent".into())
            })?;

        match snapshot.state {
            // The ciphertext was never committed and its plaintext died with the
            // crashed process, so the creation cannot be completed. It is
            // retired through the pre-prepared abort — a branch distinct from
            // the ContentPrepared one, which the spec refuses to let stand in
            // for it.
            MaterialKeyCreationIntentState::Reserved
            | MaterialKeyCreationIntentState::ProvisionalCreated
            | MaterialKeyCreationIntentState::ProvisionalReceipted => {
                self.commands
                    .prepare_pre_prepared_abandon(context, intent_id)
                    .await?;
                self.witness_and_finalize(context, intent_id, snapshot)
                    .await
            }
            MaterialKeyCreationIntentState::PrePreparedAbandonPrepared => {
                self.witness_and_finalize(context, intent_id, snapshot)
                    .await
            }
            // Ciphertext exists but nothing bound it. Retire it through the
            // guarded abort branch and the witnessed unbound-key erase.
            MaterialKeyCreationIntentState::ContentPrepared => {
                self.commands
                    .prepare_content_abandon(context, intent_id)
                    .await?;
                self.witness_and_finalize(context, intent_id, snapshot)
                    .await
            }
            MaterialKeyCreationIntentState::ContentAbandonPrepared => {
                self.witness_and_finalize(context, intent_id, snapshot)
                    .await
            }
            // A ResultPrepared marker always binds; it never abandons.
            MaterialKeyCreationIntentState::ResultPrepared => Ok(ResumptionOutcome::Parked {
                state: snapshot.state,
                reason: "ResultPrepared always binds and never abandons; its bind receipt is \
                         owned by the job that crashed",
            }),
            // The bound receipt is committed, so the promotion is replayable.
            MaterialKeyCreationIntentState::Bound => {
                self.commands.finalize_bound(context, intent_id).await?;
                Ok(ResumptionOutcome::Terminal(
                    MaterialKeyCreationIntentState::Live,
                ))
            }
            terminal @ (MaterialKeyCreationIntentState::Live
            | MaterialKeyCreationIntentState::Abandoned
            | MaterialKeyCreationIntentState::ErasurePrepared
            | MaterialKeyCreationIntentState::Tombstoned) => {
                Ok(ResumptionOutcome::AlreadyTerminal(terminal))
            }
        }
    }

    /// Erases the unbound key and appends the terminal, both idempotently.
    ///
    /// The vault erase runs outside every database transaction, and a key the
    /// crashed process already erased returns its original receipt rather than
    /// starting a second erasure.
    async fn witness_and_finalize(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        snapshot: MaterialIntentSnapshot,
    ) -> Result<ResumptionOutcome, ApplicationError> {
        if !snapshot.has_erasure_receipt {
            let receipt = self
                .erase_unbound_key(snapshot.material_key_id, snapshot.nonce)
                .await?;
            self.commands
                .record_unbound_erasure(context, intent_id, receipt)
                .await?;
        }
        self.commands.finalize_abandon(context, intent_id).await?;
        Ok(ResumptionOutcome::Terminal(
            MaterialKeyCreationIntentState::Abandoned,
        ))
    }

    /// Erases the unbound key outside every database transaction.
    ///
    /// The pre-prepared branch can be entered from `Reserved`, where the crash
    /// may have landed on either side of the vault create and no receipt exists
    /// to say which. The reconciler resolves that by creating the key first:
    /// `create_if_absent` is idempotent on the exact `(key_id, nonce)` pair the
    /// intent reserved, so it returns the original key when one already exists
    /// and never mints a second. Erasing then has something definite to
    /// witness, which is the alternative to inventing a receipt for a key
    /// nobody can prove the state of.
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
