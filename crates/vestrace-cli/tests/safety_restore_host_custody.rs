use std::fs;

use vestrace_application::{
    ApplicationError, ArchiveKeyCustody, ArchiveObjectWrite, BackupObjectStore,
};
use vestrace_domain::{
    ArchiveHead, ArchiveObjectDescriptor, ArchiveObjectDescriptorInput, ArchiveObjectKind,
    BackupObjectId, BackupSetId, SafetyJournalDigest,
};
use vestrace_infrastructure::{
    backup_archive::{FileArchiveKeyCustody, FileBackupObjectStore},
    restore_target::{RestoreTargetCustody, RestoreTargetError, RestoreTool},
};

#[test]
fn fresh_restore_target_is_disjoint_from_every_supervisor_custody_root() {
    let root =
        std::env::temp_dir().join(format!("vestrace-restore-custody-{}", uuid::Uuid::now_v7()));
    let source = root.join("source");
    let archive = root.join("archive");
    let keys = root.join("keys");
    let journal = root.join("journal");
    for path in [&source, &archive, &keys, &journal] {
        fs::create_dir_all(path).unwrap();
    }
    let custody = RestoreTargetCustody::new([
        source.clone(),
        archive.clone(),
        keys.clone(),
        journal.clone(),
    ])
    .unwrap();

    for target in [source.join("target"), archive.join("target"), root.clone()] {
        assert_eq!(
            custody.prepare_fresh(target),
            Err(RestoreTargetError::OverlappingRoots)
        );
    }
    let target = custody.prepare_fresh(root.join("fresh-target")).unwrap();
    assert!(target.is_dir());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn target_tool_never_receives_a_protected_source_argument() {
    let root = std::env::temp_dir().join(format!("vestrace-restore-tool-{}", uuid::Uuid::now_v7()));
    let source = root.join("source");
    let target = root.join("target");
    let tool_path = root.join("restore-tool");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&target).unwrap();
    fs::write(&tool_path, b"unused").unwrap();
    let custody = RestoreTargetCustody::new([source.clone()]).unwrap();
    let tool = RestoreTool::new(tool_path).unwrap();
    assert_eq!(
        custody.run_tool(&tool, &target, [source]),
        Err(RestoreTargetError::OverlappingRoots)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn target_tool_receives_only_explicit_target_contract_arguments() {
    let root = std::env::temp_dir().join(format!(
        "vestrace-restore-tool-contract-{}",
        uuid::Uuid::now_v7()
    ));
    let source = root.join("active-source");
    let target = root.join("fresh-target");
    let input = target.join("restore-input");
    let receipt = target.join(".vestrace-target-initialized-v1");
    let captured = root.join("captured-arguments.txt");
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(&source).unwrap();

    let tool_path = fake_restore_tool(&root, &captured);
    let custody = RestoreTargetCustody::new([source.clone()]).unwrap();
    let tool = RestoreTool::new(tool_path).unwrap();
    custody
        .run_tool(
            &tool,
            &target,
            [
                input.clone().into_os_string(),
                "--target-receipt".into(),
                receipt.clone().into_os_string(),
                "--plan-digest".into(),
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
                "--freeze-timeline".into(),
                "1".into(),
                "--freeze-lsn".into(),
                "256".into(),
            ],
        )
        .unwrap();

    let captured = fs::read_to_string(&captured).unwrap();
    for expected in [
        input.display().to_string(),
        "--target-receipt".to_owned(),
        receipt.display().to_string(),
        "--plan-digest".to_owned(),
        "--freeze-timeline".to_owned(),
        "--freeze-lsn".to_owned(),
        "--pgdata".to_owned(),
        target.display().to_string(),
    ] {
        assert!(
            captured.contains(&expected),
            "missing {expected:?} in {captured:?}"
        );
    }
    assert!(!captured.contains(&source.display().to_string()));
    let _ = fs::remove_dir_all(root);
}

fn fake_restore_tool(root: &std::path::Path, captured: &std::path::Path) -> std::path::PathBuf {
    #[cfg(windows)]
    {
        let tool = root.join("fake-restore.cmd");
        fs::write(
            &tool,
            format!("@echo off\r\necho %* > \"{}\"\r\n", captured.display()),
        )
        .unwrap();
        tool
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let tool = root.join("fake-restore.sh");
        fs::write(
            &tool,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
                captured.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
        tool
    }
}

#[tokio::test]
async fn materialization_opens_only_verified_descriptor_bound_archive_members() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!(
            "vestrace-restore-materialize-{}",
            uuid::Uuid::now_v7()
        ));
    let archive_root = root.join("archive");
    let key_root = root.join("keys");
    let source = root.join("source");
    fs::create_dir_all(&source).unwrap();
    let Some(store) =
        available_or_windows_parent_sync_blocked(FileBackupObjectStore::open(archive_root.clone()))
    else {
        return;
    };
    let Some(custody) =
        available_or_windows_parent_sync_blocked(FileArchiveKeyCustody::open(key_root.clone()))
    else {
        return;
    };
    let set = BackupSetId::new();
    let Some(_) = available_or_windows_parent_sync_blocked(custody.create_set_key(set).await)
    else {
        return;
    };
    let plaintext = b"base archive member";
    let segment = root.join("base.tar");
    fs::write(&segment, plaintext).unwrap();
    let (plaintext_digest, plaintext_length) =
        FileArchiveKeyCustody::digest_file(&segment).unwrap();
    let descriptor_input = ArchiveObjectDescriptorInput {
        set_id: set,
        object_id: BackupObjectId::new(),
        kind: ArchiveObjectKind::BaseChunk,
        ordinal: 1,
        timeline: 1,
        start_lsn: 8,
        end_lsn: 8,
        ciphertext_digest: SafetyJournalDigest::of(b"pending"),
        plaintext_digest,
        length: FileArchiveKeyCustody::sealed_stream_length_for_plaintext(plaintext_length)
            .unwrap(),
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
            .stage_encrypted(ArchiveObjectWrite::from_spool(descriptor.clone(), spool).unwrap())
            .await,
    ) else {
        return;
    };
    let Some(()) = available_or_windows_parent_sync_blocked(store.verify_durable(&staged).await)
    else {
        return;
    };
    let Some(_) = available_or_windows_parent_sync_blocked(store.promote_after_checkpoint(&staged))
    else {
        return;
    };

    let target_custody = RestoreTargetCustody::new([source, archive_root, key_root]).unwrap();
    let target = target_custody.prepare_fresh(root.join("target")).unwrap();
    let files = target_custody
        .materialize_manifest(&target, set, &[descriptor], &store, &custody)
        .unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(fs::read(&files[0]).unwrap(), plaintext);
    let _ = fs::remove_dir_all(root);
}

fn available_or_windows_parent_sync_blocked<T>(result: Result<T, ApplicationError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        #[cfg(windows)]
        Err(ApplicationError::Unavailable(message)) if message.contains("os error 5") => None,
        Err(error) => panic!("unexpected restore custody failure: {error}"),
    }
}
