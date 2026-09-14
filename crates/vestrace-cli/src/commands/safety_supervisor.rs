//! Host-only P05 safety supervisor. It deliberately has no listener and does
//! not load the ordinary runtime configuration or DSN.

use std::{
    env,
    ffi::OsString,
    fs::{self, File},
    io::{ErrorKind, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    thread,
};

use anyhow::anyhow;
use ring::signature::{Ed25519KeyPair, KeyPair};
use secrecy::SecretString;
use vestrace_application::{
    AcquireRestoreHold, AppendWal, ArchiveKeyCustody, ArchiveKeyEnvelopeRef,
    ArchiveLifecycleTransition, ArchiveObjectWrite, BackupArchiveController,
    BackupArchiveRepository, BackupObjectStore, CommitArchiveCheckpoint,
    InitializeInstallationSafety, InstallationMutationPermit, InstallationSafetyWitness as _,
    InstallationSupervisorContext, PermitMode, RegisterDatabaseGeneration, ReserveArchiveAppend,
    RestoreAttemptAuthorityService, RestoreCutoverController, SafetyAuthorityRepository,
    SafetyAuthorityService, StartManagedBackup,
};
use vestrace_domain::{
    ArchiveAppendReservation, ArchiveHead, ArchiveObjectDescriptor, ArchiveObjectDescriptorInput,
    ArchiveObjectKind, BackupObjectId, BackupSetId, BackupSetIdentity, DatabaseGenerationId,
    FingerprintKeyContinuityProof, FingerprintKeyId, InstallationId, JournalEntryToSign,
    JournalPublicKey, RequestId, RestoreAttemptId, RestoreAttemptProgress, RestoreHoldId,
    RestoreHoldReleaseReason, RestoreTargetId, SafetyBootstrapBinding, SafetyBootstrapRecord,
    SafetyEventKind, SafetyJournalDigest, SignedJournalEntry, SourceFreezePoint,
    TargetActivationPlan, WalArchiveCheckpoint,
};
use vestrace_infrastructure::{
    DatabaseConfig, PgBackupArchiveRepository, PgInstallationMutationPermit,
    PgRestoreCutoverRepository, PgSafetyAuthorityRepository, PgStore,
    backup_archive::{FileArchiveKeyCustody, FileBackupObjectStore},
    restore_target::{RestoreTargetCustody, RestoreTool},
    safety::{FileInstallationSafetyWitness, FileSafetyJournal},
};

pub async fn initialize(
    installation_id: uuid::Uuid,
    fingerprint_key_id: uuid::Uuid,
    continuity_proof_hex: &str,
    generation_id: uuid::Uuid,
    fault_after_witness_advance: bool,
) -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let journal_signer = load_signer(&roots.journal_signing_key)?;
    let binding = SafetyBootstrapBinding::new(
        InstallationId::from_uuid(installation_id),
        FingerprintKeyId::from_uuid(fingerprint_key_id),
        FingerprintKeyContinuityProof::from_bytes(parse_hex_32(continuity_proof_hex)?),
        JournalPublicKey::from_bytes(
            journal_signer
                .public_key()
                .as_ref()
                .try_into()
                .map_err(|_| anyhow!("journal signing key is not Ed25519"))?,
        ),
        witness_public_key(&roots.witness_root)?,
    );
    SafetyBootstrapRecord::open_or_create(&roots.bootstrap_root, &binding)
        .map_err(|error| anyhow!("bootstrap record is unavailable: {error}"))?;
    let generation = DatabaseGenerationId::from_uuid(generation_id);
    let witness = Arc::new(
        FileInstallationSafetyWitness::open_or_create(
            &roots.witness_root,
            binding.clone(),
            generation,
        )
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?,
    );
    let journal = Arc::new(
        FileSafetyJournal::open(&roots.journal_root)
            .map_err(|error| anyhow!("journal is unavailable: {error}"))?,
    );
    let store = supervisor_store().await?;
    let service = SafetyAuthorityService::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        journal,
        witness.clone(),
        Arc::new(PgSafetyAuthorityRepository::new(store)),
    );
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    if head.sequence() != 0 {
        return Err(anyhow!(
            "witness already advanced; use safety-supervisor reconcile for the exact durable receipt"
        ));
    }
    let entry = SignedJournalEntry::sign(
        &journal_signer,
        vestrace_domain::JournalEntryToSign {
            installation_id: binding.installation_id(),
            fingerprint_key_id: binding.fingerprint_key_id(),
            continuity_proof: binding.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: head
                .sequence()
                .checked_add(1)
                .ok_or_else(|| anyhow!("witness sequence overflow"))?,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::InstallationInitialized,
            generation_id: generation,
            activation_epoch: head.activation_epoch(),
            state: head.state().clone(),
        },
    );
    service
        .initialize_with_after_witness(
            &InstallationSupervisorContext::host_supervisor(),
            InitializeInstallationSafety::new(entry.request_id(), binding, generation),
            entry,
            move || {
                if fault_after_witness_advance {
                    std::process::exit(86);
                }
            },
        )
        .await
        .map_err(|error| anyhow!("guarded safety initialization failed: {error}"))?;
    println!("installation safety initialized");
    Ok(())
}

/// Starts one host-owned archive set. The initial head is derived from the
/// set identity; no operator-supplied key, object path, or ordinary runtime
/// credential participates in the transition.
pub async fn backup_begin() -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let journal_signer = load_signer(&roots.journal_signing_key)?;
    let witness = Arc::new(
        FileInstallationSafetyWitness::open(&roots.witness_root)
            .map_err(|error| anyhow!("witness is unavailable: {error}"))?,
    );
    let journal = Arc::new(
        FileSafetyJournal::open(&roots.journal_root)
            .map_err(|error| anyhow!("journal is unavailable: {error}"))?,
    );
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let set = BackupSetId::new();
    let initial_head = ArchiveHead::genesis(SafetyJournalDigest::of(
        &[
            b"vestrace-managed-backup-initial-head-v1".as_slice(),
            set.as_uuid().as_bytes(),
        ]
        .concat(),
    ));
    let identity = BackupSetIdentity::new(
        set,
        witness.binding().installation_id(),
        head.active_generation_id(),
    );
    let state = head.state().with_backup_archive_state(
        head.state()
            .backup_archive_state()
            .start_streaming(identity, initial_head)
            .map_err(|error| anyhow!("backup start state is invalid: {error}"))?,
    );
    let entry = SignedJournalEntry::sign(
        &journal_signer,
        JournalEntryToSign {
            installation_id: witness.binding().installation_id(),
            fingerprint_key_id: witness.binding().fingerprint_key_id(),
            continuity_proof: witness.binding().continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: head
                .sequence()
                .checked_add(1)
                .ok_or_else(|| anyhow!("witness sequence overflow"))?,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::BackupSetStarted,
            generation_id: head.active_generation_id(),
            activation_epoch: head.activation_epoch(),
            state,
        },
    );
    let store = supervisor_store().await?;
    let controller = BackupArchiveController::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        journal,
        witness,
        Arc::new(PgBackupArchiveRepository::new(store)),
        Arc::new(FileBackupObjectStore::open(roots.archive_root)?),
        Arc::new(FileArchiveKeyCustody::open(roots.archive_key_root)?),
    );
    controller
        .start(
            &InstallationSupervisorContext::host_supervisor(),
            StartManagedBackup::new(identity, initial_head, entry),
        )
        .await
        .map_err(|error| anyhow!("guarded managed-backup start failed: {error}"))?;
    println!("managed backup started: {}", set.as_uuid());
    Ok(())
}

/// Stages one supervisor-provided local base archive under an existing backup
/// set. The first successful member is the only accepted base checkpoint.
pub async fn backup_append_base(
    set_id: uuid::Uuid,
    segment: &std::path::Path,
    timeline: u32,
    start_lsn: u64,
    end_lsn: u64,
) -> anyhow::Result<()> {
    backup_append_object(
        set_id,
        segment,
        timeline,
        start_lsn,
        end_lsn,
        ArchiveObjectKind::BaseChunk,
        "base archive",
    )
    .await
}

/// Captures one PostgreSQL base backup under the protected archive root and
/// turns it into the required first `BaseChunk`. The source DSN is never a
/// shell fragment, journal field, or diagnostic value.
pub async fn backup_capture_base(set_id: uuid::Uuid) -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let binary = required_path("VESTRACE_PG_BASEBACKUP_BIN")?;
    require_pg_basebackup_binary(&binary)?;
    let source_dsn = env::var("VESTRACE_PG_BASEBACKUP_SOURCE_DSN")
        .map_err(|_| anyhow!("VESTRACE_PG_BASEBACKUP_SOURCE_DSN is required"))?;
    if source_dsn.is_empty() {
        return Err(anyhow!(
            "VESTRACE_PG_BASEBACKUP_SOURCE_DSN must not be empty"
        ));
    }
    let expected_system_identifier = required_source_system_identifier()?;

    let capture = roots
        .archive_root
        .join("base-capture")
        .join(set_id.to_string())
        .join(uuid::Uuid::now_v7().to_string());
    fs::create_dir_all(&capture)
        .map_err(|error| anyhow!("create protected base capture directory: {error}"))?;
    run_pg_basebackup(&binary, &source_dsn, &capture)?;
    let base = capture.join("base.tar");
    let manifest = capture.join("backup_manifest");
    require_base_capture_layout(&capture, &base, &manifest)?;
    let metadata = base_capture_metadata(&base, &manifest)?;
    if metadata.system_identifier != expected_system_identifier {
        return Err(anyhow!(
            "pg_basebackup source system identifier does not match the pinned source; protected capture retained for inspection"
        ));
    }
    let (timeline, start_lsn) = (metadata.timeline, metadata.start_lsn);
    backup_append_base(set_id, &base, timeline, start_lsn, start_lsn).await?;
    fs::remove_file(&base).map_err(|error| anyhow!("remove captured base tar: {error}"))?;
    fs::remove_file(&manifest)
        .map_err(|error| anyhow!("remove captured base manifest: {error}"))?;
    fs::remove_dir(&capture)
        .map_err(|error| anyhow!("remove empty protected base capture directory: {error}"))?;
    println!("managed backup base capture committed: {set_id}");
    Ok(())
}

/// Stages one supervisor-provided local WAL segment under an existing backup
/// set. Key bytes stay inside `FileArchiveKeyCustody`; PostgreSQL receives
/// only the immutable ciphertext descriptor and signed checkpoint state.
pub async fn backup_append_wal(
    set_id: uuid::Uuid,
    segment: &std::path::Path,
    timeline: u32,
    start_lsn: u64,
    end_lsn: u64,
) -> anyhow::Result<()> {
    backup_append_object(
        set_id,
        segment,
        timeline,
        start_lsn,
        end_lsn,
        ArchiveObjectKind::WalSegment,
        "WAL segment",
    )
    .await
}

async fn backup_append_object(
    set_id: uuid::Uuid,
    segment: &std::path::Path,
    timeline: u32,
    start_lsn: u64,
    end_lsn: u64,
    kind: ArchiveObjectKind,
    object_label: &str,
) -> anyhow::Result<()> {
    if !segment.is_absolute() {
        return Err(anyhow!(
            "backup {object_label} must be an absolute host path"
        ));
    }
    if timeline == 0 || end_lsn < start_lsn {
        return Err(anyhow!(
            "backup {object_label} timeline and LSN range are invalid"
        ));
    }
    let metadata = fs::metadata(segment)
        .map_err(|error| anyhow!("backup {object_label} is unavailable: {error}"))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(anyhow!(
            "backup {object_label} must be a nonempty regular file"
        ));
    }
    let roots = Roots::load()?;
    let journal_signer = load_signer(&roots.journal_signing_key)?;
    let witness = Arc::new(
        FileInstallationSafetyWitness::open(&roots.witness_root)
            .map_err(|error| anyhow!("witness is unavailable: {error}"))?,
    );
    let journal = Arc::new(
        FileSafetyJournal::open(&roots.journal_root)
            .map_err(|error| anyhow!("journal is unavailable: {error}"))?,
    );
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let set = BackupSetId::from_uuid(set_id);
    let archive = head.state().backup_archive_state();
    let expected_head = archive
        .archive_head(set)
        .map_err(|error| anyhow!("backup set is not available for {object_label}: {error}"))?;
    let ordinal = expected_head
        .checkpoint_ordinal
        .checked_add(1)
        .ok_or_else(|| anyhow!("backup archive ordinal overflow"))?;
    let object_id = BackupObjectId::new();
    let custody = Arc::new(
        FileArchiveKeyCustody::open(roots.archive_key_root.clone())
            .map_err(|error| anyhow!("archive key custody is unavailable: {error}"))?,
    );
    let (plaintext_digest, plaintext_length) = FileArchiveKeyCustody::digest_file(segment)
        .map_err(|error| anyhow!("backup {object_label} cannot be read: {error}"))?;
    if plaintext_length == 0 || plaintext_length != metadata.len() {
        return Err(anyhow!(
            "backup {object_label} changed while the supervisor was reading it"
        ));
    }
    let length = FileArchiveKeyCustody::sealed_stream_length_for_plaintext(plaintext_length)
        .map_err(|error| anyhow!("backup encryption frame length is invalid: {error}"))?;
    let descriptor_input = ArchiveObjectDescriptorInput {
        set_id: set,
        object_id,
        kind,
        ordinal,
        timeline,
        start_lsn,
        end_lsn,
        ciphertext_digest: SafetyJournalDigest::of(b"vestrace-unbound-archive-ciphertext-v1"),
        plaintext_digest,
        length,
        predecessor_head_digest: expected_head.digest,
    };
    let provisional = ArchiveObjectDescriptor::new(descriptor_input.clone())
        .map_err(|error| anyhow!("backup {object_label} descriptor is invalid: {error}"))?;
    let object_store = Arc::new(FileBackupObjectStore::open(roots.archive_root)?);
    let ciphertext_spool = object_store.ciphertext_spool_path(&provisional);
    let ciphertext_digest = custody
        .seal_file_to(set, &provisional, segment, &ciphertext_spool)
        .map_err(|error| anyhow!("backup {object_label} encryption failed: {error}"))?;
    let descriptor = ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
        ciphertext_digest,
        ..descriptor_input
    })
    .map_err(|error| anyhow!("backup {object_label} descriptor is invalid: {error}"))?;
    let reservation = ReserveArchiveAppend::new(set, expected_head, descriptor.clone())
        .map_err(|error| anyhow!("backup {object_label} reservation is invalid: {error}"))?;
    let domain_reservation = ArchiveAppendReservation::new(set, expected_head, descriptor.clone())
        .map_err(|error| anyhow!("backup {object_label} reservation is invalid: {error}"))?;
    let next_archive = archive
        .commit_checkpoint(
            domain_reservation.clone(),
            WalArchiveCheckpoint::from_reservation(&domain_reservation),
        )
        .map_err(|error| anyhow!("backup {object_label} checkpoint state is invalid: {error}"))?;
    let entry = SignedJournalEntry::sign(
        &journal_signer,
        JournalEntryToSign {
            installation_id: witness.binding().installation_id(),
            fingerprint_key_id: witness.binding().fingerprint_key_id(),
            continuity_proof: witness.binding().continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: head
                .sequence()
                .checked_add(1)
                .ok_or_else(|| anyhow!("witness sequence overflow"))?,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::ArchiveCheckpointCommitted,
            generation_id: head.active_generation_id(),
            activation_epoch: head.activation_epoch(),
            state: head.state().with_backup_archive_state(next_archive),
        },
    );
    let store = supervisor_store().await?;
    let controller = BackupArchiveController::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        journal,
        witness,
        Arc::new(PgBackupArchiveRepository::new(store)),
        object_store,
        custody,
    );
    controller
        .append_wal(
            &InstallationSupervisorContext::host_supervisor(),
            AppendWal::new(
                reservation,
                ArchiveObjectWrite::from_spool(descriptor, ciphertext_spool)
                    .map_err(|error| anyhow!("backup ciphertext is invalid: {error}"))?,
                entry,
            )
            .map_err(|error| anyhow!("backup {object_label} append command is invalid: {error}"))?,
        )
        .await
        .map_err(|error| anyhow!("guarded backup {object_label} append failed: {error}"))?;
    println!(
        "managed backup {object_label} checkpoint committed: {}",
        set.as_uuid()
    );
    Ok(())
}

