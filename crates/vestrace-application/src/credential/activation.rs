//! Governed first activation, rotation, and revocation of credential slots.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use vestrace_domain::{
    AuditEvent, ConnectionId, CredentialKeyCreationIntentId, CredentialPreparedAttachmentId,
    CredentialRevisionId, CredentialSlotId, ErasureReceipt, IntentNonce, MaterialKeyId,
    WorkspaceId,
};
use zeroize::Zeroizing;

use crate::{
    ApplicationError, ConnectionAuth, FenceReceipt, GovernedMutationReceipt, IdempotencyRecord,
    MaterialErasurePreparation, MaterialErasureRepository, MaterialKeyVault, OutboxMessage,
    RequestContext, UnitOfWork,
};

/// Exact, normalized input for one credential lease issued after an effect was
/// authorized and before its provider dispatch begins.
#[derive(Clone, Debug)]
pub struct CredentialDispatchLeaseRequest {
    pub lease_id: uuid::Uuid,
    pub connection_id: ConnectionId,
    pub external_effect_id: uuid::Uuid,
    pub authorization_id: uuid::Uuid,
    pub credential_slot_id: CredentialSlotId,
    pub credential_revision_id: uuid::Uuid,
    pub credential_activation_guard_id: uuid::Uuid,
    pub destination_authority: String,
    pub auth_mode: String,
    pub expires_at: DateTime<Utc>,
}

/// A database-pinned credential lease. It carries no credential plaintext.
#[derive(Clone, Debug)]
pub struct CredentialDispatchLease {
    pub id: uuid::Uuid,
    pub workspace_id: WorkspaceId,
    pub connection_id: ConnectionId,
    pub external_effect_id: uuid::Uuid,
    pub authorization_id: uuid::Uuid,
    pub credential_slot_id: CredentialSlotId,
    pub credential_revision_id: uuid::Uuid,
    pub credential_activation_guard_id: uuid::Uuid,
    pub destination_authority: String,
    pub auth_mode: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub terminal_state: Option<String>,
}

/// Operator-supplied credential bytes owned by one preparation attempt.
///
/// This value is intentionally non-`Clone`; dropping it clears the allocation.
pub struct OperatorCredential(Zeroizing<Vec<u8>>);

impl OperatorCredential {
    pub fn new(value: impl AsRef<[u8]>) -> Self {
        Self(Zeroizing::new(value.as_ref().to_vec()))
    }

    pub fn expose(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl std::fmt::Debug for OperatorCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OperatorCredential([REDACTED])")
    }
}

/// Exact already-reserved lifecycle tuple for credential material preparation.
pub struct CredentialMaterialPreparationRequest {
    pub connection_id: ConnectionId,
    pub credential_slot_id: CredentialSlotId,
    pub credential_revision_id: CredentialRevisionId,
    pub material_key_id: MaterialKeyId,
    pub intent_id: CredentialKeyCreationIntentId,
    pub intent_nonce: IntentNonce,
    pub prepared_attachment_id: CredentialPreparedAttachmentId,
    pub credential: OperatorCredential,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialMaterialPreparationState {
    CredentialPrepared,
    Bound,
    Candidate,
    Active,
    Retired,
    Revoked,
    CredentialAbandonPrepared,
    ErasurePrepared,
    Destroyed,
    Abandoned,
}

/// Opaque lifecycle result; it never carries plaintext, ciphertext, or a DEK.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialMaterialPreparation {
    pub intent_id: CredentialKeyCreationIntentId,
    pub credential_revision_id: CredentialRevisionId,
    pub material_key_id: MaterialKeyId,
    pub prepared_attachment_id: Option<CredentialPreparedAttachmentId>,
    pub state: CredentialMaterialPreparationState,
}

