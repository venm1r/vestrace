//! Host-supervisor archive orchestration ports.
//!
//! Archive bytes and key material never cross the guarded PostgreSQL boundary.
//! This module owns only the ordering between an installation permit, host
//! staging, the P05 journal/witness, and a future guarded repository.

use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    sync::Arc,
};

use async_trait::async_trait;
use vestrace_domain::{
    ArchiveAppendReservation, ArchiveHead, ArchiveObjectDescriptor, BackupArchiveStateV1,
    BackupObjectId, BackupSetId, BackupSetIdentity, RestoreHoldId, SafetyEventKind,
    SafetyJournalDigest, SignedJournalEntry, WalArchiveCheckpoint, WitnessError, WitnessHead,
    WitnessReceipt,
};

use crate::{
    ApplicationError, InstallationMutationPermit, InstallationSafetyWitness,
    InstallationSupervisorContext, PermitMode, SafetyJournal, UnitOfWork,
};

/// An opaque host-owned staging identity. Cleanup accepts it only with its
/// exact immutable descriptor, never by set-wide prefix.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArchiveStagingId(uuid::Uuid);

impl ArchiveStagingId {
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

impl Default for ArchiveStagingId {
    fn default() -> Self {
        Self::new()
    }
}

/// Ciphertext and its immutable descriptor at the host adapter boundary.
/// Plaintext is deliberately absent from this type.
#[derive(Debug, Eq, PartialEq)]
pub struct ArchiveObjectWrite {
    descriptor: ArchiveObjectDescriptor,
    ciphertext: ArchiveCiphertext,
}

/// An owned host-only ciphertext source. A spool is deleted when the write
/// leaves the staging path, including any failed staging attempt.
#[derive(Debug, Eq, PartialEq)]
enum ArchiveCiphertext {
    InMemory(Vec<u8>),
    Spool(PathBuf),
}

impl Drop for ArchiveCiphertext {
    fn drop(&mut self) {
        if let Self::Spool(path) = self {
            let _ = fs::remove_file(path);
        }
    }
}

impl ArchiveObjectWrite {
    pub fn new(
        descriptor: ArchiveObjectDescriptor,
        ciphertext: Vec<u8>,
    ) -> Result<Self, ApplicationError> {
        if ciphertext.is_empty()
            || SafetyJournalDigest::of(&ciphertext) != descriptor.to_input().ciphertext_digest
        {
            return Err(ApplicationError::Policy(
                "archive ciphertext does not match its immutable descriptor".to_owned(),
            ));
        }
        Ok(Self {
            descriptor,
            ciphertext: ArchiveCiphertext::InMemory(ciphertext),
        })
    }

    /// Binds a custody-produced encrypted spool to its final descriptor
    /// without loading its bytes into the supervisor process.
    pub fn from_spool(
        descriptor: ArchiveObjectDescriptor,
        path: PathBuf,
    ) -> Result<Self, ApplicationError> {
        let valid = fs::metadata(&path)
            .map(|metadata| metadata.is_file() && metadata.len() == descriptor.to_input().length)
            .unwrap_or(false);
        if !valid {
            let _ = fs::remove_file(&path);
            return Err(ApplicationError::Policy(
                "archive ciphertext spool does not match its immutable descriptor".to_owned(),
            ));
        }
        Ok(Self {
            descriptor,
            ciphertext: ArchiveCiphertext::Spool(path),
        })
    }

    pub fn descriptor(&self) -> &ArchiveObjectDescriptor {
        &self.descriptor
    }

    pub fn copy_ciphertext_to(&self, output: &mut dyn Write) -> Result<u64, ApplicationError> {
        match &self.ciphertext {
            ArchiveCiphertext::InMemory(ciphertext) => {
                output
                    .write_all(ciphertext)
                    .map_err(io_error("write archive ciphertext"))?;
                Ok(ciphertext.len() as u64)
            }
            ArchiveCiphertext::Spool(path) => {
                let mut input =
                    fs::File::open(path).map_err(io_error("open archive ciphertext spool"))?;
                io::copy(&mut input, output).map_err(io_error("copy archive ciphertext spool"))
            }
        }
    }
}

impl Drop for ArchiveObjectWrite {
    fn drop(&mut self) {
        if let ArchiveCiphertext::Spool(path) = &self.ciphertext {
            let _ = fs::remove_file(path);
        }
    }
}

fn io_error(operation: &'static str) -> impl FnOnce(io::Error) -> ApplicationError {
    move |error| ApplicationError::Unavailable(format!("{operation}: {error}"))
}

/// A durable staging member, distinct from the committed immutable object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StagedArchiveObject {
    staging_id: ArchiveStagingId,
    descriptor: ArchiveObjectDescriptor,
}

impl StagedArchiveObject {
    pub fn new(staging_id: ArchiveStagingId, descriptor: ArchiveObjectDescriptor) -> Self {
        Self {
            staging_id,
            descriptor,
        }
    }

    pub const fn staging_id(&self) -> ArchiveStagingId {
        self.staging_id
    }

    pub fn descriptor(&self) -> &ArchiveObjectDescriptor {
        &self.descriptor
    }
}

/// Opaque reference to a host-held per-set archive-key envelope.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArchiveKeyEnvelopeRef(uuid::Uuid);

impl ArchiveKeyEnvelopeRef {
    pub const fn from_uuid(value: uuid::Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}

/// Opaque proof that host key custody erased a prepared envelope.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArchiveKeyErasureReceipt(SafetyJournalDigest);

impl ArchiveKeyErasureReceipt {
    pub const fn new(digest: SafetyJournalDigest) -> Self {
        Self(digest)
    }

    pub const fn digest(self) -> SafetyJournalDigest {
        self.0
    }
}

/// Database-owned preparation that may be presented to host key custody once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManagedBackupDeletionPrepared {
    set_id: BackupSetId,
    preparation_digest: SafetyJournalDigest,
}