/// Acquires the non-expiring restore hold P05-C must later release with a
/// terminal target/source receipt. This host command never accepts a release
/// reason and therefore cannot weaken that P05-C boundary.
pub async fn backup_acquire_hold(set_id: uuid::Uuid, hold_id: uuid::Uuid) -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let journal_signer = load_signer(&roots.journal_signing_key)?;
    let witness = Arc::new(
        FileInstallationSafetyWitness::open(&roots.witness_root)
            .map_err(|error| anyhow!("witness is unavailable: {error}"))?,
    );
    let journal = Arc::new(
        FileSafetyJournal::open(&roots.journal_root)
            .map_err(|error| anyhow!("journal is unavailable: {error}"))?,
    );
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let set = BackupSetId::from_uuid(set_id);
    let hold = RestoreHoldId::from_uuid(hold_id);
    let next_archive = head
        .state()
        .backup_archive_state()
        .acquire_restore_hold(set, hold)
        .map_err(|error| anyhow!("backup restore hold is invalid: {error}"))?;
    let entry = SignedJournalEntry::sign(
        &journal_signer,
        JournalEntryToSign {
            installation_id: witness.binding().installation_id(),
            fingerprint_key_id: witness.binding().fingerprint_key_id(),
            continuity_proof: witness.binding().continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: head
                .sequence()
                .checked_add(1)
                .ok_or_else(|| anyhow!("witness sequence overflow"))?,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::ArchiveRestoreHoldAcquired,
            generation_id: head.active_generation_id(),
            activation_epoch: head.activation_epoch(),
            state: head.state().with_backup_archive_state(next_archive),
        },
    );
    let store = supervisor_store().await?;
    let controller = BackupArchiveController::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        journal,
        witness,
        Arc::new(PgBackupArchiveRepository::new(store)),
        Arc::new(FileBackupObjectStore::open(roots.archive_root)?),
        Arc::new(FileArchiveKeyCustody::open(roots.archive_key_root)?),
    );
    controller
        .acquire_restore_hold(
            &InstallationSupervisorContext::host_supervisor(),
            vestrace_application::AcquireRestoreHold::new(set, hold),
            entry,
        )
        .await
        .map_err(|error| anyhow!("guarded backup restore-hold acquisition failed: {error}"))?;
    println!("managed backup restore hold acquired: {}", hold.as_uuid());
    Ok(())
}

/// Records one guarded restore attempt before creating a target directory.
/// The source root is deployment-owned and cannot be selected through a CLI
/// argument; the target is constrained by every authority root before it is
/// created.
pub async fn restore_prepare(
    attempt_id: uuid::Uuid,
    backup_set_id: uuid::Uuid,
    hold_id: uuid::Uuid,
    target_id: uuid::Uuid,
    target_generation_id: uuid::Uuid,
    target_root: &Path,
) -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let source_root = required_path("VESTRACE_RESTORE_SOURCE_ROOT")?;
    if !source_root.is_dir() {
        return Err(anyhow!(
            "VESTRACE_RESTORE_SOURCE_ROOT must name an existing source directory"
        ));
    }
    let signing_key_root = roots
        .journal_signing_key
        .parent()
        .ok_or_else(|| anyhow!("journal signing key must have a parent directory"))?
        .to_owned();
    let custody = Arc::new(
        RestoreTargetCustody::new([
            source_root.clone(),
            roots.archive_root.clone(),
            roots.archive_key_root.clone(),
            roots.journal_root.clone(),
            roots.witness_root.clone(),
            roots.bootstrap_root.clone(),
            signing_key_root,
        ])
        .map_err(|error| anyhow!("restore target custody is invalid: {error}"))?,
    );
    if !target_root.is_absolute() || target_root.exists() {
        return Err(anyhow!(
            "restore target must be an absolute, formerly absent directory"
        ));
    }
    let journal_signer = load_signer(&roots.journal_signing_key)?;
    let witness = Arc::new(
        FileInstallationSafetyWitness::open(&roots.witness_root)
            .map_err(|error| anyhow!("witness is unavailable: {error}"))?,
    );
    let journal = Arc::new(
        FileSafetyJournal::open(&roots.journal_root)
            .map_err(|error| anyhow!("journal is unavailable: {error}"))?,
    );
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let attempt = RestoreAttemptProgress::prepare(
        RestoreAttemptId::from_uuid(attempt_id),
        RestoreTargetId::from_uuid(target_id),
        BackupSetId::from_uuid(backup_set_id),
        RestoreHoldId::from_uuid(hold_id),
        head.active_generation_id(),
        DatabaseGenerationId::from_uuid(target_generation_id),
    )
    .map_err(|error| anyhow!("restore attempt is invalid: {error}"))?;
    match head.state().restore_attempt_progress() {
        Some(existing) if existing == &attempt => {}
        Some(_) => return Err(anyhow!("a different restore attempt is already active")),
        None => {
            let state = head
                .state()
                .with_restore_attempt_progress(attempt.clone())
                .map_err(|error| {
                    anyhow!("restore attempt cannot follow this safety state: {error}")
                })?;
            let entry = SignedJournalEntry::sign(
                &journal_signer,
                JournalEntryToSign {
                    installation_id: witness.binding().installation_id(),
                    fingerprint_key_id: witness.binding().fingerprint_key_id(),
                    continuity_proof: witness.binding().continuity_proof().clone(),
                    request_id: RequestId::new(),
                    sequence: head
                        .sequence()
                        .checked_add(1)
                        .ok_or_else(|| anyhow!("witness sequence overflow"))?,
                    previous_digest: head.chain_digest(),
                    event_kind: SafetyEventKind::RestoreAttemptPrepared,
                    generation_id: head.active_generation_id(),
                    activation_epoch: head.activation_epoch(),
                    state,
                },
            );
            let store = supervisor_store().await?;
            let service = RestoreAttemptAuthorityService::new(
                Arc::new(PgInstallationMutationPermit::new(store.clone())),
                journal,
                witness,
                Arc::new(PgRestoreCutoverRepository::new(store)),
            );
            service
                .prepare_attempt(
                    &InstallationSupervisorContext::host_supervisor(),
                    &attempt,
                    entry,
                )
                .await
                .map_err(|error| anyhow!("guarded restore preparation failed: {error}"))?;
        }
    }
    ensure_target_locator(&roots.journal_root, &attempt, target_root)?;
    RestoreCutoverController::new(custody)
        .restore_to_target(attempt, source_root, target_root.to_owned())
        .await
        .map_err(|error| anyhow!("restore target preparation failed: {error}"))?;
    println!("managed restore target prepared: {target_id}");
    Ok(())
}

/// Persists the source point only for the exact prepared attempt in the
/// witness. The caller cannot choose a source generation other than the one
/// the attempt already bound.
pub async fn restore_freeze_source() -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let journal_signer = load_signer(&roots.journal_signing_key)?;
    let witness = Arc::new(
        FileInstallationSafetyWitness::open(&roots.witness_root)
            .map_err(|error| anyhow!("witness is unavailable: {error}"))?,
    );
    let journal = Arc::new(
        FileSafetyJournal::open(&roots.journal_root)
            .map_err(|error| anyhow!("journal is unavailable: {error}"))?,
    );
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let attempt = head
        .state()
        .restore_attempt_progress()
        .cloned()
        .ok_or_else(|| anyhow!("source freeze requires a prepared restore attempt"))?;
    let source_root = required_path("VESTRACE_RESTORE_SOURCE_ROOT")?;
    if !source_root.is_dir() {
        return Err(anyhow!(
            "VESTRACE_RESTORE_SOURCE_ROOT must name an existing source directory"
        ));
    }
    if !source_freeze_receipt_path(&source_root).exists() {
        run_source_freeze_tool(&source_root, &attempt)?;
    }
    let freeze = read_source_freeze_receipt(&source_root, &attempt)?;
    let state = head
        .state()
        .with_source_freeze(freeze)
        .map_err(|error| anyhow!("source freeze cannot follow this safety state: {error}"))?;
    let entry = SignedJournalEntry::sign(
        &journal_signer,
        JournalEntryToSign {
            installation_id: witness.binding().installation_id(),
            fingerprint_key_id: witness.binding().fingerprint_key_id(),
            continuity_proof: witness.binding().continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: head
                .sequence()
                .checked_add(1)
                .ok_or_else(|| anyhow!("witness sequence overflow"))?,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::SourceFreezeRecorded,
            generation_id: head.active_generation_id(),
            activation_epoch: head.activation_epoch(),
            state,
        },
    );
    let store = supervisor_store().await?;
    let service = RestoreAttemptAuthorityService::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        journal,
        witness,
        Arc::new(PgRestoreCutoverRepository::new(store)),
    );
    service
        .record_source_freeze(
            &InstallationSupervisorContext::host_supervisor(),
            &attempt,
            freeze,
            entry,
        )
        .await
        .map_err(|error| anyhow!("guarded source freeze failed: {error}"))?;
    println!(
        "managed restore source freeze recorded: {}",
        attempt.attempt_id().as_uuid()
    );
    Ok(())
}

/// Runs the deployment-owned source quiescer only for a receipt that does not
/// already exist. A retry after a crash reads the same attempt-bound receipt;
/// it never invokes a second quiescer or accepts an operator-supplied point.
fn run_source_freeze_tool(
    source_root: &Path,
    attempt: &RestoreAttemptProgress,
) -> anyhow::Result<()> {
    let tool = required_path("VESTRACE_PG_SOURCE_FREEZE_BIN")?;
    run_source_freeze_tool_at(&tool, source_root, attempt)
}

fn run_source_freeze_tool_at(
    tool: &Path,
    source_root: &Path,
    attempt: &RestoreAttemptProgress,
) -> anyhow::Result<()> {
    if !tool.is_file() {
        return Err(anyhow!(
            "VESTRACE_PG_SOURCE_FREEZE_BIN must name an absolute existing file"
        ));
    }
    let receipt = source_freeze_receipt_path(source_root);
    let attempt_id = attempt.attempt_id().as_uuid().to_string();
    let generation_id = attempt.source_generation_id().as_uuid().to_string();
    let status = Command::new(tool)
        .args([
            "--source-root".as_ref(),
            source_root.as_os_str(),
            "--receipt".as_ref(),
            receipt.as_os_str(),
            "--attempt-id".as_ref(),
            attempt_id.as_ref(),
            "--source-generation-id".as_ref(),
            generation_id.as_ref(),
            "--quiesce-and-drain".as_ref(),
            "--checkpoint".as_ref(),
        ])
        .status()
        .map_err(|error| anyhow!("source quiescer could not start: {error}"))?;
    if !status.success() {
        return Err(anyhow!("source quiescer failed before its freeze receipt"));
    }
    if !receipt.is_file() {
        return Err(anyhow!("source quiescer did not create a freeze receipt"));
    }
    Ok(())
}

fn source_freeze_receipt_path(source_root: &Path) -> PathBuf {
    source_root.join(".vestrace-source-freeze-v1")
}

/// Reads the receipt only from the deployment-owned source root. The
/// supervisor deliberately accepts no LSN or watermark CLI arguments: the
/// source quiescer writes one completion-only receipt after it rejects new
/// work, drains the fixed pre-existing identities, and captures a checkpoint.
fn read_source_freeze_receipt(
    source_root: &Path,
    attempt: &RestoreAttemptProgress,
) -> anyhow::Result<SourceFreezePoint> {
    let receipt = source_freeze_receipt_path(source_root);
    let text = fs::read_to_string(&receipt)
        .map_err(|error| anyhow!("source freeze receipt is unavailable: {error}"))?;
    let mut receipt_attempt = None;
    let mut receipt_generation = None;
    let mut timeline = None;
    let mut lsn = None;
    let mut mutation_watermark = None;
    for line in text.lines() {
        let Some((name, value)) = line.split_once('=') else {
            return Err(anyhow!("source freeze receipt is malformed"));
        };
        match name {
            "attempt_id" if receipt_attempt.is_none() => {
                receipt_attempt = Some(
                    uuid::Uuid::parse_str(value)
                        .map_err(|_| anyhow!("source freeze receipt has invalid attempt ID"))?,
                )
            }
            "source_generation_id" if receipt_generation.is_none() => {
                receipt_generation = Some(
                    uuid::Uuid::parse_str(value)
                        .map_err(|_| anyhow!("source freeze receipt has invalid generation ID"))?,
                )
            }
            "timeline" if timeline.is_none() => {
                timeline = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| anyhow!("source freeze timeline is malformed"))?,
                )
            }
            "lsn" if lsn.is_none() => {
                lsn = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| anyhow!("source freeze LSN is malformed"))?,
                )
            }
            "mutation_watermark" if mutation_watermark.is_none() => {
                mutation_watermark = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| anyhow!("source freeze watermark is malformed"))?,
                )
            }
            _ => {
                return Err(anyhow!(
                    "source freeze receipt has an unknown or duplicate field"
                ));
            }
        }
    }
    if receipt_attempt != Some(attempt.attempt_id().as_uuid())
        || receipt_generation != Some(attempt.source_generation_id().as_uuid())
    {
        return Err(anyhow!(
            "source freeze receipt is not bound to the prepared restore attempt"
        ));
    }
    SourceFreezePoint::new(
        attempt.source_generation_id(),
        timeline.ok_or_else(|| anyhow!("source freeze receipt lacks timeline"))?,
        lsn.ok_or_else(|| anyhow!("source freeze receipt lacks LSN"))?,
        mutation_watermark.ok_or_else(|| anyhow!("source freeze receipt lacks watermark"))?,
    )
    .map_err(|error| anyhow!("source freeze receipt is invalid: {error}"))
}

#[cfg(test)]
mod source_freeze_receipt_tests {
    use super::*;

    fn prepared_attempt() -> RestoreAttemptProgress {
        RestoreAttemptProgress::prepare(
            RestoreAttemptId::new(),
            RestoreTargetId::new(),
            BackupSetId::new(),
            RestoreHoldId::new(),
            DatabaseGenerationId::new(),
            DatabaseGenerationId::new(),
        )
        .unwrap()
    }

