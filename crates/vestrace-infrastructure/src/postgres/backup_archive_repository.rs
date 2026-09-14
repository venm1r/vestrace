//! PostgreSQL binding for the guarded P05-B archive checkpoint calls.
//!
//! This adapter never writes an archive table directly.  The supervisor role
//! can invoke only the guarded routines installed during the quiesced P05
//! bootstrap, which re-check the signed journal and witness receipt.

use async_trait::async_trait;
use sqlx::Row;
use vestrace_application::{
    AbandonArchiveAppend, AcquireRestoreHold, ApplicationError, ArchiveKeyEnvelopeRef,
    ArchiveLifecycleTransition, BackupArchiveRepository, BackupArchiveSnapshot,
    CommitArchiveCheckpoint, PendingArchiveAppend, ReserveArchiveAppend, StartManagedBackup,
    UnitOfWork,
};
use vestrace_domain::{
    ArchiveAppendReservation, ArchiveObjectDescriptor, ArchiveObjectDescriptorInput,
    ArchiveObjectKind, BackupObjectId, SafetyEventKind, SignedJournalEntry, WitnessReceipt,
};

use super::{PgStore, transaction::PgScopedTransaction};

macro_rules! bind_archive_authority {
    ($query:expr, $entry:expr, $receipt:expr) => {
        $query
            .bind($entry.installation_id().as_uuid())
            .bind($entry.fingerprint_key_id().as_uuid())
            .bind($entry.continuity_proof().as_bytes().as_slice())
            .bind($entry.request_id().as_uuid())
            .bind(as_i64($entry.sequence(), "journal sequence")?)
            .bind($entry.previous_digest().as_bytes().as_slice())
            .bind($entry.digest().as_bytes().as_slice())
            .bind($entry.signature().as_slice())
            .bind($receipt.generation_id().as_uuid())
            .bind(as_i64($receipt.activation_epoch(), "activation epoch")?)
            .bind($receipt.state().canonical_bytes())
            .bind($receipt.signature().as_slice())
    };
}

#[derive(Clone, Debug)]
pub struct PgBackupArchiveRepository {
    store: PgStore,
}

impl PgBackupArchiveRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }

    fn transaction(
        unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<&mut PgScopedTransaction, ApplicationError> {
        unit_of_work
            .as_any_mut()
            .downcast_mut::<PgScopedTransaction>()
            .ok_or_else(|| ApplicationError::Internal("expected PostgreSQL transaction".to_owned()))
    }
}

