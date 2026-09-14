use std::fs;

use vestrace_application::{
    ApplicationError, ArchiveKeyCustody, ArchiveObjectWrite, BackupObjectStore,
    ManagedBackupDeletionPrepared,
};
use vestrace_domain::{
    ArchiveHead, ArchiveObjectDescriptor, ArchiveObjectDescriptorInput, ArchiveObjectKind,
    BackupObjectId, BackupSetId, SafetyJournalDigest,
};
use vestrace_infrastructure::backup_archive::{FileArchiveKeyCustody, FileBackupObjectStore};

#[test]
fn host_archive_adapters_require_distinct_roots() {
    let root = std::env::temp_dir().join(format!("vestrace-archive-{}", uuid::Uuid::now_v7()));
    let Some(_) =
        available_or_windows_parent_sync_blocked(FileBackupObjectStore::open(root.join("objects")))
    else {
        return;
    };
    let Some(_) =
        available_or_windows_parent_sync_blocked(FileArchiveKeyCustody::open(root.join("keys")))
    else {
        return;
    };
}

#[tokio::test]
async fn ciphertext_cannot_open_with_another_set_or_changed_object_metadata() {
    let root = std::env::temp_dir().join(format!("vestrace-archive-aead-{}", uuid::Uuid::now_v7()));
    let Some(custody) =
        available_or_windows_parent_sync_blocked(FileArchiveKeyCustody::open(root.join("keys")))
    else {
        return;
    };
    let set = BackupSetId::new();
    let Some(_) = available_or_windows_parent_sync_blocked(custody.create_set_key(set).await)
    else {
        return;
    };
    let provisional = descriptor(set, 1, SafetyJournalDigest::of(b"pending"));
    let frame = custody
        .seal_object(set, &provisional, b"wal plaintext")
        .unwrap();
    let bound_descriptor = descriptor(set, 1, SafetyJournalDigest::of(&frame));
    assert_eq!(
        &*custody.open_object(set, &bound_descriptor, &frame).unwrap(),
        b"wal plaintext"
    );
    assert!(
        custody
            .open_object(
                set,
                &descriptor(set, 2, SafetyJournalDigest::of(&frame)),
                &frame
            )
            .is_err()
    );
    assert!(
        custody
            .open_object(BackupSetId::new(), &bound_descriptor, &frame)
            .is_err()
    );
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn staged_archive_is_create_only_durable_and_exact_orphan_cleanup_never_targets_final_objects()
 {
    let root =
        std::env::temp_dir().join(format!("vestrace-archive-stage-{}", uuid::Uuid::now_v7()));
    let Some(custody) =
        available_or_windows_parent_sync_blocked(FileArchiveKeyCustody::open(root.join("keys")))
    else {
        return;
    };
    let set = BackupSetId::new();
    let Some(_) = available_or_windows_parent_sync_blocked(custody.create_set_key(set).await)
    else {
        return;
    };
    let provisional = descriptor(set, 1, SafetyJournalDigest::of(b"pending"));
    let frame = custody
        .seal_object(set, &provisional, b"wal plaintext")
        .unwrap();
    let descriptor = descriptor(set, 1, SafetyJournalDigest::of(&frame));
    let Some(store) =
        available_or_windows_parent_sync_blocked(FileBackupObjectStore::open(root.join("objects")))
    else {
        return;
    };
    let Some(staged) = available_or_windows_parent_sync_blocked(
        store
            .stage_encrypted(ArchiveObjectWrite::new(descriptor, frame).unwrap())
            .await,
    ) else {
        return;
    };
    assert_eq!(
        staged.staging_id().as_uuid(),
        staged.descriptor().object_id().as_uuid(),
        "recovery must derive the only eligible staging path from the guarded object identity"
    );
    assert!(store.staging_path(&staged).is_file());
    store
        .remove_cancelled_pending_staging(set, BackupObjectId::new())
        .unwrap();
    assert!(
        store.staging_path(&staged).is_file(),
        "a cancelled intent may not remove a sibling staging object"
    );
    store.verify_durable(&staged).await.unwrap();
    let Some(final_path) =
        available_or_windows_parent_sync_blocked(store.promote_after_checkpoint(&staged))
    else {
        return;
    };
    assert!(final_path.is_file());
    assert!(!store.staging_path(&staged).exists());
    let Some(()) = available_or_windows_parent_sync_blocked(store.remove_orphan(&staged).await)
    else {
        return;
    };
    assert!(final_path.is_file());
    assert!(store.remove_committed(staged.descriptor()).await.is_err());
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn prepared_deletion_removes_only_the_manifest_named_final_object_and_retries_exactly() {
    let root =
        std::env::temp_dir().join(format!("vestrace-archive-delete-{}", uuid::Uuid::now_v7()));
    let Some(custody) =
        available_or_windows_parent_sync_blocked(FileArchiveKeyCustody::open(root.join("keys")))
    else {
        return;
    };
    let set = BackupSetId::new();
    let Some(_) = available_or_windows_parent_sync_blocked(custody.create_set_key(set).await)
    else {
        return;
    };
    let provisional = descriptor(set, 1, SafetyJournalDigest::of(b"pending"));
    let frame = custody
        .seal_object(set, &provisional, b"wal plaintext")
        .unwrap();
    let descriptor = descriptor(set, 1, SafetyJournalDigest::of(&frame));
    let Some(store) =
        available_or_windows_parent_sync_blocked(FileBackupObjectStore::open(root.join("objects")))
    else {
        return;
    };
    let Some(staged) = available_or_windows_parent_sync_blocked(
        store
            .stage_encrypted(ArchiveObjectWrite::new(descriptor.clone(), frame).unwrap())
            .await,
    ) else {
        return;
    };
    let Some(final_path) =
        available_or_windows_parent_sync_blocked(store.promote_after_checkpoint(&staged))
    else {
        return;
    };
    let prepared = ManagedBackupDeletionPrepared::new(set, SafetyJournalDigest::of(b"prepared"));
    let Some(()) = available_or_windows_parent_sync_blocked(
        store.remove_prepared_committed(prepared, &descriptor).await,
    ) else {
        return;
    };
    assert!(!final_path.exists());
    store
        .remove_prepared_committed(prepared, &descriptor)
        .await
        .unwrap();
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn streamed_ciphertext_spool_is_chunked_verified_and_removed_after_exact_staging() {
    let root =
        std::env::temp_dir().join(format!("vestrace-archive-stream-{}", uuid::Uuid::now_v7()));
    let Some(custody) =
        available_or_windows_parent_sync_blocked(FileArchiveKeyCustody::open(root.join("keys")))
    else {
        return;
    };
    let Some(store) =
        available_or_windows_parent_sync_blocked(FileBackupObjectStore::open(root.join("objects")))
    else {
        return;
    };
    let set = BackupSetId::new();
    let Some(_) = available_or_windows_parent_sync_blocked(custody.create_set_key(set).await)
    else {
        return;
    };
    let plaintext: Vec<u8> = (0..(64 * 1024 * 2 + 19))
        .map(|index| (index % 251) as u8)
        .collect();
    let segment = root.join("segment.wal");
    fs::write(&segment, &plaintext).unwrap();
    let (plaintext_digest, plaintext_length) =
        FileArchiveKeyCustody::digest_file(&segment).unwrap();
    let length =
        FileArchiveKeyCustody::sealed_stream_length_for_plaintext(plaintext_length).unwrap();
    let descriptor_input = ArchiveObjectDescriptorInput {
        set_id: set,
        object_id: BackupObjectId::new(),
        kind: ArchiveObjectKind::WalSegment,
        ordinal: 1,
        timeline: 1,
        start_lsn: 8,
        end_lsn: 16,
        ciphertext_digest: SafetyJournalDigest::of(b"pending-ciphertext"),
        plaintext_digest,
        length,
        predecessor_head_digest: ArchiveHead::genesis(SafetyJournalDigest::of(b"head")).digest,
    };
    let provisional = ArchiveObjectDescriptor::new(descriptor_input.clone()).unwrap();
    let spool = store.ciphertext_spool_path(&provisional);
    let ciphertext_digest = custody
        .seal_file_to(set, &provisional, &segment, &spool)
        .unwrap();
    let descriptor = ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
        ciphertext_digest,
        ..descriptor_input
    })
    .unwrap();
    let Some(staged) = available_or_windows_parent_sync_blocked(
        store
            .stage_encrypted(
                ArchiveObjectWrite::from_spool(descriptor.clone(), spool.clone()).unwrap(),
            )
            .await,
    ) else {
        return;
    };
    assert!(
        !spool.exists(),
        "staging owns and removes the exact ciphertext spool"
    );
    store.verify_durable(&staged).await.unwrap();
    let Some(final_path) =
        available_or_windows_parent_sync_blocked(store.promote_after_checkpoint(&staged))
    else {
        return;
    };
    let frame = fs::read(final_path).unwrap();
    assert_eq!(
        custody
            .open_object(set, &descriptor, &frame)
            .unwrap()
            .as_slice(),
        plaintext.as_slice()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn compose_does_not_own_archive_or_key_custody_or_a_supervisor_service() {
    let compose = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docker-compose.yml"
    ))
    .unwrap();
    assert!(!compose.contains("VESTRACE_BACKUP_ARCHIVE_ROOT"));
    assert!(!compose.contains("VESTRACE_BACKUP_ARCHIVE_KEY_ROOT"));
    assert!(!compose.contains("safety-supervisor:"));
}

fn descriptor(
    set: BackupSetId,
    ordinal: u64,
    ciphertext_digest: SafetyJournalDigest,
) -> ArchiveObjectDescriptor {
    ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
        set_id: set,
        object_id: BackupObjectId::new(),
        kind: ArchiveObjectKind::WalSegment,
        ordinal,
        timeline: 1,
        start_lsn: 8,
        end_lsn: 16,
        ciphertext_digest,
        plaintext_digest: SafetyJournalDigest::of(b"wal plaintext"),
        length: 13,
        predecessor_head_digest: ArchiveHead::genesis(SafetyJournalDigest::of(b"head")).digest,
    })
    .unwrap()
}

fn available_or_windows_parent_sync_blocked<T>(result: Result<T, ApplicationError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        #[cfg(windows)]
        Err(ApplicationError::Unavailable(message)) if message.contains("os error 5") => None,
        Err(error) => panic!("unexpected host archive custody failure: {error}"),
    }
}