    #[test]
    fn source_freeze_receipt_must_name_the_exact_prepared_attempt() {
        let root = std::env::temp_dir().join(format!(
            "vestrace-source-freeze-receipt-{}",
            uuid::Uuid::now_v7()
        ));
        fs::create_dir(&root).unwrap();
        let attempt = prepared_attempt();
        fs::write(
            root.join(".vestrace-source-freeze-v1"),
            format!(
                "attempt_id={}\nsource_generation_id={}\ntimeline=1\nlsn=256\nmutation_watermark=9\n",
                RestoreAttemptId::new().as_uuid(),
                attempt.source_generation_id().as_uuid(),
            ),
        )
        .unwrap();
        assert!(read_source_freeze_receipt(&root, &attempt).is_err());

        fs::write(
            root.join(".vestrace-source-freeze-v1"),
            format!(
                "attempt_id={}\nsource_generation_id={}\ntimeline=1\nlsn=256\nmutation_watermark=9\n",
                attempt.attempt_id().as_uuid(),
                attempt.source_generation_id().as_uuid(),
            ),
        )
        .unwrap();
        assert_eq!(
            read_source_freeze_receipt(&root, &attempt).unwrap().lsn(),
            256
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn source_quiescer_receipt_is_created_by_an_absolute_tool_with_bound_ids() {
        let root = std::env::temp_dir().join(format!(
            "vestrace-source-freeze-tool-{}",
            uuid::Uuid::now_v7()
        ));
        fs::create_dir(&root).unwrap();
        let tool = fake_source_quiescer(&root);
        let attempt = prepared_attempt();
        run_source_freeze_tool_at(&tool, &root, &attempt).unwrap();
        assert_eq!(
            read_source_freeze_receipt(&root, &attempt).unwrap().lsn(),
            256
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    fn fake_source_quiescer(root: &Path) -> PathBuf {
        let tool = root.join("source-quiescer.cmd");
        fs::write(
            &tool,
            concat!(
                "@echo off\r\n",
                "setlocal EnableExtensions DisableDelayedExpansion\r\n",
                "set receipt=\r\n",
                "set attempt=\r\n",
                "set generation=\r\n",
                ":next\r\n",
                "if \"%~1\"==\"\" goto done\r\n",
                "if \"%~1\"==\"--receipt\" (set \"receipt=%~2\" & shift)\r\n",
                "if \"%~1\"==\"--attempt-id\" (set \"attempt=%~2\" & shift)\r\n",
                "if \"%~1\"==\"--source-generation-id\" (set \"generation=%~2\" & shift)\r\n",
                "shift\r\n",
                "goto next\r\n",
                ":done\r\n",
                "(echo attempt_id=%attempt%&echo source_generation_id=%generation%&echo timeline=1&echo lsn=256&echo mutation_watermark=9)> \"%receipt%\"\r\n"
            ),
        )
        .unwrap();
        tool
    }

    #[cfg(not(windows))]
    fn fake_source_quiescer(root: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let tool = root.join("source-quiescer");
        fs::write(
            &tool,
            concat!(
                "#!/bin/sh\n",
                "while [ \"$#\" -gt 0 ]; do\n",
                "  case \"$1\" in\n",
                "    --receipt) receipt=\"$2\"; shift ;;\n",
                "    --attempt-id) attempt=\"$2\"; shift ;;\n",
                "    --source-generation-id) generation=\"$2\"; shift ;;\n",
                "  esac\n",
                "  shift\n",
                "done\n",
                "printf 'attempt_id=%s\\nsource_generation_id=%s\\ntimeline=1\\nlsn=256\\nmutation_watermark=9\\n' \"$attempt\" \"$generation\" > \"$receipt\"\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
        tool
    }
}

/// Materializes only the manifest guarded by the exact frozen attempt. The
/// short database permit ends before any archive key or target filesystem is
/// opened, and the physical tool receives no active-source path.
pub async fn restore_materialize(target_root: &Path) -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let source_root = required_path("VESTRACE_RESTORE_SOURCE_ROOT")?;
    if !source_root.is_dir() {
        return Err(anyhow!(
            "VESTRACE_RESTORE_SOURCE_ROOT must name an existing source directory"
        ));
    }
    let signing_key_root = roots
        .journal_signing_key
        .parent()
        .ok_or_else(|| anyhow!("journal signing key must have a parent directory"))?
        .to_owned();
    let custody = RestoreTargetCustody::new([
        source_root,
        roots.archive_root.clone(),
        roots.archive_key_root.clone(),
        roots.journal_root.clone(),
        roots.witness_root.clone(),
        roots.bootstrap_root.clone(),
        signing_key_root,
    ])
    .map_err(|error| anyhow!("restore target custody is invalid: {error}"))?;
    let witness = FileInstallationSafetyWitness::open(&roots.witness_root)
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let head = vestrace_application::InstallationSafetyWitness::read_head(&witness)
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let attempt = head
        .state()
        .restore_attempt_progress()
        .cloned()
        .filter(|attempt| attempt.source_freeze().is_some() && attempt.terminal_receipt().is_none())
        .ok_or_else(|| anyhow!("restore materialization requires a nonterminal frozen attempt"))?;
    verify_target_locator(&roots.journal_root, &attempt, target_root)?;
    let plan = head
        .state()
        .target_activation_plan()
        .filter(|plan| {
            plan.attempt_id() == attempt.attempt_id()
                && plan.target_id() == attempt.target_id()
                && plan.backup_set_id() == attempt.backup_set_id()
                && plan.source_freeze() == attempt.source_freeze().expect("frozen above")
        })
        .ok_or_else(|| anyhow!("restore materialization requires its pinned activation plan"))?;
    if recover_target_materialization_receipt(target_root, plan)? {
        println!(
            "managed restore target materialization recovered: {}",
            attempt.attempt_id().as_uuid()
        );
        return Ok(());
    }
    let store = supervisor_store().await?;
    let permit = PgInstallationMutationPermit::new(store.clone());
    let context = InstallationSupervisorContext::host_supervisor();
    let mut handle = permit
        .acquire(PermitMode::Exclusive, context.request_context())
        .await
        .map_err(|error| anyhow!("restore manifest permit is unavailable: {error}"))?;
    let manifest = PgRestoreCutoverRepository::new(store)
        .list_restore_objects_in(handle.unit_of_work_mut(), &attempt)
        .await
        .map_err(|error| anyhow!("guarded restore manifest is unavailable: {error}"))?;
    handle
        .commit()
        .await
        .map_err(|error| anyhow!("restore manifest permit commit failed: {error}"))?;
    let archive = FileBackupObjectStore::open(roots.archive_root)
        .map_err(|error| anyhow!("archive object custody is unavailable: {error}"))?;
    let keys = FileArchiveKeyCustody::open(roots.archive_key_root)
        .map_err(|error| anyhow!("archive key custody is unavailable: {error}"))?;
    custody
        .materialize_manifest(
            target_root,
            attempt.backup_set_id(),
            &manifest,
            &archive,
            &keys,
        )
        .map_err(|error| anyhow!("restore materialization failed: {error}"))?;
    let tool = RestoreTool::new(required_path("VESTRACE_PG_RESTORE_BIN")?)
        .map_err(|error| anyhow!("restore tool is unavailable: {error}"))?;
    let tool_arguments = restore_tool_arguments(target_root, plan);
    custody
        .run_tool(&tool, target_root, tool_arguments)
        .map_err(|error| anyhow!("target-only PostgreSQL restore failed: {error}"))?;
    restore_fault_point("before-target-initialization");
    read_target_initialization_receipt(target_root, plan)?;
    println!(
        "managed restore target materialized: {}",
        attempt.attempt_id().as_uuid()
    );
    Ok(())
}

/// Supplies the tool only with the target-side inputs needed to produce the
/// receipt which the supervisor verifies before a target can be initialized.
/// The active source root is deliberately not an input to this contract.
fn restore_tool_arguments(target_root: &Path, plan: &TargetActivationPlan) -> Vec<OsString> {
    let freeze = plan.source_freeze();
    vec![
        target_root.join("restore-input").into_os_string(),
        OsString::from("--target-receipt"),
        target_receipt_path(target_root).into_os_string(),
        OsString::from("--plan-digest"),
        OsString::from(encode_digest(plan.digest())),
        OsString::from("--freeze-timeline"),
        OsString::from(freeze.timeline().to_string()),
        OsString::from("--freeze-lsn"),
        OsString::from(freeze.lsn().to_string()),
    ]
}

pub async fn restore_plan_activation() -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let signer = load_signer(&roots.journal_signing_key)?;
    let witness = Arc::new(
        FileInstallationSafetyWitness::open(&roots.witness_root)
            .map_err(|e| anyhow!("witness is unavailable: {e}"))?,
    );
    let journal = Arc::new(
        FileSafetyJournal::open(&roots.journal_root)
            .map_err(|e| anyhow!("journal is unavailable: {e}"))?,
    );
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
        .await
        .map_err(|e| anyhow!("witness is unavailable: {e}"))?;
    let attempt = head
        .state()
        .restore_attempt_progress()
        .cloned()
        .filter(|a| a.source_freeze().is_some() && a.terminal_receipt().is_none())
        .ok_or_else(|| anyhow!("activation planning requires a nonterminal frozen attempt"))?;
    let archive_head = head
        .state()
        .backup_archive_state()
        .archive_head(attempt.backup_set_id())
        .map_err(|e| anyhow!("activation plan archive head is unavailable: {e}"))?;
    let plan = TargetActivationPlan::new(
        attempt.attempt_id(),
        attempt.target_id(),
        attempt.backup_set_id(),
        attempt.source_generation_id(),
        attempt.target_generation_id(),
        attempt.source_freeze().expect("frozen above"),
        archive_head.digest,
    )
    .map_err(|e| anyhow!("activation plan is invalid: {e}"))?;
    if let Some(existing) = head.state().target_activation_plan() {
        if existing == &plan {
            return Ok(());
        }
        return Err(anyhow!("a different activation plan is already pinned"));
    }
    let state = head
        .state()
        .with_target_activation_plan(plan.clone())
        .map_err(|e| anyhow!("activation plan cannot follow safety state: {e}"))?;
    let entry = sign_restore_entry(
        &signer,
        witness.as_ref(),
        &head,
        SafetyEventKind::TargetActivationPlanned,
        head.active_generation_id(),
        head.activation_epoch(),
        state,
    )?;
    let store = supervisor_store().await?;
    RestoreAttemptAuthorityService::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        journal,
        witness,
        Arc::new(PgRestoreCutoverRepository::new(store)),
    )
    .record_activation_plan(
        &InstallationSupervisorContext::host_supervisor(),
        &plan,
        entry,
    )
    .await
    .map_err(|e| anyhow!("guarded activation planning failed: {e}"))?;
    println!("managed restore activation plan pinned: {}", plan.digest());
    Ok(())
}

pub async fn restore_activate(target_root: &Path) -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let signer = load_signer(&roots.journal_signing_key)?;
    let witness = Arc::new(
        FileInstallationSafetyWitness::open(&roots.witness_root)
            .map_err(|e| anyhow!("witness is unavailable: {e}"))?,
    );
    let journal = Arc::new(
        FileSafetyJournal::open(&roots.journal_root)
            .map_err(|e| anyhow!("journal is unavailable: {e}"))?,
    );
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
        .await
        .map_err(|e| anyhow!("witness is unavailable: {e}"))?;
    let plan = head
        .state()
        .target_activation_plan()
        .cloned()
        .ok_or_else(|| anyhow!("activation requires a pinned plan"))?;
    let attempt = head
        .state()
        .restore_attempt_progress()
        .cloned()
        .ok_or_else(|| anyhow!("activation requires a restore attempt"))?;
    verify_target_locator(&roots.journal_root, &attempt, target_root)?;
    let marker = read_target_initialization_receipt(target_root, &plan)?;
    let terminal = vestrace_domain::RestoreTerminalReceipt::new(
        attempt.attempt_id(),
        attempt.target_id(),
        RestoreHoldReleaseReason::TargetInitializationComplete { receipt: marker },
    );
    let store = supervisor_store().await?;
    let service = RestoreAttemptAuthorityService::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        journal.clone(),
        witness.clone(),
        Arc::new(PgRestoreCutoverRepository::new(store.clone())),
    );
    if attempt.terminal_receipt().is_none() {
        let state = head
            .state()
            .with_restore_terminal_receipt(terminal.clone())
            .map_err(|e| anyhow!("target terminal receipt cannot follow safety state: {e}"))?;
        let entry = sign_restore_entry(
            &signer,
            witness.as_ref(),
            &head,
            SafetyEventKind::RestoreTerminalRecorded,
            head.active_generation_id(),
            head.activation_epoch(),
            state,
        )?;
        service
            .record_target_initialized(
                &InstallationSupervisorContext::host_supervisor(),
                &attempt,
                &terminal,
                entry,
            )
            .await
            .map_err(|e| anyhow!("guarded target initialization failed: {e}"))?;
    } else if attempt.terminal_receipt() != Some(&terminal) {
        return Err(anyhow!(
            "target terminal receipt differs from the pinned target"
        ));
    } else {
        service
            .ensure_target_initialized(
                &InstallationSupervisorContext::host_supervisor(),
                &attempt,
                &terminal,
            )
            .await
            .map_err(|e| anyhow!("guarded target initialization replay failed: {e}"))?;
    }
    restore_fault_point("after-target-initialized");
    let activated_head =
        vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
            .await
            .map_err(|e| anyhow!("witness is unavailable: {e}"))?;
    let activation_head = if activated_head.active_generation_id() == plan.source_generation_id() {
        match activated_head.state().target_activation_started() {
            None => {
                let state = activated_head
                    .state()
                    .with_target_activation_started()
                    .map_err(|e| anyhow!("target activation start is invalid: {e}"))?;
                let entry = sign_restore_entry(
                    &signer,
                    witness.as_ref(),
                    &activated_head,
                    SafetyEventKind::TargetActivating,
                    plan.source_generation_id(),
                    activated_head.activation_epoch(),
                    state,
                )?;
                service
                    .record_activation_started(
                        &InstallationSupervisorContext::host_supervisor(),
                        &plan,
                        entry,
                    )
                    .await
                    .map_err(|e| anyhow!("guarded target activation start failed: {e}"))?;
                restore_fault_point("after-target-activating");
                vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
                    .await
                    .map_err(|e| anyhow!("witness is unavailable: {e}"))?
            }
            Some(digest) if digest == plan.digest() => activated_head,
            Some(_) => return Err(anyhow!("activation start differs from the pinned plan")),
        }
    } else {
        activated_head
    };
    if activation_head.active_generation_id() == plan.source_generation_id() {
        let state = activation_head
            .state()
            .register_generation(plan.target_generation_id())
            .map_err(|e| anyhow!("target activation state is invalid: {e}"))?;
        let entry = sign_restore_entry(
            &signer,
            witness.as_ref(),
            &activation_head,
            SafetyEventKind::GenerationRegistered,
            plan.target_generation_id(),
            activation_head
                .activation_epoch()
                .checked_add(1)
                .ok_or_else(|| anyhow!("activation epoch overflow"))?,
            state,
        )?;
        SafetyAuthorityService::new(
            Arc::new(PgInstallationMutationPermit::new(store.clone())),
            journal,
            witness,
            Arc::new(PgSafetyAuthorityRepository::new(store.clone())),
        )
        .register_generation(
            &InstallationSupervisorContext::host_supervisor(),
            RegisterDatabaseGeneration::new(
                entry.request_id(),
                plan.target_generation_id(),
                entry.activation_epoch(),
            ),
            entry,
        )
        .await
        .map_err(|e| anyhow!("guarded target activation failed: {e}"))?;
    } else if activation_head.active_generation_id() != plan.target_generation_id() {
        return Err(anyhow!("active generation differs from the pinned target"));
    }
    service
        .release_hold(&InstallationSupervisorContext::host_supervisor(), &terminal)
        .await
        .map_err(|e| anyhow!("guarded restore-hold release failed: {e}"))?;
    println!(
        "managed restore target activated: {}",
        plan.target_generation_id().as_uuid()
    );
    Ok(())
}

fn restore_fault_point(point: &str) {
    if env::var("VESTRACE_P05_RESTORE_FAULT_POINT").ok().as_deref() == Some(point) {
        std::process::exit(87);
    }
}

/// Refuses a failed target only after its prepared locator identifies the
/// exact path. The source-resume tool must bind its receipt to the resulting
/// destruction marker before the guarded restore hold can be released.
pub async fn restore_refuse(target_root: &Path) -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let signer = load_signer(&roots.journal_signing_key)?;
    let witness = Arc::new(
        FileInstallationSafetyWitness::open(&roots.witness_root)
            .map_err(|error| anyhow!("witness is unavailable: {error}"))?,
    );
    let journal = Arc::new(
        FileSafetyJournal::open(&roots.journal_root)
            .map_err(|error| anyhow!("journal is unavailable: {error}"))?,
    );
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let attempt = head
        .state()
        .restore_attempt_progress()
        .cloned()
        .filter(|attempt| attempt.source_freeze().is_some() && attempt.terminal_receipt().is_none())
        .ok_or_else(|| anyhow!("target refusal requires a nonterminal frozen restore attempt"))?;
    verify_target_locator(&roots.journal_root, &attempt, target_root)?;
    let source_root = required_path("VESTRACE_RESTORE_SOURCE_ROOT")?;
    if !source_root.is_dir() {
        return Err(anyhow!(
            "VESTRACE_RESTORE_SOURCE_ROOT must name an existing source directory"
        ));
    }
    let signing_key_root = roots
        .journal_signing_key
        .parent()
        .ok_or_else(|| anyhow!("journal signing key must have a parent directory"))?
        .to_owned();
    let custody = RestoreTargetCustody::new([
        source_root.clone(),
        roots.archive_root.clone(),
        roots.archive_key_root.clone(),
        roots.journal_root.clone(),
        roots.witness_root.clone(),
        roots.bootstrap_root.clone(),
        signing_key_root,
    ])
    .map_err(|error| anyhow!("restore target custody is invalid: {error}"))?;
    let destruction =
        destroy_target_with_marker(&custody, &roots.journal_root, &attempt, target_root)?;
    let resume_receipt = run_and_read_source_resume(&source_root, &attempt, destruction)?;
    let terminal = vestrace_domain::RestoreTerminalReceipt::new(
        attempt.attempt_id(),
        attempt.target_id(),
        RestoreHoldReleaseReason::SourceResumePrepared {
            receipt: resume_receipt,
        },
    );
    let state = head
        .state()
        .with_restore_terminal_receipt(terminal.clone())
        .map_err(|error| anyhow!("source resume cannot follow this safety state: {error}"))?;
    let entry = sign_restore_entry(
        &signer,
        witness.as_ref(),
        &head,
        SafetyEventKind::RestoreTerminalRecorded,
        head.active_generation_id(),
        head.activation_epoch(),
        state,
    )?;
    let store = supervisor_store().await?;
    let service = RestoreAttemptAuthorityService::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        journal,
        witness,
        Arc::new(PgRestoreCutoverRepository::new(store)),
    );
    service
        .record_source_resume_prepared(
            &InstallationSupervisorContext::host_supervisor(),
            &attempt,
            &terminal,
            entry,
        )
        .await
        .map_err(|error| anyhow!("guarded source resume failed: {error}"))?;
    service
        .release_hold(&InstallationSupervisorContext::host_supervisor(), &terminal)
        .await
        .map_err(|error| anyhow!("guarded restore-hold release failed: {error}"))?;
    println!("managed restore target refused and source resume prepared");
    Ok(())
}