impl ManagedBackupDeletionPrepared {
    pub const fn new(set_id: BackupSetId, preparation_digest: SafetyJournalDigest) -> Self {
        Self {
            set_id,
            preparation_digest,
        }
    }

    pub const fn set_id(self) -> BackupSetId {
        self.set_id
    }

    pub const fn preparation_digest(self) -> SafetyJournalDigest {
        self.preparation_digest
    }
}

/// Guarded archive snapshot returned only after a transition commits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupArchiveSnapshot {
    state: BackupArchiveStateV1,
}

/// Starts exactly one set at a known empty archive head. The signed entry
/// carries the resulting archive-state successor; the envelope itself stays
/// in host custody and is represented in PostgreSQL only by its opaque ID.
#[derive(Clone)]
pub struct StartManagedBackup {
    identity: BackupSetIdentity,
    initial_head: ArchiveHead,
    signed_entry: SignedJournalEntry,
}

impl StartManagedBackup {
    pub const fn new(
        identity: BackupSetIdentity,
        initial_head: ArchiveHead,
        signed_entry: SignedJournalEntry,
    ) -> Self {
        Self {
            identity,
            initial_head,
            signed_entry,
        }
    }

    pub const fn identity(&self) -> BackupSetIdentity {
        self.identity
    }

    pub const fn initial_head(&self) -> ArchiveHead {
        self.initial_head
    }
}

impl BackupArchiveSnapshot {
    pub const fn new(state: BackupArchiveStateV1) -> Self {
        Self { state }
    }

    pub const fn state(&self) -> &BackupArchiveStateV1 {
        &self.state
    }
}

/// Exact archive-head CAS input for a host-staged append.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveArchiveAppend {
    set_id: BackupSetId,
    expected_head: ArchiveHead,
    object: ArchiveObjectDescriptor,
}

impl ReserveArchiveAppend {
    pub fn new(
        set_id: BackupSetId,
        expected_head: ArchiveHead,
        object: ArchiveObjectDescriptor,
    ) -> Result<Self, ApplicationError> {
        if object.set_id() != set_id || object.predecessor_head_digest() != expected_head.digest {
            return Err(ApplicationError::Policy(
                "archive append is not bound to its set and expected head".to_owned(),
            ));
        }
        Ok(Self {
            set_id,
            expected_head,
            object,
        })
    }

    pub const fn set_id(&self) -> BackupSetId {
        self.set_id
    }

    pub const fn expected_head(&self) -> ArchiveHead {
        self.expected_head
    }

    pub fn object(&self) -> &ArchiveObjectDescriptor {
        &self.object
    }
}

/// Exact checkpoint commit input; a staged descriptor cannot be replaced after
/// the durable host write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitArchiveCheckpoint {
    reservation: ArchiveAppendReservation,
    staged: StagedArchiveObject,
}

/// Cancels the durable reservation for exactly one object while no signed
/// checkpoint names it.  It is intentionally the reservation itself rather
/// than a set-wide operation, so a failed append cannot unblock or erase a
/// concurrent member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AbandonArchiveAppend {
    reservation: ArchiveAppendReservation,
}

impl AbandonArchiveAppend {
    pub const fn new(reservation: ArchiveAppendReservation) -> Self {
        Self { reservation }
    }

    pub const fn reservation(&self) -> &ArchiveAppendReservation {
        &self.reservation
    }
}

/// A guarded PostgreSQL append intent that has no signed checkpoint yet.
/// Reconciliation may remove it only if the witnessed archive head remains
/// the exact predecessor recorded here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PendingArchiveAppend {
    set_id: BackupSetId,
    expected_head: ArchiveHead,
    object_id: BackupObjectId,
    ciphertext_digest: SafetyJournalDigest,
}

impl PendingArchiveAppend {
    pub const fn new(
        set_id: BackupSetId,
        expected_head: ArchiveHead,
        object_id: BackupObjectId,
        ciphertext_digest: SafetyJournalDigest,
    ) -> Self {
        Self {
            set_id,
            expected_head,
            object_id,
            ciphertext_digest,
        }
    }

    pub const fn set_id(self) -> BackupSetId {
        self.set_id
    }

    pub const fn expected_head(self) -> ArchiveHead {
        self.expected_head
    }

    pub const fn object_id(self) -> BackupObjectId {
        self.object_id
    }

    pub const fn ciphertext_digest(self) -> SafetyJournalDigest {
        self.ciphertext_digest
    }
}

impl CommitArchiveCheckpoint {
    pub fn new(
        reservation: ArchiveAppendReservation,
        staged: StagedArchiveObject,
    ) -> Result<Self, ApplicationError> {
        if reservation.object() != staged.descriptor() {
            return Err(ApplicationError::Policy(
                "staged archive object differs from the reserved checkpoint".to_owned(),
            ));
        }
        Ok(Self {
            reservation,
            staged,
        })
    }

    pub fn reservation(&self) -> &ArchiveAppendReservation {
        &self.reservation
    }

    pub fn staged(&self) -> &StagedArchiveObject {
        &self.staged
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcquireRestoreHold {
    set_id: BackupSetId,
    hold_id: RestoreHoldId,
}

/// One guarded lifecycle step that changes no archive member identity. The
/// prepared-deletion digest is opaque to PostgreSQL except for equality: it
/// stays bound to the one deletion preparation through key erasure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveLifecycleTransition {
    BeginSealing {
        set_id: BackupSetId,
    },
    CommitSealed {
        set_id: BackupSetId,
    },
    PrepareDeletion {
        prepared: ManagedBackupDeletionPrepared,
    },
    PrepareArchiveKeyErasure {
        prepared: ManagedBackupDeletionPrepared,
    },
    RecordArchiveKeyErased {
        prepared: ManagedBackupDeletionPrepared,
    },
    FinalizeDeleted {
        set_id: BackupSetId,
    },
}

impl ArchiveLifecycleTransition {
    pub const fn set_id(self) -> BackupSetId {
        match self {
            Self::BeginSealing { set_id }
            | Self::CommitSealed { set_id }
            | Self::FinalizeDeleted { set_id } => set_id,
            Self::PrepareDeletion { prepared }
            | Self::PrepareArchiveKeyErasure { prepared }
            | Self::RecordArchiveKeyErased { prepared } => prepared.set_id(),
        }
    }