/// Application boundary for turning one reserved credential identity into a
/// versioned encrypted frame.
#[async_trait]
pub trait CredentialMaterialPreparer: Send + Sync {
    async fn prepare(
        &self,
        context: &RequestContext,
        request: CredentialMaterialPreparationRequest,
    ) -> Result<CredentialMaterialPreparation, ApplicationError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialMaterialPreparationFaultPoint {
    AfterVaultCreate,
    AfterProvisionalCreated,
    AfterProvisionalReceipt,
    AfterSeal,
    AfterPrepared,
}

pub trait CredentialMaterialPreparationFaultInjector: Send + Sync {
    fn check(&self, point: CredentialMaterialPreparationFaultPoint)
    -> Result<(), ApplicationError>;
}

#[derive(Default)]
pub struct NoCredentialMaterialPreparationFaults;

impl CredentialMaterialPreparationFaultInjector for NoCredentialMaterialPreparationFaults {
    fn check(
        &self,
        _point: CredentialMaterialPreparationFaultPoint,
    ) -> Result<(), ApplicationError> {
        Ok(())
    }
}

/// One first publication of a Candidate credential revision into an empty slot.
#[derive(Clone, Debug)]
pub struct CredentialActivationCommand {
    pub connection_id: ConnectionId,
    pub credential_slot_id: CredentialSlotId,
    pub execution_guard_id: uuid::Uuid,
    pub activation_guard_id: uuid::Uuid,
    pub credential_revision_id: uuid::Uuid,
    pub credential_intent_id: uuid::Uuid,
    pub connection_qualification_revision_id: uuid::Uuid,
    pub expected_slot_version: u64,
    pub idempotency: Option<IdempotencyRecord>,
    pub outbox: Vec<OutboxMessage>,
    pub audit: AuditEvent,
}

/// One ordinary replacement of the current credential revision by a Candidate.
#[derive(Clone, Debug)]
pub struct CredentialRotationCommand {
    pub connection_id: ConnectionId,
    pub credential_slot_id: CredentialSlotId,
    pub execution_guard_id: uuid::Uuid,
    pub activation_guard_id: uuid::Uuid,
    pub previous_credential_revision_id: uuid::Uuid,
    pub activated_credential_revision_id: uuid::Uuid,
    pub activated_credential_intent_id: uuid::Uuid,
    pub connection_qualification_revision_id: uuid::Uuid,
    pub expected_slot_version: u64,
    pub idempotency: Option<IdempotencyRecord>,
    pub outbox: Vec<OutboxMessage>,
    pub audit: AuditEvent,
}

/// Immediate, final removal of a credential revision from slot resolution.
#[derive(Clone, Debug)]
pub struct CredentialRevocationCommand {
    pub connection_id: ConnectionId,
    pub credential_slot_id: CredentialSlotId,
    pub execution_guard_id: uuid::Uuid,
    pub activation_guard_id: uuid::Uuid,
    pub credential_revision_id: uuid::Uuid,
    pub credential_intent_id: uuid::Uuid,
    pub expected_slot_version: u64,
    pub idempotency: Option<IdempotencyRecord>,
    pub outbox: Vec<OutboxMessage>,
    pub audit: AuditEvent,
}

/// The one immutable Candidate association version that may be abandoned.
/// Its request evidence is mandatory because phase one is externally
/// recoverable: a retry must find the original preparation rather than write
/// another cancellation, audit event, or outbox message.
#[derive(Clone, Debug)]
pub struct CandidateCredentialAbandonCommand {
    pub credential_intent_id: uuid::Uuid,
    pub expected_association_version: u64,
    pub idempotency: IdempotencyRecord,
    pub outbox: Vec<OutboxMessage>,
    pub audit: AuditEvent,
}

/// Typed failures specific to the credential activation boundary.
#[derive(Debug, thiserror::Error)]
pub enum CredentialActivationError {
    /// P04 owns the evidence needed to move a live embedding dependency.
    #[error("a live embedding dependency requires a P04 embedding transition")]
    EmbeddingTransitionRequired,
    #[error(transparent)]
    Application(ApplicationError),
}

impl From<ApplicationError> for CredentialActivationError {
    fn from(error: ApplicationError) -> Self {
        match error {
            ApplicationError::Conflict(code) if code == "EMBEDDING_TRANSITION_REQUIRED" => {
                Self::EmbeddingTransitionRequired
            }
            other => Self::Application(other),
        }
    }
}

/// The sole application boundary that may publish, replace, or revoke a slot's
/// resolved credential revision.
#[async_trait]
pub trait CredentialActivationRepository: Send + Sync {
    /// Commits Candidate cancellation, its request evidence, and exactly one
    /// erasure preparation before host-vault work starts.  An exact retry
    /// returns that preparation without re-emitting the mutation evidence.
    async fn prepare_candidate_abandon(
        &self,
        context: RequestContext,
        command: CandidateCredentialAbandonCommand,
    ) -> Result<MaterialErasurePreparation, CredentialActivationError>;