fn sign_restore_entry(
    signer: &Ed25519KeyPair,
    witness: &FileInstallationSafetyWitness,
    head: &vestrace_domain::WitnessHead,
    event_kind: SafetyEventKind,
    generation_id: DatabaseGenerationId,
    activation_epoch: u64,
    state: vestrace_domain::WitnessStateV1,
) -> anyhow::Result<SignedJournalEntry> {
    Ok(SignedJournalEntry::sign(
        signer,
        JournalEntryToSign {
            installation_id: witness.binding().installation_id(),
            fingerprint_key_id: witness.binding().fingerprint_key_id(),
            continuity_proof: witness.binding().continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: head
                .sequence()
                .checked_add(1)
                .ok_or_else(|| anyhow!("witness sequence overflow"))?,
            previous_digest: head.chain_digest(),
            event_kind,
            generation_id,
            activation_epoch,
            state,
        },
    ))
}

fn target_receipt_path(target: &Path) -> PathBuf {
    target.join(".vestrace-target-initialized-v1")
}

fn encode_digest(digest: SafetyJournalDigest) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut encoded = String::with_capacity(64);
    for byte in digest.as_bytes() {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}

/// Binds a host target path to the prepared attempt without placing the path
/// in the signed witness payload. Later host operations must present the same
/// absolute path; a retry can only reopen the exact create-only locator.
fn ensure_target_locator(
    journal_root: &Path,
    attempt: &RestoreAttemptProgress,
    target_root: &Path,
) -> anyhow::Result<()> {
    let expected = target_locator_contents(attempt, target_root)?;
    let locator = target_locator_path(journal_root, attempt);
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&locator)
    {
        Ok(mut file) => {
            use std::io::Write as _;

            file.write_all(expected.as_bytes())
                .map_err(|error| anyhow!("write restore target locator: {error}"))?;
            file.sync_all()
                .map_err(|error| anyhow!("sync restore target locator: {error}"))?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_target_locator(journal_root, attempt, target_root)
        }
        Err(error) => Err(anyhow!("create restore target locator: {error}")),
    }
}

fn verify_target_locator(
    journal_root: &Path,
    attempt: &RestoreAttemptProgress,
    target_root: &Path,
) -> anyhow::Result<()> {
    let expected = target_locator_contents(attempt, target_root)?;
    let actual = fs::read_to_string(target_locator_path(journal_root, attempt))
        .map_err(|error| anyhow!("restore target locator is unavailable: {error}"))?;
    if actual != expected {
        return Err(anyhow!(
            "restore target root differs from the path prepared for this attempt"
        ));
    }
    Ok(())
}

fn target_locator_path(journal_root: &Path, attempt: &RestoreAttemptProgress) -> PathBuf {
    journal_root.join(format!(
        ".vestrace-restore-target-{}-v1",
        attempt.attempt_id().as_uuid()
    ))
}

fn target_locator_contents(
    attempt: &RestoreAttemptProgress,
    target_root: &Path,
) -> anyhow::Result<String> {
    if !target_root.is_absolute() {
        return Err(anyhow!(
            "restore target locator requires an absolute target root"
        ));
    }
    let target = target_root
        .to_str()
        .ok_or_else(|| anyhow!("restore target root must be valid UTF-8"))?;
    let digest = SafetyJournalDigest::of(
        &[
            b"vestrace-restore-target-locator-v1".as_slice(),
            attempt.attempt_id().as_uuid().as_bytes(),
            target.as_bytes(),
        ]
        .concat(),
    );
    Ok(format!("{}\n", encode_digest(digest)))
}

fn destroy_target_with_marker(
    custody: &RestoreTargetCustody,
    journal_root: &Path,
    attempt: &RestoreAttemptProgress,
    target_root: &Path,
) -> anyhow::Result<SafetyJournalDigest> {
    let marker = journal_root.join(format!(
        ".vestrace-refused-target-{}-v1",
        attempt.attempt_id().as_uuid()
    ));
    let expected = SafetyJournalDigest::of(
        &[
            b"vestrace-refused-target-destruction-v1".as_slice(),
            target_locator_contents(attempt, target_root)?.as_bytes(),
        ]
        .concat(),
    );
    let contents = format!("{}\n", encode_digest(expected));
    if marker.exists() {
        if fs::read_to_string(&marker)
            .map_err(|error| anyhow!("read target destruction marker: {error}"))?
            == contents
        {
            return Ok(expected);
        }
        return Err(anyhow!(
            "target destruction marker differs from this restore attempt"
        ));
    }
    custody
        .destroy_refused_target(target_root)
        .map_err(|error| anyhow!("refused target destruction failed: {error}"))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker)
        .map_err(|error| anyhow!("create target destruction marker: {error}"))?;
    use std::io::Write as _;
    file.write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| anyhow!("persist target destruction marker: {error}"))?;
    Ok(expected)
}

fn run_and_read_source_resume(
    source_root: &Path,
    attempt: &RestoreAttemptProgress,
    destruction: SafetyJournalDigest,
) -> anyhow::Result<SafetyJournalDigest> {
    let receipt = source_root.join(".vestrace-source-resume-v1");
    if !receipt.exists() {
        let tool = required_path("VESTRACE_PG_SOURCE_RESUME_BIN")?;
        if !tool.is_file() {
            return Err(anyhow!(
                "VESTRACE_PG_SOURCE_RESUME_BIN must name an absolute existing file"
            ));
        }
        let attempt_id = attempt.attempt_id().as_uuid().to_string();
        let destruction = encode_digest(destruction);
        let status = Command::new(tool)
            .args([
                "--source-root".as_ref(),
                source_root.as_os_str(),
                "--receipt".as_ref(),
                receipt.as_os_str(),
                "--attempt-id".as_ref(),
                attempt_id.as_ref(),
                "--target-destruction-receipt".as_ref(),
                destruction.as_ref(),
                "--resume".as_ref(),
            ])
            .status()
            .map_err(|error| anyhow!("source resume tool could not start: {error}"))?;
        if !status.success() {
            return Err(anyhow!("source resume tool failed before its receipt"));
        }
    }
    let bytes = fs::read(&receipt)
        .map_err(|error| anyhow!("source resume receipt is unavailable: {error}"))?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| anyhow!("source resume receipt is malformed"))?;
    let expected_attempt = format!("attempt_id={}", attempt.attempt_id().as_uuid());
    let expected_destruction = format!("target_destruction_receipt={}", encode_digest(destruction));
    if text.lines().collect::<Vec<_>>() != [expected_attempt, expected_destruction] {
        return Err(anyhow!(
            "source resume receipt does not bind this refused target"
        ));
    }
    Ok(SafetyJournalDigest::of(&bytes))
}

#[cfg(test)]
mod refusal_receipt_tests {
    use super::*;

    fn attempt() -> RestoreAttemptProgress {
        RestoreAttemptProgress::prepare(
            RestoreAttemptId::new(),
            RestoreTargetId::new(),
            BackupSetId::new(),
            RestoreHoldId::new(),
            DatabaseGenerationId::new(),
            DatabaseGenerationId::new(),
        )
        .unwrap()
    }

    #[test]
    fn refusal_marker_is_idempotent_and_resume_receipt_binds_it() {
        let root = std::env::temp_dir().join(format!("vestrace-refusal-{}", uuid::Uuid::now_v7()));
        let journal = root.join("journal");
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&journal).unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(target.join("incomplete")).unwrap();
        let attempt = attempt();
        ensure_target_locator(&journal, &attempt, &target).unwrap();
        let custody = RestoreTargetCustody::new([source.clone(), journal.clone()]).unwrap();

        let destruction =
            destroy_target_with_marker(&custody, &journal, &attempt, &target).unwrap();
        assert!(!target.exists());
        assert_eq!(
            destroy_target_with_marker(&custody, &journal, &attempt, &target).unwrap(),
            destruction
        );
        let receipt = source.join(".vestrace-source-resume-v1");
        fs::write(
            &receipt,
            format!(
                "attempt_id={}\ntarget_destruction_receipt={}\n",
                attempt.attempt_id().as_uuid(),
                encode_digest(destruction)
            ),
        )
        .unwrap();
        assert!(run_and_read_source_resume(&source, &attempt, destruction).is_ok());
        fs::write(
            &receipt,
            "attempt_id=wrong\ntarget_destruction_receipt=wrong\n",
        )
        .unwrap();
        assert!(run_and_read_source_resume(&source, &attempt, destruction).is_err());
        let _ = fs::remove_dir_all(root);
    }
}

fn read_target_initialization_receipt(
    target: &Path,
    plan: &TargetActivationPlan,
) -> anyhow::Result<SafetyJournalDigest> {
    let bytes = fs::read(target_receipt_path(target))
        .map_err(|e| anyhow!("target initialization receipt is unavailable: {e}"))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| anyhow!("target initialization receipt is malformed"))?;
    let mut digest = None;
    let mut timeline = None;
    let mut final_lsn = None;
    for line in text.lines() {
        let Some((name, value)) = line.split_once('=') else {
            return Err(anyhow!("target initialization receipt is malformed"));
        };
        match name {
            "plan_digest" if digest.is_none() => {
                digest = Some(SafetyJournalDigest::from_bytes(parse_hex_32(value)?));
            }
            "timeline" if timeline.is_none() => {
                timeline = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| anyhow!("target receipt timeline is malformed"))?,
                );
            }
            "final_lsn" if final_lsn.is_none() => {
                final_lsn = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| anyhow!("target receipt final LSN is malformed"))?,
                );
            }
            _ => {
                return Err(anyhow!(
                    "target initialization receipt has an unknown or duplicate field"
                ));
            }
        }
    }
    let freeze = plan.source_freeze();
    if digest != Some(plan.digest())
        || timeline != Some(freeze.timeline())
        || final_lsn.is_none_or(|lsn| lsn < freeze.lsn())
    {
        return Err(anyhow!(
            "target initialization receipt does not reach the pinned source freeze"
        ));
    }
    Ok(SafetyJournalDigest::of(&bytes))
}

