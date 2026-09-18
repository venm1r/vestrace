//! SQLx adapter for the four P05-C guarded restore/cutover procedures.

use sqlx::Row;
use vestrace_application::{ApplicationError, RestoreAttemptAuthority, UnitOfWork};
use vestrace_domain::{
    ArchiveObjectDescriptor, ArchiveObjectDescriptorInput, ArchiveObjectKind, BackupObjectId,
    RestoreAttemptProgress, RestoreTerminalReceipt, SafetyEventKind, SafetyJournalDigest,
    SignedJournalEntry, SourceFreezePoint, WitnessReceipt,
};

use super::{PgStore, transaction::PgScopedTransaction};

#[derive(Clone, Debug)]
pub struct PgRestoreCutoverRepository {
    store: PgStore,
}

impl PgRestoreCutoverRepository {
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

    pub async fn prepare_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
    ) -> Result<(), ApplicationError> {
        if attempt.source_freeze().is_some() || attempt.terminal_receipt().is_some() {
            return Err(ApplicationError::Policy(
                "restore attempt preparation requires its exact prepared state".to_owned(),
            ));
        }
        let transaction = Self::transaction(unit_of_work)?;
        sqlx::query("SELECT public.vestrace_prepare_restore_attempt($1,$2,$3,$4,$5,$6)")
            .bind(attempt.attempt_id().as_uuid())
            .bind(attempt.backup_set_id().as_uuid())
            .bind(attempt.hold_id().as_uuid())
            .bind(attempt.target_id().as_uuid())
            .bind(attempt.source_generation_id().as_uuid())
            .bind(attempt.target_generation_id().as_uuid())
            .execute(transaction.connection())
            .await
            .map_err(storage_error)?;
        let _ = &self.store;
        Ok(())
    }

    pub async fn record_source_freeze_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
        freeze: SourceFreezePoint,
    ) -> Result<(), ApplicationError> {
        if attempt.source_freeze().is_some()
            || freeze.generation_id() != attempt.source_generation_id()
        {
            return Err(ApplicationError::Policy(
                "source freeze does not bind the prepared attempt".to_owned(),
            ));
        }
        let timeline = i32::try_from(freeze.timeline()).map_err(|_| {
            ApplicationError::Policy("freeze timeline exceeds PostgreSQL integer".to_owned())
        })?;
        let lsn = i64::try_from(freeze.lsn()).map_err(|_| {
            ApplicationError::Policy("freeze LSN exceeds PostgreSQL bigint".to_owned())
        })?;
        let watermark = i64::try_from(freeze.mutation_watermark()).map_err(|_| {
            ApplicationError::Policy("freeze watermark exceeds PostgreSQL bigint".to_owned())
        })?;
        sqlx::query("SELECT public.vestrace_record_source_freeze($1,$2,$3,$4)")
            .bind(attempt.attempt_id().as_uuid())
            .bind(timeline)
            .bind(lsn)
            .bind(watermark)
            .execute(Self::transaction(unit_of_work)?.connection())
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    pub async fn record_target_initialized_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
        receipt: SafetyJournalDigest,
    ) -> Result<(), ApplicationError> {
        if attempt.source_freeze().is_none() {
            return Err(ApplicationError::Policy(
                "target initialization requires a frozen restore attempt".to_owned(),
            ));
        }
        sqlx::query("SELECT public.vestrace_record_target_initialized($1,$2)")
            .bind(attempt.attempt_id().as_uuid())
            .bind(receipt.as_bytes().as_slice())
            .execute(Self::transaction(unit_of_work)?.connection())
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    pub async fn record_source_resume_prepared_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
        receipt: SafetyJournalDigest,
    ) -> Result<(), ApplicationError> {
        if attempt.source_freeze().is_none() || attempt.terminal_receipt().is_some() {
            return Err(ApplicationError::Policy(
                "source resume requires a nonterminal frozen restore attempt".to_owned(),
            ));
        }
        sqlx::query("SELECT public.vestrace_record_source_resume_prepared($1,$2)")
            .bind(attempt.attempt_id().as_uuid())
            .bind(receipt.as_bytes().as_slice())
            .execute(Self::transaction(unit_of_work)?.connection())
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    /// Mirrors each non-generation P05-C safety advance into PostgreSQL before
    /// a later generation CAS can rely on the exact witness predecessor.
    pub async fn record_restore_safety_event_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        entry: SignedJournalEntry,
        receipt: WitnessReceipt,
    ) -> Result<(), ApplicationError> {
        let event_kind = match entry.event_kind() {
            SafetyEventKind::TargetActivationPlanned => 11_i16,
            SafetyEventKind::RestoreTerminalRecorded => 12_i16,
            SafetyEventKind::RestoreAttemptPrepared => 13_i16,
            SafetyEventKind::SourceFreezeRecorded => 14_i16,
            SafetyEventKind::TargetActivating => 15_i16,
            _ => {
                return Err(ApplicationError::Policy(
                    "restore authority cannot mirror a non-P05-C safety event".to_owned(),
                ));
            }
        };
        sqlx::query(
            "SELECT public.vestrace_record_restore_safety_event(\
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(entry.installation_id().as_uuid())
        .bind(entry.fingerprint_key_id().as_uuid())
        .bind(entry.continuity_proof().as_bytes().as_slice())
        .bind(entry.request_id().as_uuid())
        .bind(receipt.sequence() as i64)
        .bind(receipt.journal_digest().as_bytes().as_slice())
        .bind(entry.signature().as_slice())
        .bind(event_kind)
        .bind(receipt.generation_id().as_uuid())
        .bind(receipt.activation_epoch() as i64)
        .bind(receipt.state().canonical_bytes())
        .bind(receipt.signature().as_slice())
        .execute(Self::transaction(unit_of_work)?.connection())
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    /// Fetches the exact restore manifest while the short supervisor permit
    /// is held. Callers must commit this read before touching host files.
    pub async fn list_restore_objects_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
    ) -> Result<Vec<ArchiveObjectDescriptor>, ApplicationError> {
        if attempt.source_freeze().is_none() || attempt.terminal_receipt().is_some() {
            return Err(ApplicationError::Policy(
                "restore manifest requires a nonterminal frozen attempt".to_owned(),
            ));
        }
        let rows = sqlx::query(
            "SELECT backup_set_id, object_id, ordinal, object_kind, timeline, start_lsn, end_lsn, object_digest, plaintext_digest, object_length, predecessor_head_digest \
             FROM public.vestrace_list_restore_archive_objects($1)",
        )
        .bind(attempt.attempt_id().as_uuid())
        .fetch_all(Self::transaction(unit_of_work)?.connection())
        .await
        .map_err(storage_error)?;
        rows.into_iter().map(restore_descriptor_from_row).collect()
    }

    pub async fn release_hold_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        terminal: &RestoreTerminalReceipt,
    ) -> Result<(), ApplicationError> {
        let receipt = terminal.release_reason().receipt();
        sqlx::query("SELECT public.vestrace_release_restore_hold($1,$2)")
            .bind(terminal.attempt_id().as_uuid())
            .bind(receipt.as_bytes().as_slice())
            .execute(Self::transaction(unit_of_work)?.connection())
            .await
            .map_err(storage_error)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl RestoreAttemptAuthority for PgRestoreCutoverRepository {
    async fn prepare_attempt_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
    ) -> Result<(), ApplicationError> {
        self.prepare_in(unit_of_work, attempt).await
    }

    async fn record_source_freeze_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
        freeze: SourceFreezePoint,
    ) -> Result<(), ApplicationError> {
        PgRestoreCutoverRepository::record_source_freeze_in(self, unit_of_work, attempt, freeze)
            .await
    }

    async fn record_target_initialized_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
        receipt: SafetyJournalDigest,
    ) -> Result<(), ApplicationError> {
        PgRestoreCutoverRepository::record_target_initialized_in(
            self,
            unit_of_work,
            attempt,
            receipt,
        )
        .await
    }

    async fn record_source_resume_prepared_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        attempt: &RestoreAttemptProgress,
        receipt: SafetyJournalDigest,
    ) -> Result<(), ApplicationError> {
        PgRestoreCutoverRepository::record_source_resume_prepared_in(
            self,
            unit_of_work,
            attempt,
            receipt,
        )
        .await
    }

    async fn record_restore_safety_event_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        entry: SignedJournalEntry,
        receipt: WitnessReceipt,
    ) -> Result<(), ApplicationError> {
        PgRestoreCutoverRepository::record_restore_safety_event_in(
            self,
            unit_of_work,
            entry,
            receipt,
        )
        .await
    }

    async fn release_hold_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        terminal: &RestoreTerminalReceipt,
    ) -> Result<(), ApplicationError> {
        PgRestoreCutoverRepository::release_hold_in(self, unit_of_work, terminal).await
    }
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