    pub const fn event_kind(self) -> SafetyEventKind {
        match self {
            Self::BeginSealing { .. } => SafetyEventKind::ArchiveSealingStarted,
            Self::CommitSealed { .. } => SafetyEventKind::ArchiveSealed,
            Self::PrepareDeletion { .. } => SafetyEventKind::ManagedBackupDeletionPrepared,
            Self::PrepareArchiveKeyErasure { .. } => SafetyEventKind::PreparedArchiveKeyErasure,
            Self::RecordArchiveKeyErased { .. } => SafetyEventKind::ArchiveKeyErased,
            Self::FinalizeDeleted { .. } => SafetyEventKind::ManagedBackupDeleted,
        }
    }
}

impl AcquireRestoreHold {
    pub const fn new(set_id: BackupSetId, hold_id: RestoreHoldId) -> Self {
        Self { set_id, hold_id }
    }

    pub const fn set_id(self) -> BackupSetId {
        self.set_id
    }

    pub const fn hold_id(self) -> RestoreHoldId {
        self.hold_id
    }
}

/// Host-only staging and final-object operations. Implementations must never
/// open a hidden database transaction.
#[async_trait]
pub trait BackupObjectStore: Send + Sync {
    async fn stage_encrypted(
        &self,
        write: ArchiveObjectWrite,
    ) -> Result<StagedArchiveObject, ApplicationError>;
    async fn verify_durable(&self, staged: &StagedArchiveObject) -> Result<(), ApplicationError>;
    /// Makes an exact staged member visible under its immutable object name
    /// only after PostgreSQL has committed the matching checkpoint.
    async fn promote_after_checkpoint(
        &self,
        staged: &StagedArchiveObject,
    ) -> Result<(), ApplicationError>;
    async fn remove_orphan(&self, staged: &StagedArchiveObject) -> Result<(), ApplicationError>;
    async fn remove_committed(
        &self,
        object: &ArchiveObjectDescriptor,
    ) -> Result<(), ApplicationError>;
    /// Removes one immutable object only after the caller has fetched the
    /// exact guarded manifest for this deletion preparation.
    async fn remove_prepared_committed(
        &self,
        _: ManagedBackupDeletionPrepared,
        object: &ArchiveObjectDescriptor,
    ) -> Result<(), ApplicationError> {
        self.remove_committed(object).await
    }
}

/// Host-owned archive key custody. Neither input nor output contains key bytes.
#[async_trait]
pub trait ArchiveKeyCustody: Send + Sync {
    async fn create_set_key(
        &self,
        set: BackupSetId,
    ) -> Result<ArchiveKeyEnvelopeRef, ApplicationError>;
    /// Removes a just-created envelope only before any journal entry names
    /// the set. Callers must retain the envelope once journalling begins so a
    /// later reconciliation can finish the exact signed transition.
    async fn discard_uncommitted_set_key(&self, set: BackupSetId) -> Result<(), ApplicationError>;
    async fn erase_prepared_key(
        &self,
        prepared: ManagedBackupDeletionPrepared,
    ) -> Result<ArchiveKeyErasureReceipt, ApplicationError>;
}

/// Guarded PostgreSQL boundary. Every mutation is bound to the permit-owned
/// unit of work supplied by the controller.
#[async_trait]
pub trait BackupArchiveRepository: Send + Sync {
    async fn start_set_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        command: StartManagedBackup,
        envelope: ArchiveKeyEnvelopeRef,
        entry: SignedJournalEntry,
        receipt: WitnessReceipt,
    ) -> Result<BackupArchiveSnapshot, ApplicationError>;

    async fn reserve_append_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        command: ReserveArchiveAppend,
    ) -> Result<ArchiveAppendReservation, ApplicationError>;

    async fn abandon_append_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        command: AbandonArchiveAppend,
    ) -> Result<(), ApplicationError>;

    async fn list_pending_appends_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<Vec<PendingArchiveAppend>, ApplicationError>;

    async fn abandon_pending_append_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        pending: PendingArchiveAppend,
    ) -> Result<(), ApplicationError>;

    async fn commit_checkpoint_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        command: CommitArchiveCheckpoint,
        entry: SignedJournalEntry,
        receipt: WitnessReceipt,
    ) -> Result<BackupArchiveSnapshot, ApplicationError>;

    async fn acquire_hold_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        command: AcquireRestoreHold,
        entry: SignedJournalEntry,
        receipt: WitnessReceipt,
    ) -> Result<BackupArchiveSnapshot, ApplicationError>;

    async fn transition_lifecycle_in(
        &self,
        _: &mut dyn UnitOfWork,
        _: ArchiveLifecycleTransition,
        _: SignedJournalEntry,
        _: WitnessReceipt,
    ) -> Result<BackupArchiveSnapshot, ApplicationError> {
        Err(ApplicationError::Internal(
            "archive lifecycle transition is not configured".to_owned(),
        ))
    }

    /// Reads the immutable members named by one prepared deletion. The
    /// controller commits the short guarded read before it touches host files.
    async fn list_prepared_archive_objects_in(
        &self,
        _: &mut dyn UnitOfWork,
        _: ManagedBackupDeletionPrepared,
    ) -> Result<Vec<ArchiveObjectDescriptor>, ApplicationError> {
        Err(ApplicationError::Internal(
            "prepared archive manifest listing is not configured".to_owned(),
        ))
    }
}

