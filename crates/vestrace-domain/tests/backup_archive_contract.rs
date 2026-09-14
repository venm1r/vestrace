use vestrace_domain::{
    ArchiveHead, ArchiveObjectDescriptor, ArchiveObjectDescriptorInput, ArchiveObjectKind,
    BackupArchiveError, BackupArchiveStateV1, BackupObjectId, BackupSetId, BackupSetIdentity,
    DatabaseGenerationId, InstallationId, RestoreHoldId, SafetyJournalDigest, WalArchiveCheckpoint,
    WitnessStateV1,
};

fn digest(label: &[u8]) -> SafetyJournalDigest {
    SafetyJournalDigest::of(label)
}

fn streaming_set() -> (BackupArchiveStateV1, BackupSetId, ArchiveHead) {
    let set = BackupSetId::new();
    let head = ArchiveHead::genesis(digest(b"archive-genesis"));
    let state = BackupArchiveStateV1::empty()
        .start_streaming(
            BackupSetIdentity::new(set, InstallationId::new(), DatabaseGenerationId::new()),
            head,
        )
        .unwrap();
    (state, set, head)
}

fn object(set: BackupSetId, head: ArchiveHead, kind: ArchiveObjectKind) -> ArchiveObjectDescriptor {
    ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
        set_id: set,
        object_id: BackupObjectId::new(),
        kind,
        ordinal: head.checkpoint_ordinal + 1,
        timeline: 1,
        start_lsn: 100,
        end_lsn: 200,
        ciphertext_digest: digest(b"ciphertext"),
        plaintext_digest: digest(b"plaintext"),
        length: 64,
        predecessor_head_digest: head.digest,
    })
    .unwrap()
}

fn with_base(
    streaming: BackupArchiveStateV1,
    set: BackupSetId,
    head: ArchiveHead,
) -> BackupArchiveStateV1 {
    let base = streaming
        .reserve_append(set, head, object(set, head, ArchiveObjectKind::BaseChunk))
        .unwrap();
    streaming
        .commit_checkpoint(base.clone(), WalArchiveCheckpoint::from_reservation(&base))
        .unwrap()
}

#[test]
fn base_checkpoint_is_required_before_wal_or_restore_hold() {
    let (streaming, set, head) = streaming_set();
    assert_eq!(
        streaming.reserve_append(set, head, object(set, head, ArchiveObjectKind::WalSegment)),
        Err(BackupArchiveError::BaseCheckpointRequired)
    );
    assert_eq!(
        streaming.acquire_restore_hold(set, RestoreHoldId::new()),
        Err(BackupArchiveError::NotRestorable)
    );

    let base = streaming
        .reserve_append(set, head, object(set, head, ArchiveObjectKind::BaseChunk))
        .unwrap();
    let captured = streaming
        .commit_checkpoint(base.clone(), WalArchiveCheckpoint::from_reservation(&base))
        .unwrap();
    assert_eq!(
        captured.reserve_append(
            set,
            captured.archive_head(set).unwrap(),
            object(
                set,
                captured.archive_head(set).unwrap(),
                ArchiveObjectKind::BaseChunk,
            ),
        ),
        Err(BackupArchiveError::DuplicateBaseCheckpoint)
    );
    assert!(
        captured
            .acquire_restore_hold(set, RestoreHoldId::new())
            .is_ok()
    );

    let head = captured.archive_head(set).unwrap();
    let wrong_timeline = ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
        timeline: 2,
        ..object(set, head, ArchiveObjectKind::WalSegment).to_input()
    })
    .unwrap();
    assert_eq!(
        captured.reserve_append(set, head, wrong_timeline),
        Err(BackupArchiveError::BaseCheckpointAncestryMismatch)
    );
}

#[test]
fn sealing_refuses_a_hold_or_append_and_checkpoint_cas_needs_the_exact_head() {
    let (streaming, set, head) = streaming_set();
    let streaming = with_base(streaming, set, head);
    let head = streaming.archive_head(set).unwrap();
    let held = streaming
        .acquire_restore_hold(set, RestoreHoldId::new())
        .unwrap();
    assert_eq!(
        held.begin_sealing(set),
        Err(BackupArchiveError::RestoreHoldPresent)
    );

    let sealed = streaming
        .begin_sealing(set)
        .unwrap()
        .commit_sealed(set)
        .unwrap();
    assert_eq!(
        sealed.acquire_restore_hold(set, RestoreHoldId::new()),
        Err(BackupArchiveError::NotRestorable)
    );
    assert_eq!(
        sealed.reserve_append(set, head, object(set, head, ArchiveObjectKind::WalSegment)),
        Err(BackupArchiveError::NotStreaming)
    );

    let reservation = streaming
        .reserve_append(set, head, object(set, head, ArchiveObjectKind::WalSegment))
        .unwrap();
    let checkpoint = WalArchiveCheckpoint::from_reservation(&reservation);
    let replay_checkpoint = WalArchiveCheckpoint::from_reservation(&reservation);
    let committed = streaming
        .commit_checkpoint(reservation.clone(), checkpoint)
        .unwrap();
    assert_eq!(
        committed.commit_checkpoint(reservation, replay_checkpoint),
        Err(BackupArchiveError::ArchiveHeadMismatch)
    );
}