fn restore_descriptor_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<ArchiveObjectDescriptor, ApplicationError> {
    let kind: i16 = row.try_get("object_kind").map_err(storage_error)?;
    ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
        set_id: vestrace_domain::BackupSetId::from_uuid(
            row.try_get("backup_set_id").map_err(storage_error)?,
        ),
        object_id: BackupObjectId::from_uuid(row.try_get("object_id").map_err(storage_error)?),
        kind: match kind {
            0 => ArchiveObjectKind::BaseChunk,
            1 => ArchiveObjectKind::WalSegment,
            2 => ArchiveObjectKind::TimelineHistory,
            _ => {
                return Err(ApplicationError::Storage(
                    "restore archive object kind is invalid".to_owned(),
                ));
            }
        },
        ordinal: u64::try_from(row.try_get::<i64, _>("ordinal").map_err(storage_error)?).map_err(
            |_| ApplicationError::Storage("restore archive ordinal is negative".to_owned()),
        )?,
        timeline: u32::try_from(row.try_get::<i32, _>("timeline").map_err(storage_error)?)
            .map_err(|_| {
                ApplicationError::Storage("restore archive timeline is negative".to_owned())
            })?,
        start_lsn: u64::try_from(row.try_get::<i64, _>("start_lsn").map_err(storage_error)?)
            .map_err(|_| {
                ApplicationError::Storage("restore archive start LSN is negative".to_owned())
            })?,
        end_lsn: u64::try_from(row.try_get::<i64, _>("end_lsn").map_err(storage_error)?).map_err(
            |_| ApplicationError::Storage("restore archive end LSN is negative".to_owned()),
        )?,
        ciphertext_digest: digest(
            row.try_get("object_digest").map_err(storage_error)?,
            "restore archive ciphertext digest",
        )?,
        plaintext_digest: digest(
            row.try_get("plaintext_digest").map_err(storage_error)?,
            "restore archive plaintext digest",
        )?,
        length: u64::try_from(
            row.try_get::<i64, _>("object_length")
                .map_err(storage_error)?,
        )
        .map_err(|_| ApplicationError::Storage("restore archive length is negative".to_owned()))?,
        predecessor_head_digest: digest(
            row.try_get("predecessor_head_digest")
                .map_err(storage_error)?,
            "restore archive predecessor digest",
        )?,
    })
    .map_err(|error| ApplicationError::Storage(error.to_string()))
}

fn digest(bytes: Vec<u8>, label: &str) -> Result<SafetyJournalDigest, ApplicationError> {
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| ApplicationError::Storage(format!("{label} has an invalid length")))?;
    Ok(SafetyJournalDigest::from_bytes(bytes))
}