/// Complete signed command for a WAL object whose ciphertext has already been
/// produced by the host supervisor. The controller neither accepts plaintext
/// nor signs entries itself.
pub struct AppendWal {
    reservation: ReserveArchiveAppend,
    write: ArchiveObjectWrite,
    signed_entry: SignedJournalEntry,
}

impl AppendWal {
    pub fn new(
        reservation: ReserveArchiveAppend,
        write: ArchiveObjectWrite,
        signed_entry: SignedJournalEntry,
    ) -> Result<Self, ApplicationError> {
        if reservation.object() != write.descriptor() {
            return Err(ApplicationError::Policy(
                "archive write differs from the requested reservation".to_owned(),
            ));
        }
        Ok(Self {
            reservation,
            write,
            signed_entry,
        })
    }
}

/// Orders archive staging around the sole installation safety authority.
pub struct BackupArchiveController {
    permit: Arc<dyn InstallationMutationPermit>,
    journal: Arc<dyn SafetyJournal>,
    witness: Arc<dyn InstallationSafetyWitness>,
    repository: Arc<dyn BackupArchiveRepository>,
    object_store: Arc<dyn BackupObjectStore>,
    key_custody: Arc<dyn ArchiveKeyCustody>,
}

impl BackupArchiveController {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        permit: Arc<dyn InstallationMutationPermit>,
        journal: Arc<dyn SafetyJournal>,
        witness: Arc<dyn InstallationSafetyWitness>,
        repository: Arc<dyn BackupArchiveRepository>,
        object_store: Arc<dyn BackupObjectStore>,
        key_custody: Arc<dyn ArchiveKeyCustody>,
    ) -> Self {
        Self {
            permit,
            journal,
            witness,
            repository,
            object_store,
            key_custody,
        }
    }

    /// Creates the host-owned envelope before the signed transition. Any
    /// failure before journal durability removes precisely that envelope; a
    /// failure after journalling retains it for exact reconciliation.
    pub async fn start(
        &self,
        context: &InstallationSupervisorContext,
        command: StartManagedBackup,
    ) -> Result<BackupArchiveSnapshot, ApplicationError> {
        if command.signed_entry.event_kind() != SafetyEventKind::BackupSetStarted {
            return Err(ApplicationError::Policy(
                "managed backup start requires its dedicated signed event".to_owned(),
            ));
        }
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let set = command.identity().set_id();
        let envelope = self.key_custody.create_set_key(set).await?;
        let head = match self.witness.read_head().await.map_err(witness_error) {
            Ok(head) => head,
            Err(error) => return self.discard_uncommitted_key(set, error).await,
        };
        let expected_state = match head
            .state()
            .backup_archive_state()
            .start_streaming(command.identity(), command.initial_head())
        {
            Ok(state) => state,
            Err(error) => {
                return self
                    .discard_uncommitted_key(set, ApplicationError::Policy(error.to_string()))
                    .await;
            }
        };
        if command.signed_entry.state().backup_archive_state() != &expected_state {
            return self
                .discard_uncommitted_key(
                    set,
                    ApplicationError::Policy(
                        "signed backup-start entry does not carry the exact started state"
                            .to_owned(),
                    ),
                )
                .await;
        }
        let advance = match head.accept_signed(&command.signed_entry) {
            Ok(advance) => advance,
            Err(error) => {
                return self
                    .discard_uncommitted_key(set, ApplicationError::Policy(error.to_string()))
                    .await;
            }
        };
        if let Err(error) = self.journal.append(&command.signed_entry).await {
            return self.discard_uncommitted_key(set, error).await;
        }
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        let snapshot = self
            .repository
            .start_set_in(
                permit.unit_of_work_mut(),
                command.clone(),
                envelope,
                command.signed_entry,
                receipt,
            )
            .await?;
        permit.commit().await?;
        Ok(snapshot)
    }

    /// Stages exactly one ciphertext object before the corresponding
    /// journal/witness/database transition. Failures before journal append
    /// remove only the exact staging identity; failures after it retain the
    /// signed pair for later reconciliation.
    pub async fn append_wal(
        &self,
        context: &InstallationSupervisorContext,
        command: AppendWal,
    ) -> Result<BackupArchiveSnapshot, ApplicationError> {
        let reservation = self.reserve_append(context, command.reservation).await?;
        if reservation.object() != command.write.descriptor() {
            return self
                .abandon_before_journal(
                    context,
                    reservation,
                    None,
                    ApplicationError::Policy(
                        "guarded reservation differs from host archive write".to_owned(),
                    ),
                )
                .await;
        }
        let staged = match self.object_store.stage_encrypted(command.write).await {
            Ok(staged) => staged,
            Err(error) => {
                return self
                    .abandon_before_journal(context, reservation, None, error)
                    .await;
            }
        };
        if staged.descriptor() != reservation.object() {
            return self
                .abandon_before_journal(
                    context,
                    reservation,
                    Some(staged),
                    ApplicationError::Policy(
                        "host staging returned an object other than the guarded reservation"
                            .to_owned(),
                    ),
                )
                .await;
        }
        if let Err(error) = self.object_store.verify_durable(&staged).await {
            return self
                .abandon_before_journal(context, reservation, Some(staged), error)
                .await;
        }
        let head = match self.witness.read_head().await.map_err(witness_error) {
            Ok(head) => head,
            Err(error) => {
                return self
                    .abandon_before_journal(context, reservation, Some(staged), error)
                    .await;
            }
        };
        let expected_state = match expected_checkpoint_state(&head, &reservation) {
            Ok(state) => state,
            Err(error) => {
                return self
                    .abandon_before_journal(context, reservation, Some(staged), error)
                    .await;
            }
        };
        if command.signed_entry.state().backup_archive_state() != &expected_state {
            return self
                .abandon_before_journal(
                    context,
                    reservation,
                    Some(staged),
                    ApplicationError::Policy(
                        "signed archive entry does not carry the exact reserved checkpoint state"
                            .to_owned(),
                    ),
                )
                .await;
        }
        let advance = match head.accept_signed(&command.signed_entry) {
            Ok(advance) => advance,
            Err(error) => {
                return self
                    .abandon_before_journal(
                        context,
                        reservation,
                        Some(staged),
                        ApplicationError::Policy(error.to_string()),
                    )
                    .await;
            }
        };
        if let Err(error) = self.journal.append(&command.signed_entry).await {
            return self
                .abandon_before_journal(context, reservation, Some(staged), error)
                .await;
        }
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        let commit = CommitArchiveCheckpoint::new(reservation, staged)?;
        let staged = commit.staged().clone();
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let snapshot = self
            .repository
            .commit_checkpoint_in(
                permit.unit_of_work_mut(),
                commit,
                command.signed_entry,
                receipt,
            )
            .await?;
        permit.commit().await?;
        self.object_store.promote_after_checkpoint(&staged).await?;
        Ok(snapshot)
    }

    /// Acquires a non-expiring restore hold through the same signed authority
    /// chain as a checkpoint. The controller deliberately has no release
    /// method: P05-C must supply one of the terminal target/source receipts.
    pub async fn acquire_restore_hold(
        &self,
        context: &InstallationSupervisorContext,
        command: AcquireRestoreHold,
        signed_entry: SignedJournalEntry,
    ) -> Result<BackupArchiveSnapshot, ApplicationError> {
        if signed_entry.event_kind() != SafetyEventKind::ArchiveRestoreHoldAcquired {
            return Err(ApplicationError::Policy(
                "restore-hold acquisition requires its dedicated signed event".to_owned(),
            ));
        }
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let head = self.witness.read_head().await.map_err(witness_error)?;
        let expected_state = head
            .state()
            .backup_archive_state()
            .acquire_restore_hold(command.set_id(), command.hold_id())
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        if signed_entry.state().backup_archive_state() != &expected_state {
            return Err(ApplicationError::Policy(
                "signed restore-hold entry does not carry the exact hold state".to_owned(),
            ));
        }
        let advance = head
            .accept_signed(&signed_entry)
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        self.journal.append(&signed_entry).await?;
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        let snapshot = self
            .repository
            .acquire_hold_in(permit.unit_of_work_mut(), command, signed_entry, receipt)
            .await?;
        permit.commit().await?;
        Ok(snapshot)
    }

    /// Records one lifecycle edge after the signed entry and witness receipt
    /// are durable. Key erasure itself remains a separate host-custody action
    /// between the prepared and `ArchiveKeyErased` receipts.
    pub async fn transition_lifecycle(
        &self,
        context: &InstallationSupervisorContext,
        transition: ArchiveLifecycleTransition,
        signed_entry: SignedJournalEntry,
    ) -> Result<BackupArchiveSnapshot, ApplicationError> {
        if signed_entry.event_kind() != transition.event_kind() {
            return Err(ApplicationError::Policy(
                "archive lifecycle transition has the wrong signed event kind".to_owned(),
            ));
        }
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let head = self.witness.read_head().await.map_err(witness_error)?;
        let expected_state =
            expected_lifecycle_state(head.state().backup_archive_state(), transition)?;
        if signed_entry.state().backup_archive_state() != &expected_state {
            return Err(ApplicationError::Policy(
                "signed archive lifecycle entry does not carry the exact successor state"
                    .to_owned(),
            ));
        }
        let advance = head
            .accept_signed(&signed_entry)
            .map_err(|error| ApplicationError::Policy(error.to_string()))?;
        self.journal.append(&signed_entry).await?;
        let receipt = self
            .witness
            .compare_and_advance(head, advance)
            .await
            .map_err(witness_error)?;
        let snapshot = self
            .repository
            .transition_lifecycle_in(permit.unit_of_work_mut(), transition, signed_entry, receipt)
            .await?;
        permit.commit().await?;
        Ok(snapshot)
    }

    /// Fetches the exact deletion manifest in a short transaction. Host file
    /// removal intentionally happens only after this permit is committed.
    pub async fn prepared_deletion_manifest(
        &self,
        context: &InstallationSupervisorContext,
        prepared: ManagedBackupDeletionPrepared,
    ) -> Result<Vec<ArchiveObjectDescriptor>, ApplicationError> {
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let head = self.witness.read_head().await.map_err(witness_error)?;
        let archive = head.state().backup_archive_state();
        if archive
            .lifecycle(prepared.set_id())
            .map_err(|error| ApplicationError::Policy(error.to_string()))?
            != vestrace_domain::BackupSetLifecycle::ArchiveKeyErased
            || archive
                .deletion_preparation_digest(prepared.set_id())
                .map_err(|error| ApplicationError::Policy(error.to_string()))?
                != Some(prepared.preparation_digest())
            || archive
                .key_erasure_preparation_digest(prepared.set_id())
                .map_err(|error| ApplicationError::Policy(error.to_string()))?
                != Some(prepared.preparation_digest())
        {
            return Err(ApplicationError::Policy(
                "prepared archive deletion is not the current key-erased signed state".to_owned(),
            ));
        }
        let objects = self
            .repository
            .list_prepared_archive_objects_in(permit.unit_of_work_mut(), prepared)
            .await?;
        permit.commit().await?;
        Ok(objects)
    }

    pub async fn remove_prepared_archive_object(
        &self,
        prepared: ManagedBackupDeletionPrepared,
        object: &ArchiveObjectDescriptor,
    ) -> Result<(), ApplicationError> {
        if object.set_id() != prepared.set_id() {
            return Err(ApplicationError::Policy(
                "prepared deletion manifest object names another backup set".to_owned(),
            ));
        }
        self.object_store
            .remove_prepared_committed(prepared, object)
            .await
    }

    async fn reserve_append(
        &self,
        context: &InstallationSupervisorContext,
        reservation: ReserveArchiveAppend,
    ) -> Result<ArchiveAppendReservation, ApplicationError> {
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        let reservation = self
            .repository
            .reserve_append_in(permit.unit_of_work_mut(), reservation)
            .await?;
        permit.commit().await?;
        Ok(reservation)
    }

    async fn abandon_before_journal<T>(
        &self,
        context: &InstallationSupervisorContext,
        reservation: ArchiveAppendReservation,
        staged: Option<StagedArchiveObject>,
        original: ApplicationError,
    ) -> Result<T, ApplicationError> {
        let mut permit = self
            .permit
            .acquire(PermitMode::Exclusive, context.request_context())
            .await?;
        self.repository
            .abandon_append_in(
                permit.unit_of_work_mut(),
                AbandonArchiveAppend::new(reservation),
            )
            .await?;
        permit.commit().await?;
        match staged {
            Some(staged) => {
                self.remove_orphan_after_pre_journal_failure(staged, original)
                    .await
            }
            None => Err(original),
        }
    }

    async fn discard_uncommitted_key<T>(
        &self,
        set: BackupSetId,
        original: ApplicationError,
    ) -> Result<T, ApplicationError> {
        self.key_custody
            .discard_uncommitted_set_key(set)
            .await
            .map_err(|cleanup| {
                ApplicationError::Unavailable(format!(
                    "backup start failed before journalling ({original}); exact key cleanup also failed: {cleanup}"
                ))
            })?;
        Err(original)
    }

    async fn remove_orphan_after_pre_journal_failure<T>(
        &self,
        staged: StagedArchiveObject,
        original: ApplicationError,
    ) -> Result<T, ApplicationError> {
        match self.object_store.remove_orphan(&staged).await {
            Ok(()) => Err(original),
            Err(cleanup) => Err(ApplicationError::Unavailable(format!(
                "archive staging failed before journal and exact orphan cleanup also failed: {cleanup}"
            ))),
        }
    }
}

