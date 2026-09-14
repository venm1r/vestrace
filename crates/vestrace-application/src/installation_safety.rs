//! Ports and orchestration contracts for the host-only P05 safety supervisor.

use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{
    DatabaseGenerationId, InstallationId, JournalPublicKey, RequestId, SafetyBootstrapBinding,
    SafetyJournalDigest, SignedJournalEntry, WitnessAdvance, WitnessError, WitnessHead,
    WitnessPublicKey, WitnessReceipt,
};

use crate::{ApplicationError, InstallationMutationPermit, PermitMode, RequestContext, UnitOfWork};

const SUPERVISOR_WORKSPACE_ID: u128 = 5;
const SUPERVISOR_PRINCIPAL_ID: u128 = 6;

/// Fixed request context accepted by P05 guarded database functions.
#[derive(Clone, Debug)]
pub struct InstallationSupervisorContext(RequestContext);

impl InstallationSupervisorContext {
    /// Constructed only by the host supervisor composition.
    pub fn host_supervisor() -> Self {
        Self(RequestContext::new(
            vestrace_domain::WorkspaceId::from_uuid(uuid::Uuid::from_u128(SUPERVISOR_WORKSPACE_ID)),
            vestrace_domain::PrincipalId::from_uuid(uuid::Uuid::from_u128(SUPERVISOR_PRINCIPAL_ID)),
        ))
    }

    pub fn request_context(&self) -> &RequestContext {
        &self.0
    }
}

#[derive(Clone)]
pub struct InitializeInstallationSafety {
    request_id: RequestId,
    bootstrap: SafetyBootstrapBinding,
    generation_id: DatabaseGenerationId,
}

impl InitializeInstallationSafety {
    pub fn new(
        request_id: RequestId,
        bootstrap: SafetyBootstrapBinding,
        generation_id: DatabaseGenerationId,
    ) -> Self {
        Self {
            request_id,
            bootstrap,
            generation_id,
        }
    }
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }
    pub fn bootstrap(&self) -> &SafetyBootstrapBinding {
        &self.bootstrap
    }
    pub const fn generation_id(&self) -> DatabaseGenerationId {
        self.generation_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterDatabaseGeneration {
    request_id: RequestId,
    generation_id: DatabaseGenerationId,
    activation_epoch: u64,
}

impl RegisterDatabaseGeneration {
    pub const fn new(
        request_id: RequestId,
        generation_id: DatabaseGenerationId,
        activation_epoch: u64,
    ) -> Self {
        Self {
            request_id,
            generation_id,
            activation_epoch,
        }
    }
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }
    pub const fn generation_id(&self) -> DatabaseGenerationId {
        self.generation_id
    }
    pub const fn activation_epoch(&self) -> u64 {
        self.activation_epoch
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallationSafetySnapshot {
    installation_id: InstallationId,
    sequence: u64,
    journal_digest: SafetyJournalDigest,
    active_generation_id: DatabaseGenerationId,
    activation_epoch: u64,
    journal_public_key: JournalPublicKey,
    witness_public_key: WitnessPublicKey,
}

impl InstallationSafetySnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        installation_id: InstallationId,
        sequence: u64,
        journal_digest: SafetyJournalDigest,
        active_generation_id: DatabaseGenerationId,
        activation_epoch: u64,
        journal_public_key: JournalPublicKey,
        witness_public_key: WitnessPublicKey,
    ) -> Self {
        Self {
            installation_id,
            sequence,
            journal_digest,
            active_generation_id,
            activation_epoch,
            journal_public_key,
            witness_public_key,
        }
    }
    pub const fn installation_id(&self) -> InstallationId {
        self.installation_id
    }
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    pub const fn journal_digest(&self) -> SafetyJournalDigest {
        self.journal_digest
    }
    pub const fn active_generation_id(&self) -> DatabaseGenerationId {
        self.active_generation_id
    }
    pub const fn activation_epoch(&self) -> u64 {
        self.activation_epoch
    }
    pub fn journal_public_key(&self) -> &JournalPublicKey {
        &self.journal_public_key
    }
    pub fn witness_public_key(&self) -> &WitnessPublicKey {
        &self.witness_public_key
    }
}