/// A receipt written by the target tool is the durable boundary between a
/// completed physical restore and recording its witnessed terminal effect.
/// A retry after a crash in that interval must validate that receipt, not try
/// to materialize over the target again.
fn recover_target_materialization_receipt(
    target: &Path,
    plan: &TargetActivationPlan,
) -> anyhow::Result<bool> {
    match fs::metadata(target_receipt_path(target)) {
        Ok(metadata) if metadata.is_file() => {
            read_target_initialization_receipt(target, plan)?;
            Ok(true)
        }
        Ok(_) => Err(anyhow!(
            "target initialization receipt is not a regular file"
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(anyhow!(
            "target initialization receipt is unavailable: {error}"
        )),
    }
}

#[cfg(test)]
mod target_initialization_receipt_tests {
    use super::*;

    fn plan() -> TargetActivationPlan {
        let source_generation = DatabaseGenerationId::new();
        TargetActivationPlan::new(
            RestoreAttemptId::new(),
            RestoreTargetId::new(),
            BackupSetId::new(),
            source_generation,
            DatabaseGenerationId::new(),
            SourceFreezePoint::new(source_generation, 1, 256, 9).unwrap(),
            SafetyJournalDigest::of(b"archive-head"),
        )
        .unwrap()
    }

    fn hex(digest: SafetyJournalDigest) -> String {
        use std::fmt::Write;

        digest
            .as_bytes()
            .iter()
            .fold(String::with_capacity(64), |mut encoded, byte| {
                write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
                encoded
            })
    }

    #[test]
    fn target_tool_receipt_must_reach_the_pinned_freeze_point() {
        let root = std::env::temp_dir().join(format!(
            "vestrace-target-initialization-receipt-{}",
            uuid::Uuid::now_v7()
        ));
        fs::create_dir(&root).unwrap();
        let plan = plan();
        fs::write(
            target_receipt_path(&root),
            format!(
                "plan_digest={}\ntimeline=1\nfinal_lsn=255\n",
                hex(plan.digest())
            ),
        )
        .unwrap();
        assert!(read_target_initialization_receipt(&root, &plan).is_err());

        fs::write(
            target_receipt_path(&root),
            format!(
                "plan_digest={}\ntimeline=1\nfinal_lsn=256\n",
                hex(plan.digest())
            ),
        )
        .unwrap();
        assert!(read_target_initialization_receipt(&root, &plan).is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn target_materialization_recovers_only_an_exact_existing_receipt() {
        let root = std::env::temp_dir().join(format!(
            "vestrace-target-materialization-recovery-{}",
            uuid::Uuid::now_v7()
        ));
        fs::create_dir(&root).unwrap();
        let plan = plan();
        assert!(!recover_target_materialization_receipt(&root, &plan).unwrap());
        fs::write(
            target_receipt_path(&root),
            format!(
                "plan_digest={}\ntimeline=1\nfinal_lsn=256\n",
                hex(plan.digest())
            ),
        )
        .unwrap();
        assert!(recover_target_materialization_receipt(&root, &plan).unwrap());
        fs::write(
            target_receipt_path(&root),
            "plan_digest=00\ntimeline=1\nfinal_lsn=256\n",
        )
        .unwrap();
        assert!(recover_target_materialization_receipt(&root, &plan).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn target_tool_arguments_bind_only_target_receipt_and_pinned_freeze() {
        let root =
            std::env::temp_dir().join(format!("vestrace-target-args-{}", uuid::Uuid::now_v7()));
        let plan = plan();
        let arguments = restore_tool_arguments(&root, &plan)
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            arguments,
            vec![
                root.join("restore-input").display().to_string(),
                "--target-receipt".to_owned(),
                target_receipt_path(&root).display().to_string(),
                "--plan-digest".to_owned(),
                encode_digest(plan.digest()),
                "--freeze-timeline".to_owned(),
                "1".to_owned(),
                "--freeze-lsn".to_owned(),
                "256".to_owned(),
            ]
        );
    }
}

#[cfg(test)]
mod target_locator_tests {
    use super::*;

    fn attempt() -> RestoreAttemptProgress {
        RestoreAttemptProgress::prepare(
            RestoreAttemptId::new(),
            RestoreTargetId::new(),
            BackupSetId::new(),
            RestoreHoldId::new(),
            DatabaseGenerationId::new(),
            DatabaseGenerationId::new(),
        )
        .unwrap()
    }

    #[test]
    fn target_locator_is_create_only_and_binds_one_absolute_target_root() {
        let root = std::env::temp_dir().join(format!(
            "vestrace-restore-target-locator-{}",
            uuid::Uuid::now_v7()
        ));
        fs::create_dir(&root).unwrap();
        let attempt = attempt();
        let target = root.join("target");

        ensure_target_locator(&root, &attempt, &target).unwrap();
        ensure_target_locator(&root, &attempt, &target).unwrap();
        assert!(verify_target_locator(&root, &attempt, &target).is_ok());
        assert!(verify_target_locator(&root, &attempt, &root.join("other-target")).is_err());
        let _ = fs::remove_dir_all(root);
    }
}

/// Stops new append permits for one set. A separate commit step waits until
/// the guarded database observes that every already-reserved append drained.
pub async fn backup_begin_sealing(set_id: uuid::Uuid) -> anyhow::Result<()> {
    backup_lifecycle_transition(ArchiveLifecycleTransition::BeginSealing {
        set_id: BackupSetId::from_uuid(set_id),
    })
    .await
}

/// Marks a previously sealed-pending set immutable only after the guarded
/// database confirms that active append intents and restore holds are absent.
pub async fn backup_commit_sealed(set_id: uuid::Uuid) -> anyhow::Result<()> {
    backup_lifecycle_transition(ArchiveLifecycleTransition::CommitSealed {
        set_id: BackupSetId::from_uuid(set_id),
    })
    .await
}

/// Binds deletion to the full current sealed archive state. This receipt is
/// later preserved in the signed key-erasure intent and guarded manifest read.
pub async fn backup_prepare_deletion(set_id: uuid::Uuid) -> anyhow::Result<()> {
    let set = BackupSetId::from_uuid(set_id);
    let roots = Roots::load()?;
    let witness = FileInstallationSafetyWitness::open(&roots.witness_root)
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let head = vestrace_application::InstallationSafetyWitness::read_head(&witness)
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let archive = head.state().backup_archive_state();
    if archive
        .lifecycle(set)
        .map_err(|error| anyhow!("backup set is invalid: {error}"))?
        != vestrace_domain::BackupSetLifecycle::Sealed
    {
        return Err(anyhow!("backup set is not sealed for deletion preparation"));
    }
    let preparation = deletion_preparation_digest(archive);
    backup_lifecycle_transition(ArchiveLifecycleTransition::PrepareDeletion {
        prepared: vestrace_application::ManagedBackupDeletionPrepared::new(set, preparation),
    })
    .await?;
    println!("managed backup deletion prepared: {set_id}");
    Ok(())
}

/// Durably pins the exact deletion preparation before touching host custody,
/// then records the separate receipt. Retrying this command never broadens
/// the target: an already-unlinked envelope is accepted only for that pinned
/// signed intent.
pub async fn backup_erase_key(set_id: uuid::Uuid) -> anyhow::Result<()> {
    let set = BackupSetId::from_uuid(set_id);
    let roots = Roots::load()?;
    let witness = FileInstallationSafetyWitness::open(&roots.witness_root)
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let head = vestrace_application::InstallationSafetyWitness::read_head(&witness)
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let archive = head.state().backup_archive_state();
    let preparation = archive
        .deletion_preparation_digest(set)
        .map_err(|error| anyhow!("backup deletion preparation is invalid: {error}"))?
        .ok_or_else(|| anyhow!("backup set has no signed deletion preparation"))?;
    match archive
        .lifecycle(set)
        .map_err(|error| anyhow!("backup set is invalid: {error}"))?
    {
        vestrace_domain::BackupSetLifecycle::DeletionPrepared => {
            if archive
                .key_erasure_preparation_digest(set)
                .map_err(|error| anyhow!("backup key-erasure intent is invalid: {error}"))?
                .is_none()
            {
                backup_lifecycle_transition(ArchiveLifecycleTransition::PrepareArchiveKeyErasure {
                    prepared: vestrace_application::ManagedBackupDeletionPrepared::new(
                        set,
                        preparation,
                    ),
                })
                .await?;
            }
            let custody = FileArchiveKeyCustody::open(roots.archive_key_root)?;
            custody
                .erase_prepared_key(vestrace_application::ManagedBackupDeletionPrepared::new(
                    set,
                    preparation,
                ))
                .await
                .map_err(|error| anyhow!("prepared archive key erasure failed: {error}"))?;
            backup_lifecycle_transition(ArchiveLifecycleTransition::RecordArchiveKeyErased {
                prepared: vestrace_application::ManagedBackupDeletionPrepared::new(
                    set,
                    preparation,
                ),
            })
            .await?;
        }
        vestrace_domain::BackupSetLifecycle::ArchiveKeyErased => {}
        lifecycle => {
            return Err(anyhow!(
                "backup set is not ready for key erasure: {lifecycle:?}"
            ));
        }
    }
    println!("managed backup archive key erased: {set_id}");
    Ok(())
}

/// Deletes only the DB-returned immutable manifest for the current signed
/// `ArchiveKeyErased` preparation. The PostgreSQL permit ends before any host
/// filesystem operation, and a retry addresses the same paths one by one.
pub async fn backup_finalize_delete(set_id: uuid::Uuid) -> anyhow::Result<()> {
    let set = BackupSetId::from_uuid(set_id);
    let roots = Roots::load()?;
    let witness = Arc::new(
        FileInstallationSafetyWitness::open(&roots.witness_root)
            .map_err(|error| anyhow!("witness is unavailable: {error}"))?,
    );
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let archive = head.state().backup_archive_state();
    let preparation = archive
        .deletion_preparation_digest(set)
        .map_err(|error| anyhow!("backup deletion preparation is invalid: {error}"))?
        .ok_or_else(|| anyhow!("backup set has no signed deletion preparation"))?;
    if archive
        .lifecycle(set)
        .map_err(|error| anyhow!("backup set is invalid: {error}"))?
        != vestrace_domain::BackupSetLifecycle::ArchiveKeyErased
        || archive
            .key_erasure_preparation_digest(set)
            .map_err(|error| anyhow!("backup key-erasure intent is invalid: {error}"))?
            != Some(preparation)
    {
        return Err(anyhow!(
            "backup set is not ready for manifest-verified final deletion"
        ));
    }
    let journal = Arc::new(
        FileSafetyJournal::open(&roots.journal_root)
            .map_err(|error| anyhow!("journal is unavailable: {error}"))?,
    );
    let store = supervisor_store().await?;
    let controller = BackupArchiveController::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        journal,
        witness,
        Arc::new(PgBackupArchiveRepository::new(store)),
        Arc::new(FileBackupObjectStore::open(roots.archive_root)?),
        Arc::new(FileArchiveKeyCustody::open(roots.archive_key_root)?),
    );
    let prepared = vestrace_application::ManagedBackupDeletionPrepared::new(set, preparation);
    let objects = controller
        .prepared_deletion_manifest(&InstallationSupervisorContext::host_supervisor(), prepared)
        .await
        .map_err(|error| anyhow!("prepared deletion manifest is unavailable: {error}"))?;
    for object in &objects {
        controller
            .remove_prepared_archive_object(prepared, object)
            .await
            .map_err(|error| anyhow!("manifest-verified archive deletion failed: {error}"))?;
    }
    backup_lifecycle_transition(ArchiveLifecycleTransition::FinalizeDeleted { set_id: set })
        .await?;
    println!("managed backup deleted from exact manifest: {set_id}");
    Ok(())
}

async fn backup_lifecycle_transition(transition: ArchiveLifecycleTransition) -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let journal_signer = load_signer(&roots.journal_signing_key)?;
    let witness = Arc::new(
        FileInstallationSafetyWitness::open(&roots.witness_root)
            .map_err(|error| anyhow!("witness is unavailable: {error}"))?,
    );
    let journal = Arc::new(
        FileSafetyJournal::open(&roots.journal_root)
            .map_err(|error| anyhow!("journal is unavailable: {error}"))?,
    );
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness.as_ref())
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let archive = head.state().backup_archive_state();
    let next_archive = match transition {
        ArchiveLifecycleTransition::BeginSealing { set_id } => archive.begin_sealing(set_id),
        ArchiveLifecycleTransition::CommitSealed { set_id } => archive.commit_sealed(set_id),
        ArchiveLifecycleTransition::PrepareDeletion { prepared } => {
            archive.prepare_deletion(prepared.set_id(), prepared.preparation_digest())
        }
        ArchiveLifecycleTransition::PrepareArchiveKeyErasure { prepared } => {
            archive.prepare_archive_key_erasure(prepared.set_id(), prepared.preparation_digest())
        }
        ArchiveLifecycleTransition::RecordArchiveKeyErased { prepared } => {
            archive.record_archive_key_erased(prepared.set_id())
        }
        ArchiveLifecycleTransition::FinalizeDeleted { set_id } => archive.finalize_deleted(set_id),
    }
    .map_err(|error| anyhow!("backup lifecycle transition is invalid: {error}"))?;
    let entry = SignedJournalEntry::sign(
        &journal_signer,
        JournalEntryToSign {
            installation_id: witness.binding().installation_id(),
            fingerprint_key_id: witness.binding().fingerprint_key_id(),
            continuity_proof: witness.binding().continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: head
                .sequence()
                .checked_add(1)
                .ok_or_else(|| anyhow!("witness sequence overflow"))?,
            previous_digest: head.chain_digest(),
            event_kind: transition.event_kind(),
            generation_id: head.active_generation_id(),
            activation_epoch: head.activation_epoch(),
            state: head.state().with_backup_archive_state(next_archive),
        },
    );
    let store = supervisor_store().await?;
    let controller = BackupArchiveController::new(
        Arc::new(PgInstallationMutationPermit::new(store.clone())),
        journal,
        witness,
        Arc::new(PgBackupArchiveRepository::new(store)),
        Arc::new(FileBackupObjectStore::open(roots.archive_root)?),
        Arc::new(FileArchiveKeyCustody::open(roots.archive_key_root)?),
    );
    controller
        .transition_lifecycle(
            &InstallationSupervisorContext::host_supervisor(),
            transition,
            entry,
        )
        .await
        .map_err(|error| anyhow!("guarded backup lifecycle transition failed: {error}"))?;
    println!(
        "managed backup lifecycle transitioned: {:?}",
        transition.event_kind()
    );
    Ok(())
}

/// Reports whether the protected host roots and the database agree about this
/// installation's safety state.
///
/// This is a read and only a read. It opens the witness and journal without
/// creating either, reads the bootstrap record's bytes rather than the
/// `open_or_create` path that would write one, takes no permit, and rolls the
/// database read back. It repairs nothing: a host head ahead of the database
/// head is what `reconcile` exists for, and readiness reporting that condition
/// as healthy -- or quietly fixing it -- would make both commands untrustworthy.
///
/// It exits zero only when every compared value matches exactly.
pub async fn readiness() -> anyhow::Result<()> {
    let roots = Roots::load()?;

    let witness = FileInstallationSafetyWitness::open(&roots.witness_root)
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let binding = witness.binding().clone();

    // The bootstrap record is the host's own statement of which installation
    // these roots belong to, and `SafetyBootstrapRecord` is its only decoder.
    // That decoder writes the record when none exists, which readiness must
    // never do, so the absent case is refused here first and the decoder is
    // reached only on the branch where it compares and never creates.
    let bootstrap_path = SafetyBootstrapRecord::path(&roots.bootstrap_root);
    if !bootstrap_path.is_file() {
        return Err(anyhow!(
            "bootstrap record is absent at {}",
            bootstrap_path.display()
        ));
    }
    SafetyBootstrapRecord::open_or_create(&roots.bootstrap_root, &binding)
        .map_err(|error| anyhow!("bootstrap record does not bind these roots: {error}"))?;

    let receipt = witness
        .durable_receipt()
        .map_err(|error| anyhow!("witness receipt is unavailable: {error}"))?;
    receipt
        .verify_against(binding.witness_public_key())
        .map_err(|error| anyhow!("witness receipt is not signed by the bound witness: {error}"))?;
    let head = witness
        .read_head()
        .await
        .map_err(|error| anyhow!("witness head is unavailable: {error}"))?;
    if head.sequence() != receipt.sequence() {
        return Err(anyhow!(
            "witness head is at {} but its durable receipt is at {}",
            head.sequence(),
            receipt.sequence()
        ));
    }

    let journal = FileSafetyJournal::open(&roots.journal_root)
        .map_err(|error| anyhow!("journal is unavailable: {error}"))?;
    let entry = journal
        .read_exact(receipt.sequence(), receipt.journal_digest())
        .map_err(|error| anyhow!("journal chain is unavailable: {error}"))?;
    entry
        .verify()
        .map_err(|error| anyhow!("journal entry signature is invalid: {error}"))?;
    if entry.signer_public_key() != binding.journal_public_key() {
        return Err(anyhow!(
            "journal entry is signed by a key the bootstrap binding does not name"
        ));
    }

    let store = supervisor_store().await?;
    let repository = PgSafetyAuthorityRepository::new(store);
    let persisted = repository
        .current()
        .await
        .map_err(|error| anyhow!("guarded safety read failed: {error}"))?
        .ok_or_else(|| anyhow!("installation safety authority is not initialized"))?;

    // Every field is compared. A readiness check that compared the sequence
    // alone would pass an installation whose database names a different
    // generation at the same position.
    let mismatches = [
        (
            "installation_id",
            persisted.installation_id() != binding.installation_id(),
        ),
        (
            "fingerprint_key_id",
            persisted.fingerprint_key_id() != binding.fingerprint_key_id(),
        ),
        (
            "fingerprint_continuity_proof",
            persisted.continuity_proof() != binding.continuity_proof(),
        ),
        (
            "journal_signer_public_key",
            persisted.journal_public_key() != binding.journal_public_key(),
        ),
        (
            "witness_public_key",
            persisted.witness_public_key() != binding.witness_public_key(),
        ),
        (
            "witness_sequence",
            persisted.sequence() != receipt.sequence(),
        ),
        (
            "journal_digest",
            persisted.journal_digest() != receipt.journal_digest(),
        ),
        (
            "witness_state",
            persisted.witness_state() != receipt.state().canonical_bytes(),
        ),
        (
            "active_generation_id",
            persisted.active_generation_id() != receipt.generation_id(),
        ),
        (
            "activation_epoch",
            persisted.activation_epoch() != receipt.activation_epoch(),
        ),
    ];
    let divergent: Vec<&str> = mismatches
        .iter()
        .filter(|(_, differs)| *differs)
        .map(|(field, _)| *field)
        .collect();
    if !divergent.is_empty() {
        return Err(anyhow!(
            "host safety roots and database diverge on: {}",
            divergent.join(", ")
        ));
    }

    // Identities and positions only. The signing keys stay on disk, and the
    // continuity proof is not printed: supervisor output reaches logs.
    println!(
        "ready installation={} sequence={} generation={} epoch={} journal_digest={}",
        persisted.installation_id().as_uuid(),
        persisted.sequence(),
        persisted.active_generation_id().as_uuid(),
        persisted.activation_epoch(),
        readiness_hex(persisted.journal_digest()),
    );
    Ok(())
}

fn readiness_hex(digest: SafetyJournalDigest) -> String {
    use std::fmt::Write as _;

    digest
        .as_bytes()
        .iter()
        .fold(String::with_capacity(64), |mut encoded, byte| {
            write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
            encoded
        })
}

/// Persists only the exact already-durable journal entry and receipt after a
/// crash between witness advancement and guarded database commit.
pub async fn reconcile() -> anyhow::Result<()> {
    let roots = Roots::load()?;
    let witness = FileInstallationSafetyWitness::open(&roots.witness_root)
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    reconcile_unjournalled_pending_appends(&roots, &witness).await?;
    let receipt = witness
        .durable_receipt()
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let journal = FileSafetyJournal::open(&roots.journal_root)
        .map_err(|error| anyhow!("journal is unavailable: {error}"))?;
    let entry = journal
        .read_exact(receipt.sequence(), receipt.journal_digest())
        .map_err(|error| anyhow!("journal is unavailable: {error}"))?;
    let store = supervisor_store().await?;
    let permit = PgInstallationMutationPermit::new(store.clone());
    let repository = PgSafetyAuthorityRepository::new(store.clone());
    let context = InstallationSupervisorContext::host_supervisor();
    let mut handle = permit
        .acquire(PermitMode::Exclusive, context.request_context())
        .await
        .map_err(|error| anyhow!("supervisor permit is unavailable: {error}"))?;
    let restore_event = matches!(
        entry.event_kind(),
        SafetyEventKind::TargetActivationPlanned
            | SafetyEventKind::RestoreTerminalRecorded
            | SafetyEventKind::RestoreAttemptPrepared
            | SafetyEventKind::SourceFreezeRecorded
            | SafetyEventKind::TargetActivating
    );
    let restore_entry = entry.clone();
    let restore_receipt = receipt.clone();
    let promotion = match entry.event_kind() {
        SafetyEventKind::InstallationInitialized => repository
            .initialize_in(
                handle.unit_of_work_mut(),
                InitializeInstallationSafety::new(
                    entry.request_id(),
                    witness.binding().clone(),
                    entry.generation_id(),
                ),
                entry,
                receipt,
            )
            .await
            .map(|_| None),
        SafetyEventKind::GenerationRegistered => repository
            .register_generation_in(
                handle.unit_of_work_mut(),
                RegisterDatabaseGeneration::new(
                    entry.request_id(),
                    entry.generation_id(),
                    entry.activation_epoch(),
                ),
                entry,
                receipt,
            )
            .await
            .map(|_| None),
        SafetyEventKind::BackupSetStarted => {
            let predecessor = journal
                .read_predecessor(&entry)
                .map_err(|error| anyhow!("journal predecessor is unavailable: {error}"))?;
            let before = predecessor.state().backup_archive_state();
            let after = entry.state().backup_archive_state();
            let candidates: Vec<_> = after
                .sets()
                .filter(|set| before.backup_set_identity(*set).is_err())
                .collect();
            let [set] = candidates.as_slice() else {
                return Err(anyhow!(
                    "backup-start reconciliation requires exactly one new set in the signed state"
                ));
            };
            let identity = after
                .backup_set_identity(*set)
                .map_err(|error| anyhow!("signed backup identity is invalid: {error}"))?;
            let initial_head = after
                .archive_head(*set)
                .map_err(|error| anyhow!("signed backup head is invalid: {error}"))?;
            let custody = FileArchiveKeyCustody::open(roots.archive_key_root.clone())
                .map_err(|error| anyhow!("archive key custody is unavailable: {error}"))?;
            custody
                .ensure_set_key(*set)
                .map_err(|error| anyhow!("archive key envelope is unavailable: {error}"))?;
            PgBackupArchiveRepository::new(store.clone())
                .start_set_in(
                    handle.unit_of_work_mut(),
                    StartManagedBackup::new(identity, initial_head, entry.clone()),
                    ArchiveKeyEnvelopeRef::from_uuid(set.as_uuid()),
                    entry,
                    receipt,
                )
                .await
                .map(|_| None)
        }
        SafetyEventKind::ArchiveCheckpointCommitted => {
            let predecessor = journal
                .read_predecessor(&entry)
                .map_err(|error| anyhow!("journal predecessor is unavailable: {error}"))?;
            let before = predecessor.state().backup_archive_state();
            let after = entry.state().backup_archive_state();
            if !after.is_exact_checkpoint_successor_of(before) {
                return Err(anyhow!(
                    "checkpoint reconciliation requires one exact signed archive successor"
                ));
            }
            let candidates: Vec<_> = after
                .sets()
                .filter(|set| after.archive_head(*set) != before.archive_head(*set))
                .collect();
            let [set] = candidates.as_slice() else {
                return Err(anyhow!(
                    "checkpoint reconciliation requires exactly one changed archive set"
                ));
            };
            let checkpoint = after
                .latest_checkpoint(*set)
                .map_err(|error| anyhow!("signed checkpoint is invalid: {error}"))?
                .cloned()
                .ok_or_else(|| anyhow!("signed checkpoint successor has no checkpoint"))?;
            let reservation = ArchiveAppendReservation::new(
                *set,
                before
                    .archive_head(*set)
                    .map_err(|error| anyhow!("signed predecessor head is invalid: {error}"))?,
                checkpoint.object().clone(),
            )
            .map_err(|error| anyhow!("signed checkpoint reservation is invalid: {error}"))?;
            let staged = vestrace_application::StagedArchiveObject::new(
                vestrace_application::ArchiveStagingId::from_uuid(
                    checkpoint.object().object_id().as_uuid(),
                ),
                checkpoint.object().clone(),
            );
            let object_store = FileBackupObjectStore::open(roots.archive_root.clone())
                .map_err(|error| anyhow!("archive object store is unavailable: {error}"))?;
            object_store
                .verify_durable(&staged)
                .await
                .map_err(|error| anyhow!("exact staged checkpoint is unavailable: {error}"))?;
            PgBackupArchiveRepository::new(store.clone())
                .commit_checkpoint_in(
                    handle.unit_of_work_mut(),
                    CommitArchiveCheckpoint::new(reservation, staged.clone()).map_err(|error| {
                        anyhow!("signed checkpoint staging is invalid: {error}")
                    })?,
                    entry,
                    receipt,
                )
                .await
                .map(|_| Some((object_store, staged)))
        }
        SafetyEventKind::ArchiveRestoreHoldAcquired => {
            let predecessor = journal
                .read_predecessor(&entry)
                .map_err(|error| anyhow!("journal predecessor is unavailable: {error}"))?;
            let before = predecessor.state().backup_archive_state();
            let after = entry.state().backup_archive_state();
            if !after.is_exact_hold_successor_of(before) {
                return Err(anyhow!(
                    "restore-hold reconciliation requires one exact signed hold successor"
                ));
            }
            let mut candidates = Vec::new();
            for set in after.sets() {
                let before_holds = before.restore_hold_ids(set).map_err(|error| {
                    anyhow!("signed predecessor restore-hold inventory is invalid: {error}")
                })?;
                let after_holds = after.restore_hold_ids(set).map_err(|error| {
                    anyhow!("signed restore-hold inventory is invalid: {error}")
                })?;
                candidates.extend(
                    after_holds
                        .into_iter()
                        .filter(|hold| !before_holds.contains(hold))
                        .map(|hold| (set, hold)),
                );
            }
            let [(set, hold)] = candidates.as_slice() else {
                return Err(anyhow!(
                    "restore-hold reconciliation requires exactly one new hold identity"
                ));
            };
            PgBackupArchiveRepository::new(store.clone())
                .acquire_hold_in(
                    handle.unit_of_work_mut(),
                    AcquireRestoreHold::new(*set, *hold),
                    entry,
                    receipt,
                )
                .await
                .map(|_| None)
        }
        event @ (SafetyEventKind::ArchiveSealingStarted
        | SafetyEventKind::ArchiveSealed
        | SafetyEventKind::ManagedBackupDeletionPrepared
        | SafetyEventKind::PreparedArchiveKeyErasure
        | SafetyEventKind::ArchiveKeyErased
        | SafetyEventKind::ManagedBackupDeleted) => {
            let predecessor = journal
                .read_predecessor(&entry)
                .map_err(|error| anyhow!("journal predecessor is unavailable: {error}"))?;
            let transition = lifecycle_transition_from_signed_successor(
                predecessor.state().backup_archive_state(),
                entry.state().backup_archive_state(),
                event,
            )?;
            PgBackupArchiveRepository::new(store.clone())
                .transition_lifecycle_in(handle.unit_of_work_mut(), transition, entry, receipt)
                .await
                .map(|_| None)
        }
        SafetyEventKind::RestoreAttemptPrepared => {
            let attempt = entry
                .state()
                .restore_attempt_progress()
                .ok_or_else(|| anyhow!("signed restore preparation has no attempt"))?;
            if attempt.source_freeze().is_some()
                || attempt.terminal_receipt().is_some()
                || journal
                    .read_predecessor(&entry)
                    .map_err(|error| anyhow!("journal predecessor is unavailable: {error}"))?
                    .state()
                    .restore_attempt_progress()
                    .is_some()
            {
                return Err(anyhow!(
                    "restore preparation reconciliation requires one exact prepared successor"
                ));
            }
            PgRestoreCutoverRepository::new(store.clone())
                .prepare_in(handle.unit_of_work_mut(), attempt)
                .await
                .map(|_| None)
        }
        SafetyEventKind::SourceFreezeRecorded => {
            let predecessor = journal
                .read_predecessor(&entry)
                .map_err(|error| anyhow!("journal predecessor is unavailable: {error}"))?;
            let attempt = predecessor
                .state()
                .restore_attempt_progress()
                .ok_or_else(|| anyhow!("signed freeze predecessor has no restore attempt"))?;
            let frozen = entry
                .state()
                .restore_attempt_progress()
                .ok_or_else(|| anyhow!("signed source freeze has no restore attempt"))?;
            let freeze = frozen
                .source_freeze()
                .ok_or_else(|| anyhow!("signed source freeze has no freeze point"))?;
            if attempt.record_source_freeze(freeze).ok().as_ref() != Some(frozen) {
                return Err(anyhow!(
                    "source-freeze reconciliation requires one exact frozen successor"
                ));
            }
            PgRestoreCutoverRepository::new(store.clone())
                .record_source_freeze_in(handle.unit_of_work_mut(), attempt, freeze)
                .await
                .map(|_| None)
        }
        SafetyEventKind::TargetActivationPlanned | SafetyEventKind::TargetActivating => Ok(None),
        SafetyEventKind::RestoreTerminalRecorded => {
            let predecessor = journal
                .read_predecessor(&entry)
                .map_err(|error| anyhow!("journal predecessor is unavailable: {error}"))?;
            let attempt = predecessor
                .state()
                .restore_attempt_progress()
                .ok_or_else(|| anyhow!("terminal reconciliation has no restore attempt"))?;
            let terminal_attempt = entry
                .state()
                .restore_attempt_progress()
                .ok_or_else(|| anyhow!("terminal reconciliation has no signed restore attempt"))?;
            let terminal = terminal_attempt
                .terminal_receipt()
                .ok_or_else(|| anyhow!("terminal reconciliation has no terminal receipt"))?;
            if attempt
                .record_terminal_receipt(terminal.clone())
                .ok()
                .as_ref()
                != Some(terminal_attempt)
            {
                return Err(anyhow!(
                    "terminal reconciliation requires one exact terminal successor"
                ));
            }
            match terminal.release_reason() {
                RestoreHoldReleaseReason::TargetInitializationComplete { .. } => {
                    PgRestoreCutoverRepository::new(store.clone())
                        .record_target_initialized_in(
                            handle.unit_of_work_mut(),
                            attempt,
                            terminal.release_reason().receipt(),
                        )
                        .await
                        .map(|_| None)
                }
                RestoreHoldReleaseReason::SourceResumePrepared { .. } => {
                    PgRestoreCutoverRepository::new(store.clone())
                        .record_source_resume_prepared_in(
                            handle.unit_of_work_mut(),
                            attempt,
                            terminal.release_reason().receipt(),
                        )
                        .await
                        .map(|_| None)
                }
                RestoreHoldReleaseReason::RefusedTargetDestroyed { .. }
                | RestoreHoldReleaseReason::SalvageInstallationCompleted { .. } => {
                    return Err(anyhow!(
                        "terminal reconciliation refuses a release reason outside P05-C"
                    ));
                }
            }
        }
    }
    .map_err(|error| anyhow!("guarded safety reconciliation failed: {error}"))?;
    if restore_event {
        PgRestoreCutoverRepository::new(store.clone())
            .record_restore_safety_event_in(
                handle.unit_of_work_mut(),
                restore_entry,
                restore_receipt,
            )
            .await
            .map_err(|error| anyhow!("restore safety-event reconciliation failed: {error}"))?;
    }
    handle
        .commit()
        .await
        .map_err(|error| anyhow!("supervisor permit commit failed: {error}"))?;
    if let Some((object_store, staged)) = promotion {
        object_store
            .promote_after_checkpoint(&staged)
            .map_err(|error| anyhow!("exact checkpoint promotion failed: {error}"))?;
    }
    println!("installation safety reconciled");
    Ok(())
}

fn lifecycle_transition_from_signed_successor(
    before: &vestrace_domain::BackupArchiveStateV1,
    after: &vestrace_domain::BackupArchiveStateV1,
    event: SafetyEventKind,
) -> anyhow::Result<ArchiveLifecycleTransition> {
    use vestrace_domain::BackupSetLifecycle;

    if event == SafetyEventKind::ManagedBackupDeletionPrepared {
        if !after.is_exact_prepared_deletion_successor_of(before) {
            return Err(anyhow!(
                "deletion reconciliation requires one exact signed prepared successor"
            ));
        }
        let set = one_changed_lifecycle_set(before, after, BackupSetLifecycle::DeletionPrepared)?;
        let preparation = after
            .deletion_preparation_digest(set)
            .map_err(|error| anyhow!("signed deletion preparation is invalid: {error}"))?
            .ok_or_else(|| anyhow!("signed deletion preparation digest is absent"))?;
        return Ok(ArchiveLifecycleTransition::PrepareDeletion {
            prepared: vestrace_application::ManagedBackupDeletionPrepared::new(set, preparation),
        });
    }
    if event == SafetyEventKind::PreparedArchiveKeyErasure {
        if !after.is_exact_key_erasure_prepared_successor_of(before) {
            return Err(anyhow!(
                "archive key-erasure reconciliation requires one exact signed intent successor"
            ));
        }
        let candidates: Vec<_> = after
            .sets()
            .filter(|set| {
                before.key_erasure_preparation_digest(*set).ok()
                    != after.key_erasure_preparation_digest(*set).ok()
            })
            .collect();
        let [set] = candidates.as_slice() else {
            return Err(anyhow!(
                "archive key-erasure reconciliation requires exactly one changed set"
            ));
        };
        let preparation = after
            .key_erasure_preparation_digest(*set)
            .map_err(|error| anyhow!("signed key-erasure preparation is invalid: {error}"))?
            .ok_or_else(|| anyhow!("signed key-erasure preparation digest is absent"))?;
        return Ok(ArchiveLifecycleTransition::PrepareArchiveKeyErasure {
            prepared: vestrace_application::ManagedBackupDeletionPrepared::new(*set, preparation),
        });
    }
    let (from, to) = match event {
        SafetyEventKind::ArchiveSealingStarted => {
            (BackupSetLifecycle::Streaming, BackupSetLifecycle::Sealing)
        }
        SafetyEventKind::ArchiveSealed => (BackupSetLifecycle::Sealing, BackupSetLifecycle::Sealed),
        SafetyEventKind::ManagedBackupDeleted => (
            BackupSetLifecycle::ArchiveKeyErased,
            BackupSetLifecycle::Deleted,
        ),
        SafetyEventKind::ArchiveKeyErased => {
            let from = BackupSetLifecycle::DeletionPrepared;
            let to = BackupSetLifecycle::ArchiveKeyErased;
            if !after.is_exact_lifecycle_successor_of(before, from, to) {
                return Err(anyhow!(
                    "archive lifecycle reconciliation requires one exact signed successor"
                ));
            }
            let set = one_changed_lifecycle_set(before, after, to)?;
            let preparation = after
                .deletion_preparation_digest(set)
                .map_err(|error| anyhow!("signed deletion preparation is invalid: {error}"))?
                .ok_or_else(|| anyhow!("signed deletion preparation digest is absent"))?;
            return Ok(ArchiveLifecycleTransition::RecordArchiveKeyErased {
                prepared: vestrace_application::ManagedBackupDeletionPrepared::new(
                    set,
                    preparation,
                ),
            });
        }
        _ => return Err(anyhow!("event is not an archive lifecycle successor")),
    };
    if !after.is_exact_lifecycle_successor_of(before, from, to) {
        return Err(anyhow!(
            "archive lifecycle reconciliation requires one exact signed successor"
        ));
    }
    let set = one_changed_lifecycle_set(before, after, to)?;
    Ok(match event {
        SafetyEventKind::ArchiveSealingStarted => {
            ArchiveLifecycleTransition::BeginSealing { set_id: set }
        }
        SafetyEventKind::ArchiveSealed => ArchiveLifecycleTransition::CommitSealed { set_id: set },
        SafetyEventKind::ManagedBackupDeleted => {
            ArchiveLifecycleTransition::FinalizeDeleted { set_id: set }
        }
        _ => unreachable!("event was matched above"),
    })
}

fn one_changed_lifecycle_set(
    before: &vestrace_domain::BackupArchiveStateV1,
    after: &vestrace_domain::BackupArchiveStateV1,
    target: vestrace_domain::BackupSetLifecycle,
) -> anyhow::Result<BackupSetId> {
    let mut candidates = after.sets().filter(|set| {
        before.lifecycle(*set).ok() != after.lifecycle(*set).ok()
            && after.lifecycle(*set).ok() == Some(target)
    });
    let Some(set) = candidates.next() else {
        return Err(anyhow!(
            "archive lifecycle reconciliation has no changed set"
        ));
    };
    if candidates.next().is_some() {
        return Err(anyhow!(
            "archive lifecycle reconciliation has multiple changed sets"
        ));
    }
    Ok(set)
}

/// Removes only durable intents whose witnessed predecessor is still current.
/// A pending intent whose head has changed might instead belong to a durable
/// post-witness checkpoint, so recovery leaves it for exact receipt replay.
async fn reconcile_unjournalled_pending_appends(
    roots: &Roots,
    witness: &FileInstallationSafetyWitness,
) -> anyhow::Result<()> {
    let head = vestrace_application::InstallationSafetyWitness::read_head(witness)
        .await
        .map_err(|error| anyhow!("witness is unavailable: {error}"))?;
    let archive = head.state().backup_archive_state();
    let store = supervisor_store().await?;
    let permit = PgInstallationMutationPermit::new(store.clone());
    let repository = PgBackupArchiveRepository::new(store);
    let context = InstallationSupervisorContext::host_supervisor();
    let mut handle = permit
        .acquire(PermitMode::Exclusive, context.request_context())
        .await
        .map_err(|error| anyhow!("supervisor permit is unavailable: {error}"))?;
    let pending = repository
        .list_pending_appends_in(handle.unit_of_work_mut())
        .await
        .map_err(|error| anyhow!("pending archive intent inventory is unavailable: {error}"))?;
    let mut cancelled = Vec::new();
    for intent in pending {
        if archive.archive_head(intent.set_id()).ok() == Some(intent.expected_head()) {
            repository
                .abandon_pending_append_in(handle.unit_of_work_mut(), intent)
                .await
                .map_err(|error| anyhow!("pending archive intent cancellation failed: {error}"))?;
            cancelled.push(intent);
        }
    }
    handle
        .commit()
        .await
        .map_err(|error| anyhow!("pending archive intent commit failed: {error}"))?;
    let object_store = FileBackupObjectStore::open(roots.archive_root.clone())
        .map_err(|error| anyhow!("archive object store is unavailable: {error}"))?;
    for intent in cancelled {
        object_store
            .remove_cancelled_pending_staging(intent.set_id(), intent.object_id())
            .map_err(|error| anyhow!("pending archive staging cleanup failed: {error}"))?;
    }
    Ok(())
}

struct Roots {
    journal_root: PathBuf,
    witness_root: PathBuf,
    bootstrap_root: PathBuf,
    journal_signing_key: PathBuf,
    archive_root: PathBuf,
    archive_key_root: PathBuf,
}

impl Roots {
    fn load() -> anyhow::Result<Self> {
        let roots = Self {
            // This is deliberately environment-only: a CLI argument could
            // point an operator at an arbitrary host directory and claim it
            // was the engine-resolved dedicated volume.
            journal_root: required_path("VESTRACE_SAFETY_JOURNAL_ROOT")?,
            witness_root: required_path("VESTRACE_SAFETY_WITNESS_ROOT")?,
            bootstrap_root: required_path("VESTRACE_SAFETY_BOOTSTRAP_ROOT")?,
            journal_signing_key: required_path("VESTRACE_SAFETY_JOURNAL_SIGNING_KEY")?,
            archive_root: required_path("VESTRACE_BACKUP_ARCHIVE_ROOT")?,
            archive_key_root: required_path("VESTRACE_BACKUP_ARCHIVE_KEY_ROOT")?,
        };
        for (name, path) in [
            ("VESTRACE_BACKUP_ARCHIVE_ROOT", &roots.archive_root),
            ("VESTRACE_BACKUP_ARCHIVE_KEY_ROOT", &roots.archive_key_root),
        ] {
            for protected in [
                &roots.journal_root,
                &roots.witness_root,
                &roots.bootstrap_root,
            ] {
                if path == protected || path.starts_with(protected) || protected.starts_with(path) {
                    return Err(anyhow!("{name} must not overlap a safety authority root"));
                }
            }
        }
        if roots.archive_root == roots.archive_key_root
            || roots.archive_root.starts_with(&roots.archive_key_root)
            || roots.archive_key_root.starts_with(&roots.archive_root)
        {
            return Err(anyhow!("archive object and key roots must be disjoint"));
        }
        Ok(roots)
    }
}

async fn supervisor_store() -> anyhow::Result<PgStore> {
    let url = env::var("VESTRACE_SAFETY_SUPERVISOR_DATABASE_URL")
        .map_err(|_| anyhow!("VESTRACE_SAFETY_SUPERVISOR_DATABASE_URL is required"))?;
    PgStore::connect(&DatabaseConfig {
        url: SecretString::from(url),
        max_connections: 1,
    })
    .await
    .map_err(|error| anyhow!("supervisor database is unavailable: {error}"))
}

fn required_path(name: &str) -> anyhow::Result<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| anyhow!("{name} must name an absolute protected host path"))
}