fn expected_checkpoint_state(
    head: &WitnessHead,
    reservation: &ArchiveAppendReservation,
) -> Result<BackupArchiveStateV1, ApplicationError> {
    let checkpoint = WalArchiveCheckpoint::from_reservation(reservation);
    head.state()
        .backup_archive_state()
        .commit_checkpoint(reservation.clone(), checkpoint)
        .map_err(|error| ApplicationError::Policy(error.to_string()))
}

fn expected_lifecycle_state(
    state: &BackupArchiveStateV1,
    transition: ArchiveLifecycleTransition,
) -> Result<BackupArchiveStateV1, ApplicationError> {
    let next = match transition {
        ArchiveLifecycleTransition::BeginSealing { set_id } => state.begin_sealing(set_id),
        ArchiveLifecycleTransition::CommitSealed { set_id } => state.commit_sealed(set_id),
        ArchiveLifecycleTransition::PrepareDeletion { prepared } => {
            state.prepare_deletion(prepared.set_id(), prepared.preparation_digest())
        }
        ArchiveLifecycleTransition::PrepareArchiveKeyErasure { prepared } => {
            state.prepare_archive_key_erasure(prepared.set_id(), prepared.preparation_digest())
        }
        ArchiveLifecycleTransition::RecordArchiveKeyErased { prepared } => {
            state.record_archive_key_erased(prepared.set_id())
        }
        ArchiveLifecycleTransition::FinalizeDeleted { set_id } => state.finalize_deleted(set_id),
    };
    next.map_err(|error| ApplicationError::Policy(error.to_string()))
}