#[test]
fn prepared_deletion_pins_its_digest_into_the_signed_archive_state() {
    let (streaming, set, head) = streaming_set();
    let streaming = with_base(streaming, set, head);
    let sealed = streaming
        .begin_sealing(set)
        .unwrap()
        .commit_sealed(set)
        .unwrap();
    let preparation = digest(b"prepared-deletion-manifest");
    let prepared = sealed.prepare_deletion(set, preparation).unwrap();

    assert_eq!(
        prepared.deletion_preparation_digest(set).unwrap(),
        Some(preparation)
    );
    assert!(prepared.is_exact_prepared_deletion_successor_of(&sealed));
    assert!(!prepared.is_exact_lifecycle_successor_of(
        &sealed,
        vestrace_domain::BackupSetLifecycle::Sealed,
        vestrace_domain::BackupSetLifecycle::DeletionPrepared,
    ));
    assert_eq!(
        BackupArchiveStateV1::from_canonical_bytes(&prepared.canonical_bytes()).unwrap(),
        prepared
    );
}

#[test]
fn key_erasure_intent_is_an_exact_signed_successor_of_the_preparation() {
    let (streaming, set, head) = streaming_set();
    let streaming = with_base(streaming, set, head);
    let preparation = digest(b"prepared-key-erasure");
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
        prepared.record_archive_key_erased(set),
        Err(BackupArchiveError::InvalidLifecycleTransition)
    );
    assert!(intent.is_exact_key_erasure_prepared_successor_of(&prepared));
    assert_eq!(
        intent.key_erasure_preparation_digest(set).unwrap(),
        Some(preparation)
    );
    assert_eq!(
        intent
            .record_archive_key_erased(set)
            .unwrap()
            .lifecycle(set)
            .unwrap(),
        vestrace_domain::BackupSetLifecycle::ArchiveKeyErased
    );
    assert_eq!(
        BackupArchiveStateV1::from_canonical_bytes(&intent.canonical_bytes()).unwrap(),
        intent
    );
}

#[test]
fn checkpoint_bytes_bind_set_object_timeline_lsn_and_predecessor_digest() {
    let (streaming, set, head) = streaming_set();
    let streaming = with_base(streaming, set, head);
    let head = streaming.archive_head(set).unwrap();
    let first = streaming
        .reserve_append(set, head, object(set, head, ArchiveObjectKind::WalSegment))
        .unwrap();
    let first_checkpoint = WalArchiveCheckpoint::from_reservation(&first);

    let changed_lsn = ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
        end_lsn: 201,
        ..first.object().to_input()
    })
    .unwrap();
    let changed_lsn = streaming.reserve_append(set, head, changed_lsn).unwrap();
    let changed_lsn_checkpoint = WalArchiveCheckpoint::from_reservation(&changed_lsn);
    assert_ne!(
        first_checkpoint.canonical_bytes(),
        changed_lsn_checkpoint.canonical_bytes()
    );

    let changed_previous = ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
        predecessor_head_digest: digest(b"other-predecessor"),
        ..first.object().to_input()
    })
    .unwrap();
    assert_eq!(
        streaming.reserve_append(set, head, changed_previous),
        Err(BackupArchiveError::ArchiveHeadMismatch)
    );
    assert_ne!(first_checkpoint.digest(), changed_lsn_checkpoint.digest());
}

#[test]
fn archive_witness_state_is_versioned_and_rejects_duplicate_or_trailing_records() {
    let (streaming, set, head) = streaming_set();
    let streaming = with_base(streaming, set, head);
    let bytes = streaming.canonical_bytes();
    assert_eq!(
        BackupArchiveStateV1::from_canonical_bytes(&bytes).unwrap(),
        streaming
    );
    assert!(BackupArchiveStateV1::from_canonical_bytes(&[3]).is_err());
    let mut trailing = bytes;
    trailing.push(0);
    assert!(BackupArchiveStateV1::from_canonical_bytes(&trailing).is_err());
}

#[test]
fn witness_state_carries_the_closed_archive_slot_without_losing_lineage() {
    let generation = DatabaseGenerationId::new();
    let state = WitnessStateV1::genesis(generation);
    let reopened = WitnessStateV1::from_canonical_bytes(&state.canonical_bytes()).unwrap();
    assert_eq!(reopened, state);
    assert!(reopened.backup_archive_state().sets().next().is_none());
}