const BASE_CAPTURE_MANIFEST_LIMIT: u64 = 1024 * 1024;
const PGBACKUP_STDERR_DIAGNOSTIC_LIMIT: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BaseCaptureMetadata {
    timeline: u32,
    start_lsn: u64,
    system_identifier: u64,
}

fn run_pg_basebackup(binary: &Path, source_dsn: &str, capture: &Path) -> anyhow::Result<()> {
    let mut child = Command::new(binary)
        .args(pg_basebackup_arguments(source_dsn, capture))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| anyhow!("launch pg_basebackup: {error}"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("pg_basebackup stderr pipe is unavailable"))?;
    let reader = thread::spawn(move || read_bounded_stderr(stderr));
    let status = child
        .wait()
        .map_err(|error| anyhow!("wait for pg_basebackup: {error}"))?;
    let _diagnostic = reader
        .join()
        .map_err(|_| anyhow!("pg_basebackup stderr reader panicked"))?
        .map_err(|error| anyhow!("read pg_basebackup stderr: {error}"))?;
    if !status.success() {
        return Err(anyhow!(
            "pg_basebackup exited with {status}; bounded stderr diagnostic was redacted to avoid exposing the source DSN; protected capture retained for inspection"
        ));
    }
    Ok(())
}

fn pg_basebackup_arguments(source_dsn: &str, capture: &Path) -> [std::ffi::OsString; 8] {
    [
        "--dbname".into(),
        source_dsn.into(),
        "--format=tar".into(),
        "--wal-method=none".into(),
        "--checkpoint=fast".into(),
        "--manifest-force-encode".into(),
        "--pgdata".into(),
        capture.as_os_str().to_owned(),
    ]
}