fn witness_error(error: WitnessError) -> ApplicationError {
    match error {
        WitnessError::Conflict => ApplicationError::Conflict("witness head changed".to_owned()),
        WitnessError::Unavailable(message) => ApplicationError::Unavailable(message),
        WitnessError::Invalid(error) => ApplicationError::Policy(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::{any::Any, sync::Mutex};

    use async_trait::async_trait;
    use ring::{rand::SystemRandom, signature::Ed25519KeyPair};
    use vestrace_domain::{
        ArchiveHead, ArchiveObjectDescriptorInput, ArchiveObjectKind, BackupObjectId,
        BackupSetIdentity, DatabaseGenerationId, FingerprintKeyContinuityProof, FingerprintKeyId,
        InstallationId, JournalEntryToSign, RequestId, WitnessStateV1,
    };

    use super::*;
    use crate::PermitHandle;

    #[test]
    fn archive_write_binds_ciphertext_to_the_immutable_descriptor() {
        let set = BackupSetId::new();
        let ciphertext = b"encrypted-wal".to_vec();
        let descriptor = ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
            set_id: set,
            object_id: BackupObjectId::new(),
            kind: ArchiveObjectKind::BaseChunk,
            ordinal: 1,
            timeline: 1,
            start_lsn: 8,
            end_lsn: 16,
            ciphertext_digest: SafetyJournalDigest::of(&ciphertext),
            plaintext_digest: SafetyJournalDigest::of(b"plaintext-digest"),
            length: ciphertext.len() as u64,
            predecessor_head_digest: ArchiveHead::genesis(SafetyJournalDigest::of(b"head")).digest,
        })
        .unwrap();
        assert!(ArchiveObjectWrite::new(descriptor.clone(), ciphertext).is_ok());
        assert!(matches!(
            ArchiveObjectWrite::new(descriptor, b"different-ciphertext".to_vec()),
            Err(ApplicationError::Policy(_))
        ));
    }

    #[test]
    fn reserve_command_binds_the_object_to_its_exact_predecessor_head() {
        let set = BackupSetId::new();
        let head = ArchiveHead::genesis(SafetyJournalDigest::of(b"head"));
        let descriptor = ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
            set_id: set,
            object_id: BackupObjectId::new(),
            kind: ArchiveObjectKind::WalSegment,
            ordinal: 1,
            timeline: 1,
            start_lsn: 8,
            end_lsn: 16,
            ciphertext_digest: SafetyJournalDigest::of(b"ciphertext"),
            plaintext_digest: SafetyJournalDigest::of(b"plaintext"),
            length: 10,
            predecessor_head_digest: head.digest,
        })
        .unwrap();
        assert!(ReserveArchiveAppend::new(set, head, descriptor).is_ok());
    }

    #[tokio::test]
    async fn append_reservation_commits_before_any_host_staging() {
        let (reservation, write) = reservation_and_write();
        let trace = Trace::default();
        let controller = BackupArchiveController::new(
            Arc::new(FakePermit(trace.clone())),
            Arc::new(FakeJournal),
            Arc::new(FakeWitness),
            Arc::new(FakeRepository {
                trace: trace.clone(),
                reservation: reservation.clone(),
            }),
            Arc::new(FailingDurabilityStore {
                trace: trace.clone(),
            }),
            Arc::new(FakeKeyCustody),
        );

        let _ = write;
        assert!(
            controller
                .reserve_append(
                    &InstallationSupervisorContext::host_supervisor(),
                    reservation,
                )
                .await
                .is_ok()
        );
        assert_eq!(trace.events(), ["exclusive", "reserve", "commit"]);
    }

    #[tokio::test]
    async fn backup_start_discards_only_its_unjournalled_key_envelope_when_witness_is_unavailable()
    {
        let (reservation, _) = reservation_and_write();
        let trace = Trace::default();
        let command = start_command(reservation.set_id());
        let controller = BackupArchiveController::new(
            Arc::new(FakePermit(trace.clone())),
            Arc::new(FakeJournal),
            Arc::new(FailingStartWitness),
            Arc::new(FakeRepository {
                trace: trace.clone(),
                reservation,
            }),
            Arc::new(FailingDurabilityStore {
                trace: trace.clone(),
            }),
            Arc::new(StartKeyCustody {
                trace: trace.clone(),
            }),
        );

        assert!(matches!(
            controller
                .start(&InstallationSupervisorContext::host_supervisor(), command)
                .await,
            Err(ApplicationError::Unavailable(_))
        ));
        assert_eq!(trace.events(), ["exclusive", "key-create", "key-discard"]);
    }

    fn start_command(set: BackupSetId) -> StartManagedBackup {
        let signer = Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        let installation = InstallationId::new();
        let generation = DatabaseGenerationId::new();
        let state = WitnessStateV1::genesis(generation);
        let entry = SignedJournalEntry::sign(
            &signer,
            JournalEntryToSign {
                installation_id: installation,
                fingerprint_key_id: FingerprintKeyId::new(),
                continuity_proof: FingerprintKeyContinuityProof::from_bytes([7; 32]),
                request_id: RequestId::new(),
                sequence: 1,
                previous_digest: SafetyJournalDigest::of(b"test-start-previous"),
                event_kind: SafetyEventKind::BackupSetStarted,
                generation_id: generation,
                activation_epoch: 0,
                state,
            },
        );
        StartManagedBackup::new(
            BackupSetIdentity::new(set, installation, generation),
            ArchiveHead::genesis(SafetyJournalDigest::of(b"test-initial-head")),
            entry,
        )
    }

    fn reservation_and_write() -> (ReserveArchiveAppend, ArchiveObjectWrite) {
        let set = BackupSetId::new();
        let head = ArchiveHead::genesis(SafetyJournalDigest::of(b"head"));
        let ciphertext = b"encrypted-wal".to_vec();
        let descriptor = ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
            set_id: set,
            object_id: BackupObjectId::new(),
            kind: ArchiveObjectKind::BaseChunk,
            ordinal: 1,
            timeline: 1,
            start_lsn: 8,
            end_lsn: 16,
            ciphertext_digest: SafetyJournalDigest::of(&ciphertext),
            plaintext_digest: SafetyJournalDigest::of(b"plaintext"),
            length: ciphertext.len() as u64,
            predecessor_head_digest: head.digest,
        })
        .unwrap();
        (
            ReserveArchiveAppend::new(set, head, descriptor.clone()).unwrap(),
            ArchiveObjectWrite::new(descriptor, ciphertext).unwrap(),
        )
    }

    #[derive(Clone, Default)]
    struct Trace(Arc<Mutex<Vec<&'static str>>>);

    impl Trace {
        fn push(&self, event: &'static str) {
            self.0.lock().unwrap().push(event);
        }

        fn events(&self) -> Vec<&'static str> {
            self.0.lock().unwrap().clone()
        }
    }

    struct FakeUnit(Trace);

    #[async_trait]
    impl UnitOfWork for FakeUnit {
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        async fn commit(self: Box<Self>) -> Result<(), ApplicationError> {
            self.0.push("commit");
            Ok(())
        }

        async fn rollback(self: Box<Self>) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    struct FakePermit(Trace);

    #[async_trait]
    impl InstallationMutationPermit for FakePermit {
        async fn acquire(
            &self,
            mode: PermitMode,
            _: &crate::RequestContext,
        ) -> Result<PermitHandle, ApplicationError> {
            assert_eq!(mode, PermitMode::Exclusive);
            self.0.push("exclusive");
            Ok(PermitHandle::new(Box::new(FakeUnit(self.0.clone()))))
        }
    }

    struct FakeRepository {
        trace: Trace,
        reservation: ReserveArchiveAppend,
    }

    #[async_trait]
    impl BackupArchiveRepository for FakeRepository {
        async fn start_set_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: StartManagedBackup,
            _: ArchiveKeyEnvelopeRef,
            _: SignedJournalEntry,
            _: WitnessReceipt,
        ) -> Result<BackupArchiveSnapshot, ApplicationError> {
            unreachable!("append test never starts a backup set")
        }

        async fn reserve_append_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: ReserveArchiveAppend,
        ) -> Result<ArchiveAppendReservation, ApplicationError> {
            self.trace.push("reserve");
            let archive = BackupArchiveStateV1::empty()
                .start_streaming(
                    vestrace_domain::BackupSetIdentity::new(
                        self.reservation.set_id(),
                        vestrace_domain::InstallationId::new(),
                        vestrace_domain::DatabaseGenerationId::new(),
                    ),
                    self.reservation.expected_head(),
                )
                .map_err(|error| ApplicationError::Policy(error.to_string()))?;
            archive
                .reserve_append(
                    self.reservation.set_id(),
                    self.reservation.expected_head(),
                    self.reservation.object().clone(),
                )
                .map_err(|error| ApplicationError::Policy(error.to_string()))
        }

        async fn abandon_append_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: AbandonArchiveAppend,
        ) -> Result<(), ApplicationError> {
            self.trace.push("abandon");
            Ok(())
        }

        async fn list_pending_appends_in(
            &self,
            _: &mut dyn UnitOfWork,
        ) -> Result<Vec<PendingArchiveAppend>, ApplicationError> {
            Ok(Vec::new())
        }

        async fn abandon_pending_append_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: PendingArchiveAppend,
        ) -> Result<(), ApplicationError> {
            unreachable!("append unit test does not reconcile pending intents")
        }

        async fn commit_checkpoint_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: CommitArchiveCheckpoint,
            _: SignedJournalEntry,
            _: WitnessReceipt,
        ) -> Result<BackupArchiveSnapshot, ApplicationError> {
            panic!("durability failure must not reach checkpoint commit")
        }

        async fn acquire_hold_in(
            &self,
            _: &mut dyn UnitOfWork,
            _: AcquireRestoreHold,
            _: SignedJournalEntry,
            _: WitnessReceipt,
        ) -> Result<BackupArchiveSnapshot, ApplicationError> {
            unreachable!("append test never acquires a restore hold")
        }
    }

    struct FailingDurabilityStore {
        trace: Trace,
    }

    #[async_trait]
    impl BackupObjectStore for FailingDurabilityStore {
        async fn stage_encrypted(
            &self,
            write: ArchiveObjectWrite,
        ) -> Result<StagedArchiveObject, ApplicationError> {
            self.trace.push("stage");
            Ok(StagedArchiveObject::new(
                ArchiveStagingId::new(),
                write.descriptor().clone(),
            ))
        }

        async fn verify_durable(&self, _: &StagedArchiveObject) -> Result<(), ApplicationError> {
            self.trace.push("durable");
            Err(ApplicationError::Unavailable(
                "injected fsync failure".to_owned(),
            ))
        }

        async fn promote_after_checkpoint(
            &self,
            _: &StagedArchiveObject,
        ) -> Result<(), ApplicationError> {
            self.trace.push("promote");
            Ok(())
        }

        async fn remove_orphan(&self, _: &StagedArchiveObject) -> Result<(), ApplicationError> {
            self.trace.push("orphan");
            Ok(())
        }

        async fn remove_committed(
            &self,
            _: &ArchiveObjectDescriptor,
        ) -> Result<(), ApplicationError> {
            unreachable!("durability failure must not remove a committed object")
        }
    }

    struct FakeJournal;

    #[async_trait]
    impl SafetyJournal for FakeJournal {
        async fn append(&self, _: &SignedJournalEntry) -> Result<(), ApplicationError> {
            panic!("durability failure must not write a journal entry")
        }
    }

    struct FakeWitness;

    #[async_trait]
    impl InstallationSafetyWitness for FakeWitness {
        async fn read_head(&self) -> Result<WitnessHead, WitnessError> {
            panic!("durability failure must not contact the witness")
        }

        async fn compare_and_advance(
            &self,
            _: WitnessHead,
            _: vestrace_domain::WitnessAdvance,
        ) -> Result<WitnessReceipt, WitnessError> {
            panic!("durability failure must not advance the witness")
        }
    }

    struct FailingStartWitness;

    #[async_trait]
    impl InstallationSafetyWitness for FailingStartWitness {
        async fn read_head(&self) -> Result<WitnessHead, WitnessError> {
            Err(WitnessError::Unavailable(
                "injected witness unavailability".to_owned(),
            ))
        }

        async fn compare_and_advance(
            &self,
            _: WitnessHead,
            _: vestrace_domain::WitnessAdvance,
        ) -> Result<WitnessReceipt, WitnessError> {
            unreachable!("unavailable witness cannot advance")
        }
    }

    struct FakeKeyCustody;

    #[async_trait]
    impl ArchiveKeyCustody for FakeKeyCustody {
        async fn create_set_key(
            &self,
            _: BackupSetId,
        ) -> Result<ArchiveKeyEnvelopeRef, ApplicationError> {
            unreachable!("append test does not create a set key")
        }

        async fn discard_uncommitted_set_key(
            &self,
            _: BackupSetId,
        ) -> Result<(), ApplicationError> {
            unreachable!("append test does not create a set key")
        }

        async fn erase_prepared_key(
            &self,
            _: ManagedBackupDeletionPrepared,
        ) -> Result<ArchiveKeyErasureReceipt, ApplicationError> {
            unreachable!("append test does not erase a key")
        }
    }

    struct StartKeyCustody {
        trace: Trace,
    }

    #[async_trait]
    impl ArchiveKeyCustody for StartKeyCustody {
        async fn create_set_key(
            &self,
            set: BackupSetId,
        ) -> Result<ArchiveKeyEnvelopeRef, ApplicationError> {
            self.trace.push("key-create");
            Ok(ArchiveKeyEnvelopeRef::from_uuid(set.as_uuid()))
        }

        async fn discard_uncommitted_set_key(
            &self,
            _: BackupSetId,
        ) -> Result<(), ApplicationError> {
            self.trace.push("key-discard");
            Ok(())
        }

        async fn erase_prepared_key(
            &self,
            _: ManagedBackupDeletionPrepared,
        ) -> Result<ArchiveKeyErasureReceipt, ApplicationError> {
            unreachable!("backup start never erases a key")
        }
    }
}