#[async_trait]
impl BackupArchiveRepository for PgBackupArchiveRepository {
    async fn list_prepared_archive_objects_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        prepared: vestrace_application::ManagedBackupDeletionPrepared,
    ) -> Result<Vec<ArchiveObjectDescriptor>, ApplicationError> {
        let transaction = Self::transaction(unit_of_work)?;
        let rows = sqlx::query(
            "SELECT object_id, ordinal, object_kind, timeline, start_lsn, end_lsn, object_digest, plaintext_digest, object_length, predecessor_head_digest \
             FROM public.vestrace_list_prepared_backup_archive_objects($1,$2)",
        )
        .bind(prepared.set_id().as_uuid())
        .bind(prepared.preparation_digest().as_bytes().as_slice())
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        rows.into_iter()
            .map(|row| {
                let kind: i16 = row.try_get("object_kind").map_err(storage_error)?;
                let ordinal: i64 = row.try_get("ordinal").map_err(storage_error)?;
                let timeline: i32 = row.try_get("timeline").map_err(storage_error)?;
                let start_lsn: i64 = row.try_get("start_lsn").map_err(storage_error)?;
                let end_lsn: i64 = row.try_get("end_lsn").map_err(storage_error)?;
                let length: i64 = row.try_get("object_length").map_err(storage_error)?;
                ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
                    set_id: prepared.set_id(),
                    object_id: BackupObjectId::from_uuid(
                        row.try_get("object_id").map_err(storage_error)?,
                    ),
                    kind: archive_object_kind(kind)?,
                    ordinal: u64::try_from(ordinal).map_err(|_| {
                        ApplicationError::Storage("negative prepared archive ordinal".to_owned())
                    })?,
                    timeline: u32::try_from(timeline).map_err(|_| {
                        ApplicationError::Storage("negative prepared archive timeline".to_owned())
                    })?,
                    start_lsn: u64::try_from(start_lsn).map_err(|_| {
                        ApplicationError::Storage("negative prepared archive start LSN".to_owned())
                    })?,
                    end_lsn: u64::try_from(end_lsn).map_err(|_| {
                        ApplicationError::Storage("negative prepared archive end LSN".to_owned())
                    })?,
                    ciphertext_digest: digest_from_bytes(
                        row.try_get("object_digest").map_err(storage_error)?,
                        "prepared archive object digest",
                    )?,
                    plaintext_digest: digest_from_bytes(
                        row.try_get("plaintext_digest").map_err(storage_error)?,
                        "prepared archive plaintext digest",
                    )?,
                    length: u64::try_from(length).map_err(|_| {
                        ApplicationError::Storage("negative prepared archive length".to_owned())
                    })?,
                    predecessor_head_digest: digest_from_bytes(
                        row.try_get("predecessor_head_digest")
                            .map_err(storage_error)?,
                        "prepared archive predecessor digest",
                    )?,
                })
                .map_err(|error| ApplicationError::Storage(error.to_string()))
            })
            .collect()
    }

    async fn start_set_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        command: StartManagedBackup,
        envelope: ArchiveKeyEnvelopeRef,
        entry: SignedJournalEntry,
        receipt: WitnessReceipt,
    ) -> Result<BackupArchiveSnapshot, ApplicationError> {
        validate_entry_and_receipt(&entry, &receipt)?;
        if entry.event_kind() != SafetyEventKind::BackupSetStarted {
            return Err(ApplicationError::Policy(
                "managed backup start requires its dedicated signed event".to_owned(),
            ));
        }
        let identity = command.identity();
        if identity.installation_id() != entry.installation_id()
            || identity.generation_id() != entry.generation_id()
        {
            return Err(ApplicationError::Policy(
                "managed backup identity does not match signed safety authority".to_owned(),
            ));
        }
        let transaction = Self::transaction(unit_of_work)?;
        sqlx::query("SELECT public.vestrace_start_managed_backup_set($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)")
            .bind(identity.set_id().as_uuid())
            .bind(command.initial_head().digest.as_bytes().as_slice())
            .bind(envelope.as_uuid())
            .bind(entry.installation_id().as_uuid())
            .bind(entry.fingerprint_key_id().as_uuid())
            .bind(entry.continuity_proof().as_bytes().as_slice())
            .bind(entry.request_id().as_uuid())
            .bind(as_i64(entry.sequence(), "journal sequence")?)
            .bind(entry.previous_digest().as_bytes().as_slice())
            .bind(entry.digest().as_bytes().as_slice())
            .bind(entry.signature().as_slice())
            .bind(receipt.generation_id().as_uuid())
            .bind(as_i64(receipt.activation_epoch(), "activation epoch")?)
            .bind(receipt.state().canonical_bytes())
            .bind(receipt.signature().as_slice())
            .execute(transaction.connection())
            .await
            .map_err(storage_error)?;
        Ok(BackupArchiveSnapshot::new(
            entry.state().backup_archive_state().clone(),
        ))
    }

    async fn reserve_append_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        command: ReserveArchiveAppend,
    ) -> Result<ArchiveAppendReservation, ApplicationError> {
        let expected = command.expected_head();
        let object = command.object().to_input();
        let ordinal = i64::try_from(expected.checkpoint_ordinal).map_err(|_| {
            ApplicationError::Policy("archive ordinal exceeds PostgreSQL bigint".to_owned())
        })?;
        let transaction = Self::transaction(unit_of_work)?;
        // `object_id` is a caller-supplied immutable UUID in the descriptor;
        // using it as the unique intent identity makes commit bind precisely
        // the member that was staged, with no hidden set-wide lookup.
        sqlx::query("SELECT public.vestrace_reserve_backup_archive_append($1, $2, $3, $4, $5)")
            .bind(command.set_id().as_uuid())
            .bind(ordinal)
            .bind(expected.digest.as_bytes().as_slice())
            .bind(object.object_id.as_uuid())
            .bind(object.ciphertext_digest.as_bytes().as_slice())
            .execute(transaction.connection())
            .await
            .map_err(storage_error)?;
        let _ = &self.store;
        ArchiveAppendReservation::new(command.set_id(), expected, command.object().clone())
            .map_err(|error| ApplicationError::Policy(error.to_string()))
    }

    async fn abandon_append_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        command: AbandonArchiveAppend,
    ) -> Result<(), ApplicationError> {
        let reservation = command.reservation();
        let expected = reservation.expected_head();
        let object = reservation.object().to_input();
        let transaction = Self::transaction(unit_of_work)?;
        sqlx::query("SELECT public.vestrace_abandon_backup_archive_append($1,$2,$3,$4,$5)")
            .bind(reservation.set_id().as_uuid())
            .bind(as_i64(expected.checkpoint_ordinal, "archive ordinal")?)
            .bind(expected.digest.as_bytes().as_slice())
            .bind(object.object_id.as_uuid())
            .bind(object.ciphertext_digest.as_bytes().as_slice())
            .execute(transaction.connection())
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn list_pending_appends_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
    ) -> Result<Vec<PendingArchiveAppend>, ApplicationError> {
        let transaction = Self::transaction(unit_of_work)?;
        let rows = sqlx::query(
            "SELECT backup_set_id, intent_id, ordinal, expected_head_digest, object_digest \
             FROM public.vestrace_list_pending_backup_archive_appends()",
        )
        .fetch_all(transaction.connection())
        .await
        .map_err(storage_error)?;
        rows.into_iter()
            .map(|row| {
                let set = vestrace_domain::BackupSetId::from_uuid(
                    row.try_get("backup_set_id").map_err(storage_error)?,
                );
                let intent = vestrace_domain::BackupObjectId::from_uuid(
                    row.try_get("intent_id").map_err(storage_error)?,
                );
                let ordinal: i64 = row.try_get("ordinal").map_err(storage_error)?;
                let digest: Vec<u8> = row.try_get("expected_head_digest").map_err(storage_error)?;
                let object_digest: Vec<u8> = row.try_get("object_digest").map_err(storage_error)?;
                let expected = vestrace_domain::ArchiveHead {
                    checkpoint_ordinal: u64::try_from(ordinal)
                        .map_err(|_| {
                            ApplicationError::Storage("negative archive intent ordinal".to_owned())
                        })?
                        .checked_sub(1)
                        .ok_or_else(|| {
                            ApplicationError::Storage("zero archive intent ordinal".to_owned())
                        })?,
                    digest: digest_from_bytes(digest, "archive intent expected digest")?,
                };
                Ok(PendingArchiveAppend::new(
                    set,
                    expected,
                    intent,
                    digest_from_bytes(object_digest, "archive intent object digest")?,
                ))
            })
            .collect()
    }

    async fn abandon_pending_append_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        pending: PendingArchiveAppend,
    ) -> Result<(), ApplicationError> {
        let transaction = Self::transaction(unit_of_work)?;
        sqlx::query("SELECT public.vestrace_abandon_backup_archive_append($1,$2,$3,$4,$5)")
            .bind(pending.set_id().as_uuid())
            .bind(as_i64(
                pending.expected_head().checkpoint_ordinal,
                "archive ordinal",
            )?)
            .bind(pending.expected_head().digest.as_bytes().as_slice())
            .bind(pending.object_id().as_uuid())
            .bind(pending.ciphertext_digest().as_bytes().as_slice())
            .execute(transaction.connection())
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn commit_checkpoint_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        command: CommitArchiveCheckpoint,
        entry: SignedJournalEntry,
        receipt: WitnessReceipt,
    ) -> Result<BackupArchiveSnapshot, ApplicationError> {
        validate_entry_and_receipt(&entry, &receipt)?;
        if entry.event_kind() != SafetyEventKind::ArchiveCheckpointCommitted {
            return Err(ApplicationError::Policy(
                "archive checkpoint requires its dedicated signed event".to_owned(),
            ));
        }
        let reservation = command.reservation();
        let object = reservation.object().to_input();
        let checkpoint = vestrace_domain::WalArchiveCheckpoint::from_reservation(reservation);
        let transaction = Self::transaction(unit_of_work)?;
        sqlx::query("SELECT public.vestrace_commit_backup_archive_checkpoint($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25)")
            .bind(reservation.set_id().as_uuid())
            .bind(object.object_id.as_uuid())
            .bind(as_i64(object.ordinal, "archive ordinal")?)
            .bind(object.object_id.as_uuid())
            .bind(object_kind_code(object.kind))
            .bind(i32::try_from(object.timeline).map_err(|_| ApplicationError::Policy("archive timeline exceeds PostgreSQL integer".to_owned()))?)
            .bind(as_i64(object.start_lsn, "archive start LSN")?)
            .bind(as_i64(object.end_lsn, "archive end LSN")?)
            .bind(object.ciphertext_digest.as_bytes().as_slice())
            .bind(object.plaintext_digest.as_bytes().as_slice())
            .bind(as_i64(object.length, "archive object length")?)
            .bind(object.predecessor_head_digest.as_bytes().as_slice())
            .bind(checkpoint.digest().as_bytes().as_slice())
            .bind(entry.installation_id().as_uuid())
            .bind(entry.fingerprint_key_id().as_uuid())
            .bind(entry.continuity_proof().as_bytes().as_slice())
            .bind(entry.request_id().as_uuid())
            .bind(as_i64(entry.sequence(), "journal sequence")?)
            .bind(entry.previous_digest().as_bytes().as_slice())
            .bind(entry.digest().as_bytes().as_slice())
            .bind(entry.signature().as_slice())
            .bind(receipt.generation_id().as_uuid())
            .bind(as_i64(receipt.activation_epoch(), "activation epoch")?)
            .bind(receipt.state().canonical_bytes())
            .bind(receipt.signature().as_slice())
            .execute(transaction.connection())
            .await
            .map_err(storage_error)?;
        let _ = &self.store;
        Ok(BackupArchiveSnapshot::new(
            entry.state().backup_archive_state().clone(),
        ))
    }

    async fn acquire_hold_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        command: AcquireRestoreHold,
        entry: SignedJournalEntry,
        receipt: WitnessReceipt,
    ) -> Result<BackupArchiveSnapshot, ApplicationError> {
        validate_entry_and_receipt(&entry, &receipt)?;
        if entry.event_kind() != SafetyEventKind::ArchiveRestoreHoldAcquired {
            return Err(ApplicationError::Policy(
                "restore-hold acquisition requires its dedicated signed event".to_owned(),
            ));
        }
        let transaction = Self::transaction(unit_of_work)?;
        sqlx::query("SELECT public.vestrace_acquire_managed_backup_restore_hold($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
            .bind(command.set_id().as_uuid())
            .bind(command.hold_id().as_uuid())
            .bind(entry.installation_id().as_uuid())
            .bind(entry.fingerprint_key_id().as_uuid())
            .bind(entry.continuity_proof().as_bytes().as_slice())
            .bind(entry.request_id().as_uuid())
            .bind(as_i64(entry.sequence(), "journal sequence")?)
            .bind(entry.previous_digest().as_bytes().as_slice())
            .bind(entry.digest().as_bytes().as_slice())
            .bind(entry.signature().as_slice())
            .bind(receipt.generation_id().as_uuid())
            .bind(as_i64(receipt.activation_epoch(), "activation epoch")?)
            .bind(receipt.state().canonical_bytes())
            .bind(receipt.signature().as_slice())
            .execute(transaction.connection())
            .await
            .map_err(storage_error)?;
        Ok(BackupArchiveSnapshot::new(
            entry.state().backup_archive_state().clone(),
        ))
    }

    async fn transition_lifecycle_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        transition: ArchiveLifecycleTransition,
        entry: SignedJournalEntry,
        receipt: WitnessReceipt,
    ) -> Result<BackupArchiveSnapshot, ApplicationError> {
        validate_entry_and_receipt(&entry, &receipt)?;
        if entry.event_kind() != transition.event_kind() {
            return Err(ApplicationError::Policy(
                "archive lifecycle event does not match its guarded transition".to_owned(),
            ));
        }
        let transaction = Self::transaction(unit_of_work)?;
        match transition {
            ArchiveLifecycleTransition::BeginSealing { set_id } => {
                bind_archive_authority!(
                    sqlx::query("SELECT public.vestrace_begin_managed_backup_sealing($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
                        .bind(set_id.as_uuid()),
                    entry,
                    receipt
                )
                    .execute(transaction.connection()).await
                    .map_err(storage_error)?;
            }
            ArchiveLifecycleTransition::CommitSealed { set_id } => {
                bind_archive_authority!(
                    sqlx::query("SELECT public.vestrace_commit_managed_backup_sealed($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
                        .bind(set_id.as_uuid()),
                    entry,
                    receipt
                )
                    .execute(transaction.connection()).await
                    .map_err(storage_error)?;
            }
            ArchiveLifecycleTransition::PrepareDeletion { prepared } => {
                bind_archive_authority!(
                    sqlx::query("SELECT public.vestrace_prepare_managed_backup_deletion($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
                        .bind(prepared.set_id().as_uuid())
                        .bind(prepared.preparation_digest().as_bytes().as_slice()),
                    entry,
                    receipt
                )
                    .execute(transaction.connection()).await
                    .map_err(storage_error)?;
            }
            ArchiveLifecycleTransition::PrepareArchiveKeyErasure { prepared } => {
                bind_archive_authority!(
                    sqlx::query("SELECT public.vestrace_prepare_archive_key_erasure($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
                        .bind(prepared.set_id().as_uuid())
                        .bind(prepared.preparation_digest().as_bytes().as_slice()),
                    entry,
                    receipt
                )
                    .execute(transaction.connection()).await
                    .map_err(storage_error)?;
            }
            ArchiveLifecycleTransition::RecordArchiveKeyErased { prepared } => {
                bind_archive_authority!(
                    sqlx::query("SELECT public.vestrace_record_archive_key_erased($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
                        .bind(prepared.set_id().as_uuid())
                        .bind(prepared.preparation_digest().as_bytes().as_slice()),
                    entry,
                    receipt
                )
                    .execute(transaction.connection()).await
                    .map_err(storage_error)?;
            }
            ArchiveLifecycleTransition::FinalizeDeleted { set_id } => {
                bind_archive_authority!(
                    sqlx::query("SELECT public.vestrace_finalize_managed_backup_deleted($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
                        .bind(set_id.as_uuid()),
                    entry,
                    receipt
                )
                    .execute(transaction.connection()).await
                    .map_err(storage_error)?;
            }
        }
        Ok(BackupArchiveSnapshot::new(
            entry.state().backup_archive_state().clone(),
        ))
    }
}

fn as_i64(value: u64, label: &str) -> Result<i64, ApplicationError> {
    i64::try_from(value)
        .map_err(|_| ApplicationError::Policy(format!("{label} exceeds PostgreSQL bigint")))
}

const fn object_kind_code(kind: ArchiveObjectKind) -> i16 {
    match kind {
        ArchiveObjectKind::BaseChunk => 0,
        ArchiveObjectKind::WalSegment => 1,
        ArchiveObjectKind::TimelineHistory => 2,
    }
}

fn archive_object_kind(value: i16) -> Result<ArchiveObjectKind, ApplicationError> {
    match value {
        0 => Ok(ArchiveObjectKind::BaseChunk),
        1 => Ok(ArchiveObjectKind::WalSegment),
        2 => Ok(ArchiveObjectKind::TimelineHistory),
        _ => Err(ApplicationError::Storage(
            "prepared archive object kind is invalid".to_owned(),
        )),
    }
}

fn validate_entry_and_receipt(
    entry: &SignedJournalEntry,
    receipt: &WitnessReceipt,
) -> Result<(), ApplicationError> {
    entry
        .verify()
        .map_err(|error| ApplicationError::Policy(error.to_string()))?;
    receipt
        .verify_against(receipt.witness_public_key())
        .map_err(|error| ApplicationError::Policy(error.to_string()))?;
    if receipt.installation_id() != entry.installation_id()
        || receipt.fingerprint_key_id() != entry.fingerprint_key_id()
        || receipt.continuity_proof() != entry.continuity_proof()
        || receipt.sequence() != entry.sequence()
        || receipt.journal_digest() != entry.digest()
        || receipt.generation_id() != entry.generation_id()
        || receipt.activation_epoch() != entry.activation_epoch()
        || receipt.state() != entry.state()
    {
        return Err(ApplicationError::Conflict(
            "witness receipt does not exactly bind the archive entry".to_owned(),
        ));
    }
    Ok(())
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn digest_from_bytes(
    bytes: Vec<u8>,
    label: &str,
) -> Result<vestrace_domain::SafetyJournalDigest, ApplicationError> {
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| ApplicationError::Storage(format!("{label} must contain exactly 32 bytes")))?;
    Ok(vestrace_domain::SafetyJournalDigest::from_bytes(bytes))
}