fn read_bounded_stderr(mut stderr: impl Read) -> std::io::Result<Vec<u8>> {
    let mut diagnostic = Vec::with_capacity(PGBACKUP_STDERR_DIAGNOSTIC_LIMIT);
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stderr.read(&mut buffer)?;
        if read == 0 {
            return Ok(diagnostic);
        }
        let remaining = PGBACKUP_STDERR_DIAGNOSTIC_LIMIT.saturating_sub(diagnostic.len());
        diagnostic.extend_from_slice(&buffer[..read.min(remaining)]);
    }
}

fn require_base_capture_layout(capture: &Path, base: &Path, manifest: &Path) -> anyhow::Result<()> {
    let mut entries = fs::read_dir(capture)
        .map_err(|error| anyhow!("inspect protected base capture directory: {error}"))?
        .map(|entry| {
            entry.map_err(|error| anyhow!("read protected base capture directory: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    if entries.len() != 2
        || !entries.iter().any(|entry| entry.path() == base)
        || !entries.iter().any(|entry| entry.path() == manifest)
        || entries.iter().any(|entry| {
            !entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
    {
        return Err(anyhow!(
            "pg_basebackup produced an unsupported capture layout; protected capture retained for inspection"
        ));
    }
    Ok(())
}

fn base_capture_metadata(base: &Path, manifest: &Path) -> anyhow::Result<BaseCaptureMetadata> {
    let (timeline, start_lsn) = backup_label_start(base)?;
    Ok(BaseCaptureMetadata {
        timeline,
        start_lsn,
        system_identifier: backup_manifest_system_identifier(manifest)?,
    })
}

fn backup_manifest_system_identifier(manifest: &Path) -> anyhow::Result<u64> {
    let mut input = File::open(manifest)
        .map_err(|error| anyhow!("open captured backup manifest: {error}"))?
        .take(BASE_CAPTURE_MANIFEST_LIMIT + 1);
    let mut manifest_bytes = Vec::with_capacity(BASE_CAPTURE_MANIFEST_LIMIT as usize);
    input
        .read_to_end(&mut manifest_bytes)
        .map_err(|error| anyhow!("read captured backup manifest: {error}"))?;
    if manifest_bytes.len() as u64 > BASE_CAPTURE_MANIFEST_LIMIT {
        return Err(anyhow!(
            "captured backup manifest exceeds the bounded parser limit"
        ));
    }
    let document: serde_json::Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| anyhow!("captured backup manifest is not valid JSON"))?;
    if document
        .get("PostgreSQL-Backup-Manifest-Version")
        .and_then(serde_json::Value::as_u64)
        != Some(2)
    {
        return Err(anyhow!(
            "captured backup manifest must be PostgreSQL format version 2"
        ));
    }
    document
        .get("System-Identifier")
        .and_then(serde_json::Value::as_u64)
        .filter(|identifier| *identifier != 0)
        .ok_or_else(|| anyhow!("captured backup manifest has no valid system identifier"))
}

fn required_source_system_identifier() -> anyhow::Result<u64> {
    env::var("VESTRACE_PG_BASEBACKUP_SOURCE_SYSTEM_IDENTIFIER")
        .map_err(|_| anyhow!("VESTRACE_PG_BASEBACKUP_SOURCE_SYSTEM_IDENTIFIER is required"))?
        .parse::<u64>()
        .ok()
        .filter(|identifier| *identifier != 0)
        .ok_or_else(|| anyhow!("VESTRACE_PG_BASEBACKUP_SOURCE_SYSTEM_IDENTIFIER must be a nonzero decimal identifier"))
}

fn require_pg_basebackup_binary(binary: &Path) -> anyhow::Result<()> {
    let metadata = fs::metadata(binary)
        .map_err(|error| anyhow!("pg_basebackup binary is unavailable: {error}"))?;
    if !metadata.is_file() {
        return Err(anyhow!(
            "VESTRACE_PG_BASEBACKUP_BIN must name a regular file"
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(anyhow!("VESTRACE_PG_BASEBACKUP_BIN must be executable"));
        }
    }
    #[cfg(windows)]
    {
        let extension = binary
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        if !matches!(extension.as_deref(), Some("exe" | "com" | "bat" | "cmd")) {
            return Err(anyhow!(
                "VESTRACE_PG_BASEBACKUP_BIN must name an executable Windows file"
            ));
        }
    }
    Ok(())
}

fn backup_label_start(base: &Path) -> anyhow::Result<(u32, u64)> {
    let mut tar = File::open(base).map_err(|error| anyhow!("open captured base tar: {error}"))?;
    let mut header = [0_u8; 512];
    let mut parsed = None;
    loop {
        tar.read_exact(&mut header)
            .map_err(|error| anyhow!("read captured base tar header: {error}"))?;
        if header.iter().all(|byte| *byte == 0) {
            return parsed.ok_or_else(|| anyhow!("captured base tar has no backup_label"));
        }
        let name = std::str::from_utf8(&header[..100])
            .map_err(|_| anyhow!("captured base tar member name is not UTF-8"))?
            .trim_end_matches('\0');
        let size = std::str::from_utf8(&header[124..136])
            .map_err(|_| anyhow!("captured base tar member size is malformed"))?
            .trim_matches(['\0', ' ']);
        let size = u64::from_str_radix(size, 8)
            .map_err(|_| anyhow!("captured base tar member size is malformed"))?;
        if name == "backup_label" {
            if parsed.is_some() {
                return Err(anyhow!(
                    "captured base tar has duplicate backup_label members"
                ));
            }
            if !matches!(header[156], 0 | b'0') {
                return Err(anyhow!("captured backup_label is not a regular tar member"));
            }
            if size > 64 * 1024 {
                return Err(anyhow!(
                    "captured backup_label exceeds the bounded parser limit"
                ));
            }
            let mut label = vec![0_u8; size as usize];
            tar.read_exact(&mut label)
                .map_err(|error| anyhow!("read captured backup_label: {error}"))?;
            parsed = Some(parse_backup_label(&label)?);
            let padded = size
                .checked_add(511)
                .ok_or_else(|| anyhow!("captured tar size overflows"))?
                / 512
                * 512;
            let padding = padded
                .checked_sub(size)
                .ok_or_else(|| anyhow!("captured tar padding underflows"))?;
            tar.seek(SeekFrom::Current(
                i64::try_from(padding).map_err(|_| anyhow!("captured tar padding is too large"))?,
            ))
            .map_err(|error| anyhow!("skip captured backup_label padding: {error}"))?;
            continue;
        }
        let padded = size
            .checked_add(511)
            .ok_or_else(|| anyhow!("captured tar size overflows"))?
            / 512
            * 512;
        tar.seek(SeekFrom::Current(
            i64::try_from(padded).map_err(|_| anyhow!("captured tar member is too large"))?,
        ))
        .map_err(|error| anyhow!("skip captured base tar member: {error}"))?;
    }
}

fn parse_backup_label(label: &[u8]) -> anyhow::Result<(u32, u64)> {
    let label = std::str::from_utf8(label).map_err(|_| anyhow!("backup_label is not UTF-8"))?;
    let line = label
        .lines()
        .find(|line| line.starts_with("START WAL LOCATION: "))
        .ok_or_else(|| anyhow!("backup_label has no start WAL location"))?;
    let value = line.trim_start_matches("START WAL LOCATION: ");
    let (lsn, file) = value
        .split_once(" (file ")
        .and_then(|(lsn, file)| file.strip_suffix(')').map(|file| (lsn, file)))
        .ok_or_else(|| anyhow!("backup_label start WAL location is malformed"))?;
    let (high, low) = lsn
        .split_once('/')
        .ok_or_else(|| anyhow!("backup_label start LSN is malformed"))?;
    let start_lsn = (u64::from(
        u32::from_str_radix(high, 16)
            .map_err(|_| anyhow!("backup_label start LSN is malformed"))?,
    ) << 32)
        | u64::from(
            u32::from_str_radix(low, 16)
                .map_err(|_| anyhow!("backup_label start LSN is malformed"))?,
        );
    if file.len() != 24 || !file.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(anyhow!("backup_label WAL filename is malformed"));
    }
    let timeline = u32::from_str_radix(
        file.get(..8)
            .ok_or_else(|| anyhow!("backup_label WAL filename is malformed"))?,
        16,
    )
    .map_err(|_| anyhow!("backup_label WAL timeline is malformed"))?;
    if timeline == 0 {
        return Err(anyhow!("backup_label WAL timeline must be nonzero"));
    }
    Ok((timeline, start_lsn))
}

#[cfg(test)]
mod base_capture_parser_tests {
    use super::*;

    fn tar_member(name: &str, contents: &[u8]) -> Vec<u8> {
        let mut header = [0_u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        let size = format!("{:011o}\0", contents.len());
        header[124..136].copy_from_slice(size.as_bytes());
        header[156] = b'0';
        let mut tar = header.to_vec();
        tar.extend_from_slice(contents);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
        tar
    }

    #[test]
    fn parses_a_bounded_backup_label_start_position() {
        let label = b"START WAL LOCATION: 16/B374D848 (file 0000000100000016000000B3)\n";
        assert_eq!(parse_backup_label(label).unwrap(), (1, 0x16_B374D848));
    }

    #[test]
    fn refuses_ambiguous_or_overflowing_backup_label_fields() {
        assert!(
            parse_backup_label(
                b"START WAL LOCATION: 100000000/1 (file 000000010000000000000001)\n"
            )
            .is_err()
        );
        assert!(parse_backup_label(b"START WAL LOCATION: 1/2 (file 00000001)\n").is_err());
        assert!(
            parse_backup_label(b"START WAL LOCATION: 1/2 (file 0000000Z0000000000000001)\n")
                .is_err()
        );
    }

    #[test]
    fn tar_reader_refuses_duplicate_backup_labels() {
        let label = b"START WAL LOCATION: 16/B374D848 (file 0000000100000016000000B3)\n";
        let mut tar = tar_member("backup_label", label);
        tar.extend_from_slice(&tar_member("backup_label", label));
        tar.extend_from_slice(&[0_u8; 1024]);
        let path =
            std::env::temp_dir().join(format!("vestrace-base-label-{}.tar", uuid::Uuid::now_v7()));
        fs::write(&path, tar).unwrap();
        assert!(backup_label_start(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn pg_basebackup_arguments_keep_the_source_dsn_as_one_argument() {
        let capture = PathBuf::from("/protected/capture");
        let source = "postgresql://backup:secret@example.test:5432/vestrace?application_name=base";
        let arguments = pg_basebackup_arguments(source, &capture);
        assert_eq!(arguments[0], "--dbname");
        assert_eq!(arguments[1], source);
        assert_eq!(arguments[2], "--format=tar");
        assert_eq!(arguments[4], "--checkpoint=fast");
        assert_eq!(arguments[5], "--manifest-force-encode");
        assert_eq!(arguments[7], capture.as_os_str());
    }

    #[test]
    fn base_capture_layout_requires_exact_base_and_manifest_files() {
        let capture = std::env::temp_dir().join(format!(
            "vestrace-base-capture-layout-{}",
            uuid::Uuid::now_v7()
        ));
        fs::create_dir_all(&capture).unwrap();
        let base = capture.join("base.tar");
        let manifest = capture.join("backup_manifest");
        fs::write(&base, b"base").unwrap();
        fs::write(&manifest, b"{}").unwrap();
        require_base_capture_layout(&capture, &base, &manifest).unwrap();
        fs::write(capture.join("unrecognized"), b"retain").unwrap();
        assert!(require_base_capture_layout(&capture, &base, &manifest).is_err());
        fs::remove_dir_all(capture).unwrap();
    }

    #[test]
    fn manifest_parser_pins_a_nonzero_postgres_17_system_identifier() {
        let path = std::env::temp_dir().join(format!(
            "vestrace-base-manifest-{}.json",
            uuid::Uuid::now_v7()
        ));
        fs::write(
            &path,
            br#"{"PostgreSQL-Backup-Manifest-Version":2,"System-Identifier":7684887527176183846,"Files":[]}"#,
        )
        .unwrap();
        assert_eq!(
            backup_manifest_system_identifier(&path).unwrap(),
            7_684_887_527_176_183_846
        );
        fs::write(
            &path,
            br#"{"PostgreSQL-Backup-Manifest-Version":1,"System-Identifier":7684887527176183846}"#,
        )
        .unwrap();
        assert!(backup_manifest_system_identifier(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stderr_capture_is_bounded_while_draining_the_child_pipe() {
        let diagnostic = read_bounded_stderr(std::io::Cursor::new(vec![7_u8; 8192])).unwrap();
        assert_eq!(diagnostic.len(), PGBACKUP_STDERR_DIAGNOSTIC_LIMIT);
    }
}

fn load_signer(path: &PathBuf) -> anyhow::Result<Ed25519KeyPair> {
    Ed25519KeyPair::from_pkcs8(&fs::read(path)?)
        .map_err(|_| anyhow!("journal signing key is not valid PKCS#8"))
}

fn witness_public_key(root: &std::path::Path) -> anyhow::Result<vestrace_domain::WitnessPublicKey> {
    let key = Ed25519KeyPair::from_pkcs8(&fs::read(root.join("installation-safety-witness.pk8"))?)
        .map_err(|_| anyhow!("witness signing key is not valid PKCS#8"))?;
    Ok(vestrace_domain::WitnessPublicKey::from_bytes(
        key.public_key()
            .as_ref()
            .try_into()
            .map_err(|_| anyhow!("witness signing key is not Ed25519"))?,
    ))
}

fn parse_hex_32(value: &str) -> anyhow::Result<[u8; 32]> {
    if value.len() != 64 {
        return Err(anyhow!(
            "fingerprint continuity proof must be 32-byte hexadecimal"
        ));
    }
    let mut bytes = [0_u8; 32];
    for (index, output) in bytes.iter_mut().enumerate() {
        *output = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| anyhow!("fingerprint continuity proof must be hexadecimal"))?;
    }
    Ok(bytes)
}

fn deletion_preparation_digest(
    archive: &vestrace_domain::BackupArchiveStateV1,
) -> SafetyJournalDigest {
    let mut receipt = b"vestrace-managed-backup-deletion-preparation-v1".to_vec();
    receipt.extend_from_slice(&archive.canonical_bytes());
    SafetyJournalDigest::of(&receipt)
}

#[cfg(test)]
mod lifecycle_reconciliation_tests {
    use super::*;
    use vestrace_domain::{
        ArchiveHead, ArchiveObjectDescriptor, ArchiveObjectDescriptorInput, ArchiveObjectKind,
        BackupArchiveStateV1, BackupObjectId, BackupSetIdentity, DatabaseGenerationId,
        InstallationId, WalArchiveCheckpoint,
    };

    fn streaming() -> (BackupArchiveStateV1, BackupSetId) {
        let set = BackupSetId::new();
        let state = BackupArchiveStateV1::empty()
            .start_streaming(
                BackupSetIdentity::new(set, InstallationId::new(), DatabaseGenerationId::new()),
                ArchiveHead::genesis(SafetyJournalDigest::of(b"lifecycle-reconcile")),
            )
            .unwrap();
        (state, set)
    }

    fn restorable() -> (BackupArchiveStateV1, BackupSetId) {
        let (streaming, set) = streaming();
        let head = streaming.archive_head(set).unwrap();
        let base = ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
            set_id: set,
            object_id: BackupObjectId::new(),
            kind: ArchiveObjectKind::BaseChunk,
            ordinal: 1,
            timeline: 1,
            start_lsn: 100,
            end_lsn: 100,
            ciphertext_digest: SafetyJournalDigest::of(b"base-ciphertext"),
            plaintext_digest: SafetyJournalDigest::of(b"base-plaintext"),
            length: 64,
            predecessor_head_digest: head.digest,
        })
        .unwrap();
        let reservation = streaming.reserve_append(set, head, base).unwrap();
        (
            streaming
                .commit_checkpoint(
                    reservation.clone(),
                    WalArchiveCheckpoint::from_reservation(&reservation),
                )
                .unwrap(),
            set,
        )
    }

    #[test]
    fn prepared_deletion_reconciliation_reuses_the_signed_preparation_digest() {
        let (streaming, set) = restorable();
        let sealed = streaming
            .begin_sealing(set)
            .unwrap()
            .commit_sealed(set)
            .unwrap();
        let preparation = SafetyJournalDigest::of(b"one-exact-preparation");
        let prepared = sealed.prepare_deletion(set, preparation).unwrap();

        assert_eq!(
            lifecycle_transition_from_signed_successor(
                &sealed,
                &prepared,
                SafetyEventKind::ManagedBackupDeletionPrepared,
            )
            .unwrap(),
            ArchiveLifecycleTransition::PrepareDeletion {
                prepared: vestrace_application::ManagedBackupDeletionPrepared::new(
                    set,
                    preparation
                ),
            }
        );
    }

    #[test]
    fn key_erasure_reconciliation_reuses_the_signed_intent_digest() {
        let (streaming, set) = restorable();
        let preparation = SafetyJournalDigest::of(b"one-exact-key-erasure-intent");
        let prepared = streaming
            .begin_sealing(set)
            .unwrap()
            .commit_sealed(set)
            .unwrap()
            .prepare_deletion(set, preparation)
            .unwrap();
        let intent = prepared
            .prepare_archive_key_erasure(set, preparation)
            .unwrap();

        assert_eq!(
            lifecycle_transition_from_signed_successor(
                &prepared,
                &intent,
                SafetyEventKind::PreparedArchiveKeyErasure,
            )
            .unwrap(),
            ArchiveLifecycleTransition::PrepareArchiveKeyErasure {
                prepared: vestrace_application::ManagedBackupDeletionPrepared::new(
                    set,
                    preparation
                ),
            }
        );
    }

    #[test]
    fn sealing_reconciliation_refuses_a_non_exact_state_change() {
        let (streaming, set) = restorable();
        let sealed = streaming
            .begin_sealing(set)
            .unwrap()
            .commit_sealed(set)
            .unwrap();

        assert!(
            lifecycle_transition_from_signed_successor(
                &streaming,
                &sealed,
                SafetyEventKind::ArchiveSealingStarted,
            )
            .is_err()
        );
    }
}