    async fn activate_first(
        &self,
        context: RequestContext,
        command: CredentialActivationCommand,
    ) -> Result<GovernedMutationReceipt, CredentialActivationError>;

    async fn rotate(
        &self,
        context: RequestContext,
        command: CredentialRotationCommand,
    ) -> Result<GovernedMutationReceipt, CredentialActivationError>;

    async fn revoke(
        &self,
        context: RequestContext,
        command: CredentialRevocationCommand,
    ) -> Result<GovernedMutationReceipt, CredentialActivationError>;
}

/// Candidate abandonment composes the guarded phase-one activation authority
/// with the existing guarded fence/finalizer port.  Each database interaction
/// is deliberately complete before the vault is called.
pub struct CandidateCredentialAbandonService<A, E, V> {
    activation: A,
    erasure: E,
    vault: Arc<V>,
}

impl<A, E, V> CandidateCredentialAbandonService<A, E, V>
where
    A: CredentialActivationRepository,
    E: MaterialErasureRepository,
    V: MaterialKeyVault + 'static,
{
    pub fn new(activation: A, erasure: E, vault: V) -> Self {
        Self {
            activation,
            erasure,
            vault: Arc::new(vault),
        }
    }

    pub async fn abandon(
        &self,
        context: &RequestContext,
        command: CandidateCredentialAbandonCommand,
    ) -> Result<ErasureReceipt, CredentialActivationError> {
        let preparation = self
            .activation
            .prepare_candidate_abandon(context.clone(), command)
            .await?;
        if let Some(receipt) = preparation.finalized_receipt() {
            return Ok(receipt);
        }

        let fence = self.prepare_vault(preparation.material_key_id()).await?;
        self.erasure
            .record_fence(context, preparation.id(), fence)
            .await
            .map_err(CredentialActivationError::from)?;
        let receipt = self.erase_vault(preparation.material_key_id()).await?;
        self.erasure
            .finalize_credential(context, preparation.id(), receipt)
            .await
            .map_err(CredentialActivationError::from)
    }

    async fn prepare_vault(
        &self,
        material_key_id: MaterialKeyId,
    ) -> Result<FenceReceipt, CredentialActivationError> {
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || {
            vault.prepare_erasure(material_key_id).map_err(|error| {
                CredentialActivationError::from(ApplicationError::Unavailable(error.to_string()))
            })
        })
        .await
        .map_err(|error| {
            CredentialActivationError::from(ApplicationError::Internal(format!(
                "material vault task failed: {error}"
            )))
        })?
    }

    async fn erase_vault(
        &self,
        material_key_id: MaterialKeyId,
    ) -> Result<ErasureReceipt, CredentialActivationError> {
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || {
            vault.erase(material_key_id).map_err(|error| {
                CredentialActivationError::from(ApplicationError::Unavailable(error.to_string()))
            })
        })
        .await
        .map_err(|error| {
            CredentialActivationError::from(ApplicationError::Internal(format!(
                "material vault task failed: {error}"
            )))
        })?
    }
}

/// The lease boundary for a provider dispatch. Consumption receives the
/// caller's already-open unit of work so it cannot create a separate
/// transaction from the dispatch it authorizes.
#[async_trait]
pub trait CredentialDispatchLeaseRepository: Send + Sync {
    async fn issue(
        &self,
        context: &RequestContext,
        request: CredentialDispatchLeaseRequest,
    ) -> Result<CredentialDispatchLease, ApplicationError>;

    /// Issue a dispatch lease inside the caller's already-open transaction.
    async fn issue_in(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        request: CredentialDispatchLeaseRequest,
    ) -> Result<CredentialDispatchLease, ApplicationError>;

    async fn consume_for_dispatch(
        &self,
        context: &RequestContext,
        unit_of_work: &mut dyn UnitOfWork,
        lease: &CredentialDispatchLease,
    ) -> Result<ConnectionAuth, ApplicationError>;
}
