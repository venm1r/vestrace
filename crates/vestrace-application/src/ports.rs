use std::any::Any;

use crate::FenceReceipt;
use crate::{ApplicationError, RequestContext};
use vestrace_domain::{
    ContentMaterialId, CredentialKeyCreationIntent, CredentialKeyCreationIntentId,
    CredentialPreparedAttachmentId, CredentialRevisionId, ErasureReceipt, InstallationDrainRequest,
    InstallationDrainRequestId, MaterialKeyBindingReceipt, MaterialKeyCreationIntent,
    MaterialKeyCreationIntentId, MaterialKeyId, PreparedMaterialAttachmentId, SizeClass,
    VaultReceipt,
};

#[async_trait::async_trait]
pub trait UnitOfWork: Send {
    /// Lets an infrastructure adapter recover its concrete transaction without
    /// putting PostgreSQL types in an application port.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    async fn commit(self: Box<Self>) -> Result<(), ApplicationError>;
    async fn rollback(self: Box<Self>) -> Result<(), ApplicationError>;
}

#[async_trait::async_trait]
pub trait TransactionManager: Send + Sync {
    async fn begin(
        &self,
        context: &RequestContext,
    ) -> Result<Box<dyn UnitOfWork>, ApplicationError>;
}

/// Transactional boundary for the fixed-identity material-key creation
/// lifecycle. Implementations must delegate all transitions to the database;
/// application code never manufactures a Live reference from prepared state.
#[async_trait::async_trait]
pub trait MaterialIntentRepository: Send + Sync {
    /// Reserve an intent in a transaction owned by the caller. Implementations
    /// that cannot share that transaction must refuse rather than open a
    /// second, hidden transaction.
    async fn reserve_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
        _intent: &MaterialKeyCreationIntent,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Unavailable(
            "transaction-bound material reservation is not configured".to_owned(),
        ))
    }

    async fn reserve(
        &self,
        context: &RequestContext,
        intent: &MaterialKeyCreationIntent,
    ) -> Result<(), ApplicationError>;

    async fn record_provisional_created(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError>;

    async fn record_provisional_receipt(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        receipt: VaultReceipt,
    ) -> Result<(), ApplicationError>;

    async fn prepare_content(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        attachment_id: PreparedMaterialAttachmentId,
        ciphertext: &[u8],
        size_class: SizeClass,
    ) -> Result<(), ApplicationError>;

    /// Prepare ciphertext in a transaction owned by the caller. This is the
    /// governed-input seam; it must not create a nested transaction.
    async fn prepare_content_in(
        &self,
        _context: &RequestContext,
        _unit_of_work: &mut dyn UnitOfWork,
        _intent_id: MaterialKeyCreationIntentId,
        _attachment_id: PreparedMaterialAttachmentId,
        _ciphertext: &[u8],
        _size_class: SizeClass,
    ) -> Result<(), ApplicationError> {
        Err(ApplicationError::Unavailable(
            "transaction-bound content preparation is not configured".to_owned(),
        ))
    }

    async fn prepare_result(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        attachment_id: PreparedMaterialAttachmentId,
        ciphertext: &[u8],
        size_class: SizeClass,
    ) -> Result<(), ApplicationError>;

    async fn bind(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        receipt: MaterialKeyBindingReceipt,
    ) -> Result<(), ApplicationError>;

    async fn finalize_bound(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError>;

    async fn prepare_content_abandon(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError>;

    /// The abort available before any prepared marker exists.
    ///
    /// Deliberately a separate method from [`Self::prepare_content_abandon`]:
    /// the two branches are distinct in the spec, and a single method taking a
    /// flag would let a caller mislabel which one it is taking.
    async fn prepare_pre_prepared_abandon(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError>;

    async fn record_unbound_erasure(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
        receipt: ErasureReceipt,
    ) -> Result<(), ApplicationError>;

    async fn finalize_abandon(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<(), ApplicationError>;

    async fn is_live(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<bool, ApplicationError>;

    /// What a crashed intent left behind.
    ///
    /// Resumption after a crash cannot ask the process that died what it was
    /// doing, so every identity a resumption needs is read back from the row
    /// rather than carried in memory.
    async fn snapshot(
        &self,
        context: &RequestContext,
        intent_id: MaterialKeyCreationIntentId,
    ) -> Result<Option<crate::MaterialIntentSnapshot>, ApplicationError>;
}

/// Transactional boundary for the fixed-identity credential-key lifecycle.
/// The database owns transition validation and Candidate publication.
#[async_trait::async_trait]
pub trait CredentialIntentRepository: Send + Sync {
    async fn reserve(
        &self,
        context: &RequestContext,
        intent: &CredentialKeyCreationIntent,
    ) -> Result<(), ApplicationError>;

    async fn record_provisional_created(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<(), ApplicationError>;

    async fn record_provisional_receipt(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        receipt: VaultReceipt,
    ) -> Result<(), ApplicationError>;

    async fn prepare(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        attachment_id: CredentialPreparedAttachmentId,
        ciphertext: &[u8],
    ) -> Result<(), ApplicationError>;

    async fn bind(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        receipt: MaterialKeyBindingReceipt,
    ) -> Result<(), ApplicationError>;

    async fn finalize_candidate(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<(), ApplicationError>;

    async fn prepare_abandon(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        expected_association_version: u64,
    ) -> Result<(), ApplicationError>;

    async fn record_unbound_erasure(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
        receipt: ErasureReceipt,
    ) -> Result<(), ApplicationError>;

    async fn finalize_abandon(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<(), ApplicationError>;

    async fn is_candidate(
        &self,
        context: &RequestContext,
        revision_id: CredentialRevisionId,
    ) -> Result<bool, ApplicationError>;

    /// What a crashed intent left behind, including the association version a
    /// pre-live abandonment must present.
    async fn snapshot(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<Option<crate::CredentialIntentSnapshot>, ApplicationError>;
}

/// DrainMutationPermit: `request` and both `reserve` functions it guards
/// take a PostgreSQL advisory transaction lock scoped to installation
/// mutation (the same conceptual exclusion `InstallationMutationPermit::
/// Exclusive` names, taken directly in SQL rather than through that Rust
/// permit type) so the drain snapshot and a concurrent reservation cannot
/// both commit past each other. `current_state` and `reconcile` are plain
/// reads plus (for `reconcile`) a conditional completion write, safe to call
/// at any time including after a crash.
#[async_trait::async_trait]
pub trait DrainMutationPermitRepository: Send + Sync {
    async fn request(
        &self,
        context: &RequestContext,
    ) -> Result<InstallationDrainRequest, ApplicationError>;

    async fn current_state(
        &self,
        context: &RequestContext,
    ) -> Result<Option<InstallationDrainRequest>, ApplicationError>;

    async fn reconcile(
        &self,
        context: &RequestContext,
        id: InstallationDrainRequestId,
    ) -> Result<InstallationDrainRequest, ApplicationError>;
}

/// The database portion of irreversible material destruction. Each method is
/// a short, scoped transaction: the host vault call belongs strictly between
/// `prepare_*` and `finalize_*`, never inside a database transaction.
#[async_trait::async_trait]
pub trait MaterialErasureRepository: Send + Sync {
    async fn prepare_content(
        &self,
        context: &RequestContext,
        material_id: ContentMaterialId,
    ) -> Result<MaterialErasurePreparation, ApplicationError>;

    async fn prepare_credential(
        &self,
        context: &RequestContext,
        intent_id: CredentialKeyCreationIntentId,
    ) -> Result<MaterialErasurePreparation, ApplicationError>;

    async fn record_fence(
        &self,
        context: &RequestContext,
        preparation_id: uuid::Uuid,
        receipt: FenceReceipt,
    ) -> Result<(), ApplicationError>;

    async fn finalize_content(
        &self,
        context: &RequestContext,
        preparation_id: uuid::Uuid,
        receipt: ErasureReceipt,
    ) -> Result<ErasureReceipt, ApplicationError>;

    async fn finalize_credential(
        &self,
        context: &RequestContext,
        preparation_id: uuid::Uuid,
        receipt: ErasureReceipt,
    ) -> Result<ErasureReceipt, ApplicationError>;
}

/// The durable phase-one result. A replay that has already completed carries
/// the original host-vault receipt and therefore must not call the vault again.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaterialErasurePreparation {
    id: uuid::Uuid,
    material_key_id: MaterialKeyId,
    finalized_receipt: Option<ErasureReceipt>,
}

impl MaterialErasurePreparation {
    pub const fn new(
        id: uuid::Uuid,
        material_key_id: MaterialKeyId,
        finalized_receipt: Option<ErasureReceipt>,
    ) -> Self {
        Self {
            id,
            material_key_id,
            finalized_receipt,
        }
    }

    pub const fn id(self) -> uuid::Uuid {
        self.id
    }

    pub const fn material_key_id(self) -> MaterialKeyId {
        self.material_key_id
    }

    pub const fn finalized_receipt(self) -> Option<ErasureReceipt> {
        self.finalized_receipt
    }
}