#[async_trait]
pub trait SafetyJournal: Send + Sync {
    /// Persist exactly one signed immutable entry before it reaches the witness.
    async fn append(&self, entry: &SignedJournalEntry) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait InstallationSafetyWitness: Send + Sync {
    async fn read_head(&self) -> Result<WitnessHead, WitnessError>;
    async fn compare_and_advance(
        &self,
        expected: WitnessHead,
        next: WitnessAdvance,
    ) -> Result<WitnessReceipt, WitnessError>;
}

#[async_trait]
pub trait SafetyAuthorityRepository: Send + Sync {
    async fn initialize_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        request: InitializeInstallationSafety,
        signed_entry: SignedJournalEntry,
        witness_receipt: WitnessReceipt,
    ) -> Result<InstallationSafetySnapshot, ApplicationError>;
    async fn register_generation_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        request: RegisterDatabaseGeneration,
        signed_entry: SignedJournalEntry,
        witness_receipt: WitnessReceipt,
    ) -> Result<InstallationSafetySnapshot, ApplicationError>;
    async fn current(&self) -> Result<Option<InstallationSafetySnapshot>, ApplicationError>;
}

/// Enforces the required ordering: exclusive permit, durable journal, witness
/// compare-and-advance, guarded SQL call, then permit-owned transaction commit.
pub struct SafetyAuthorityService {
    permit: Arc<dyn InstallationMutationPermit>,
    journal: Arc<dyn SafetyJournal>,
    witness: Arc<dyn InstallationSafetyWitness>,
    repository: Arc<dyn SafetyAuthorityRepository>,
}

struct PreparedSafetyAdvance {
    permit: crate::PermitHandle,
    head: WitnessHead,
    advance: WitnessAdvance,
}

impl SafetyAuthorityService {
    pub fn new(
        permit: Arc<dyn InstallationMutationPermit>,
        journal: Arc<dyn SafetyJournal>,
        witness: Arc<dyn InstallationSafetyWitness>,
        repository: Arc<dyn SafetyAuthorityRepository>,
    ) -> Self {
        Self {
            permit,
            journal,
            witness,
            repository,
        }
    }

    pub async fn initialize(
        &self,
        context: &InstallationSupervisorContext,
        request: InitializeInstallationSafety,
        entry: SignedJournalEntry,
    ) -> Result<InstallationSafetySnapshot, ApplicationError> {
        self.initialize_with_after_witness(context, request, entry, || {})
            .await
    }

    /// Runs a host-supervisor initialization while allowing the caller's
    /// fault harness to stop after the receipt is durable and before the
    /// guarded database mutation starts.
    pub async fn initialize_with_after_witness<F>(
        &self,
        context: &InstallationSupervisorContext,
        request: InitializeInstallationSafety,
        entry: SignedJournalEntry,
        after_witness_advance: F,
    ) -> Result<InstallationSafetySnapshot, ApplicationError>
    where
        F: FnOnce(),
    {
        let PreparedSafetyAdvance {
            mut permit,
            head,
            advance,
        } = self.acquire_and_append(context, &entry).await?;
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        after_witness_advance();
        let snapshot = self
            .repository
            .initialize_in(permit.unit_of_work_mut(), request, entry, receipt)
            .await?;
        permit.commit().await?;
        Ok(snapshot)
    }

    pub async fn register_generation(
        &self,
        context: &InstallationSupervisorContext,
        request: RegisterDatabaseGeneration,
        entry: SignedJournalEntry,
    ) -> Result<InstallationSafetySnapshot, ApplicationError> {
        let PreparedSafetyAdvance {
            mut permit,
            head,
            advance,
        } = self.acquire_and_append(context, &entry).await?;
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        let snapshot = self
            .repository
            .register_generation_in(permit.unit_of_work_mut(), request, entry, receipt)
            .await?;
        permit.commit().await?;
        Ok(snapshot)
    }

    async fn acquire_and_append(
        &self,
        context: &InstallationSupervisorContext,
        entry: &SignedJournalEntry,
    ) -> Result<PreparedSafetyAdvance, ApplicationError> {
        let permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let head = self.witness.read_head().await.map_err(witness_error)?;
        let advance = head
            .accept_signed(entry)
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        self.journal.append(entry).await?;
        Ok(PreparedSafetyAdvance {
            permit,
            head,
            advance,
        })
    }

    pub async fn current(&self) -> Result<Option<InstallationSafetySnapshot>, ApplicationError> {
        self.repository.current().await
    }
}

fn witness_error(error: WitnessError) -> ApplicationError {
    match error {
        WitnessError::Conflict => ApplicationError::Conflict("witness head changed".to_owned()),
        WitnessError::Unavailable(message) => ApplicationError::Unavailable(message),
        WitnessError::Invalid(error) => ApplicationError::Policy(error.to_string()),
    }
}
