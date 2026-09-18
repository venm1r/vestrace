use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use sha2::{Digest, Sha256};
use vestrace_domain::{
    ArchiveHead, ArchiveObjectDescriptor, ArchiveObjectDescriptorInput, ArchiveObjectKind,
    BackupArchiveStateV1, BackupObjectId, BackupSetId, BackupSetIdentity, DatabaseGenerationId,
    FingerprintKey, FingerprintKeyId, FingerprintKeyVersion, InstallationFingerprintKey,
    InstallationId, JournalEntryToSign, JournalPublicKey, RequestId, RestoreHoldId,
    SafetyBootstrapBinding, SafetyBootstrapError, SafetyBootstrapRecord, SafetyEventKind,
    SafetyJournalDigest, SignedJournalEntry, WalArchiveCheckpoint, WitnessHead, WitnessPublicKey,
    WitnessReceipt, WitnessStateV1,
};

fn signer() -> Ed25519KeyPair {
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap()
}

fn fixture() -> (WitnessHead, SignedJournalEntry, Ed25519KeyPair) {
    let installation_id = InstallationId::new();
    let fingerprint_key_id = FingerprintKeyId::new();
    let fingerprint = InstallationFingerprintKey::new(
        installation_id,
        fingerprint_key_id,
        FingerprintKeyVersion::V1,
        FingerprintKey::from_bytes([7; 32]),
    );
    let generation = DatabaseGenerationId::new();
    let journal = signer();
    let head = WitnessHead::genesis(
        installation_id,
        fingerprint_key_id,
        fingerprint.continuity_proof(),
        generation,
        JournalPublicKey::from_bytes(journal.public_key().as_ref().try_into().unwrap()),
    );
    let entry = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id,
            fingerprint_key_id,
            continuity_proof: fingerprint.continuity_proof(),
            request_id: RequestId::new(),
            sequence: 1,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::InstallationInitialized,
            generation_id: generation,
            activation_epoch: 0,
            state: WitnessStateV1::genesis(generation),
        },
    );
    (head, entry, journal)
}

fn base_checkpoint(
    state: &BackupArchiveStateV1,
    set: BackupSetId,
    head: ArchiveHead,
) -> BackupArchiveStateV1 {
    let reservation = state
        .reserve_append(
            set,
            head,
            ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
                set_id: set,
                object_id: BackupObjectId::new(),
                kind: ArchiveObjectKind::BaseChunk,
                ordinal: head.checkpoint_ordinal + 1,
                timeline: 1,
                start_lsn: 8,
                end_lsn: 16,
                ciphertext_digest: SafetyJournalDigest::of(b"base-ciphertext"),
                plaintext_digest: SafetyJournalDigest::of(b"base-plaintext"),
                length: 10,
                predecessor_head_digest: head.digest,
            })
            .unwrap(),
        )
        .unwrap();
    state
        .commit_checkpoint(
            reservation.clone(),
            WalArchiveCheckpoint::from_reservation(&reservation),
        )
        .unwrap()
}

#[test]
fn signed_entry_requires_next_sequence_same_identity_and_valid_signature() {
    let (head, entry, journal) = fixture();
    assert!(head.accept_signed(&entry).is_ok());
    assert_eq!(entry.signature().len(), 64);

    let wrong_sequence = SignedJournalEntry::sign(
        &signer(),
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 9,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::InstallationInitialized,
            generation_id: entry.generation_id(),
            activation_epoch: entry.activation_epoch(),
            state: entry.state().clone(),
        },
    );
    assert!(head.accept_signed(&wrong_sequence).is_err());

    let foreign_key_same_sequence = SignedJournalEntry::sign(
        &signer(),
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 1,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::InstallationInitialized,
            generation_id: entry.generation_id(),
            activation_epoch: entry.activation_epoch(),
            state: entry.state().clone(),
        },
    );
    assert!(head.accept_signed(&foreign_key_same_sequence).is_err());

    let different_request = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 1,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::InstallationInitialized,
            generation_id: entry.generation_id(),
            activation_epoch: entry.activation_epoch(),
            state: entry.state().clone(),
        },
    );
    assert_ne!(entry.digest(), different_request.digest());

    let next_generation = DatabaseGenerationId::new();
    let rewritten_lineage = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 1,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::GenerationRegistered,
            generation_id: next_generation,
            activation_epoch: 1,
            state: WitnessStateV1::genesis(DatabaseGenerationId::new()),
        },
    );
    assert!(head.accept_signed(&rewritten_lineage).is_err());

    let registered = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 1,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::GenerationRegistered,
            generation_id: next_generation,
            activation_epoch: 1,
            state: head.state().register_generation(next_generation).unwrap(),
        },
    );
    assert!(head.accept_signed(&registered).is_ok());
}

#[test]
fn journal_digest_covers_the_canonical_payload_not_the_detached_signature() {
    let (_, entry, _) = fixture();
    let canonical = entry.canonical_bytes();
    assert_eq!(entry.digest().as_bytes(), &Sha256::digest(&canonical)[..]);
    let mut incorrect_with_signature = canonical;
    incorrect_with_signature.extend_from_slice(&(entry.signature().len() as u64).to_be_bytes());
    incorrect_with_signature.extend_from_slice(entry.signature());
    assert_ne!(
        entry.digest().as_bytes(),
        &Sha256::digest(incorrect_with_signature)[..]
    );
}

#[test]
fn archive_checkpoint_state_requires_its_own_event_exact_successor_and_next_sequence() {
    let (head, entry, journal) = fixture();
    let set = BackupSetId::new();
    let archive_head = ArchiveHead::genesis(SafetyJournalDigest::of(b"archive-genesis"));
    let streaming = BackupArchiveStateV1::empty()
        .start_streaming(
            BackupSetIdentity::new(set, entry.installation_id(), entry.generation_id()),
            archive_head,
        )
        .unwrap();
    let descriptor = ArchiveObjectDescriptor::new(ArchiveObjectDescriptorInput {
        set_id: set,
        object_id: BackupObjectId::new(),
        kind: ArchiveObjectKind::BaseChunk,
        ordinal: 1,
        timeline: 1,
        start_lsn: 8,
        end_lsn: 16,
        ciphertext_digest: SafetyJournalDigest::of(b"ciphertext"),
        plaintext_digest: SafetyJournalDigest::of(b"plaintext"),
        length: 10,
        predecessor_head_digest: archive_head.digest,
    })
    .unwrap();
    let reservation = streaming
        .reserve_append(set, archive_head, descriptor)
        .unwrap();
    let changed_state = WitnessStateV1::genesis(entry.generation_id()).with_backup_archive_state(
        streaming
            .commit_checkpoint(
                reservation.clone(),
                WalArchiveCheckpoint::from_reservation(&reservation),
            )
            .unwrap(),
    );

    let old_event = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 1,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::InstallationInitialized,
            generation_id: entry.generation_id(),
            activation_epoch: entry.activation_epoch(),
            state: changed_state.clone(),
        },
    );
    assert!(head.accept_signed(&old_event).is_err());

    let old_generation_event = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 1,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::GenerationRegistered,
            generation_id: DatabaseGenerationId::new(),
            activation_epoch: 1,
            state: changed_state.clone(),
        },
    );
    assert!(head.accept_signed(&old_generation_event).is_err());

    let wrong_predecessor = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 1,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::ArchiveCheckpointCommitted,
            generation_id: entry.generation_id(),
            activation_epoch: entry.activation_epoch(),
            state: changed_state.clone(),
        },
    );
    assert!(head.accept_signed(&wrong_predecessor).is_err());

    let wrong_sequence = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 2,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::ArchiveCheckpointCommitted,
            generation_id: entry.generation_id(),
            activation_epoch: entry.activation_epoch(),
            state: changed_state,
        },
    );
    assert!(head.accept_signed(&wrong_sequence).is_err());
}

#[test]
fn archive_start_and_restore_hold_require_their_exact_signed_successors() {
    let (head, entry, journal) = fixture();
    let witness = signer();
    let binding = SafetyBootstrapBinding::new(
        head.installation_id(),
        head.fingerprint_key_id(),
        head.continuity_proof().clone(),
        head.journal_public_key().clone(),
        WitnessPublicKey::from_bytes(witness.public_key().as_ref().try_into().unwrap()),
    );
    let set = BackupSetId::new();
    let started_state = WitnessStateV1::genesis(entry.generation_id()).with_backup_archive_state(
        BackupArchiveStateV1::empty()
            .start_streaming(
                BackupSetIdentity::new(set, entry.installation_id(), entry.generation_id()),
                ArchiveHead::genesis(SafetyJournalDigest::of(b"start")),
            )
            .unwrap(),
    );
    let started = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 1,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::BackupSetStarted,
            generation_id: entry.generation_id(),
            activation_epoch: entry.activation_epoch(),
            state: started_state.clone(),
        },
    );
    let receipt =
        WitnessReceipt::sign(&witness, &head, head.accept_signed(&started).unwrap()).unwrap();
    let advanced = WitnessHead::from_durable_receipt(&binding, &receipt).unwrap();
    let base_state = started_state.with_backup_archive_state(base_checkpoint(
        started_state.backup_archive_state(),
        set,
        started_state
            .backup_archive_state()
            .archive_head(set)
            .unwrap(),
    ));
    let base = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 2,
            previous_digest: receipt.journal_digest(),
            event_kind: SafetyEventKind::ArchiveCheckpointCommitted,
            generation_id: entry.generation_id(),
            activation_epoch: entry.activation_epoch(),
            state: base_state.clone(),
        },
    );
    let base_receipt =
        WitnessReceipt::sign(&witness, &advanced, advanced.accept_signed(&base).unwrap()).unwrap();
    let advanced_base = WitnessHead::from_durable_receipt(&binding, &base_receipt).unwrap();
    let held_state = base_state.with_backup_archive_state(
        base_state
            .backup_archive_state()
            .acquire_restore_hold(set, RestoreHoldId::new())
            .unwrap(),
    );
    let held = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 3,
            previous_digest: base_receipt.journal_digest(),
            event_kind: SafetyEventKind::ArchiveRestoreHoldAcquired,
            generation_id: entry.generation_id(),
            activation_epoch: entry.activation_epoch(),
            state: held_state.clone(),
        },
    );
    assert!(advanced_base.accept_signed(&held).is_ok());

    let wrong_kind = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id: entry.installation_id(),
            fingerprint_key_id: entry.fingerprint_key_id(),
            continuity_proof: entry.continuity_proof().clone(),
            request_id: RequestId::new(),
            sequence: 3,
            previous_digest: base_receipt.journal_digest(),
            event_kind: SafetyEventKind::ArchiveSealingStarted,
            generation_id: entry.generation_id(),
            activation_epoch: entry.activation_epoch(),
            state: held_state,
        },
    );
    assert!(advanced_base.accept_signed(&wrong_kind).is_err());
}

#[test]
fn receipt_requires_its_pinned_witness_public_key() {
    let (head, entry, _) = fixture();
    let witness = signer();
    let receipt =
        WitnessReceipt::sign(&witness, &head, head.accept_signed(&entry).unwrap()).unwrap();
    assert!(receipt.verify_against(receipt.witness_public_key()).is_ok());
    assert_eq!(receipt.signature().len(), 64);
    assert_eq!(receipt.generation_id(), entry.generation_id());
    assert_eq!(receipt.state(), entry.state());
    let other = signer();
    let other_key = vestrace_domain::WitnessPublicKey::from_bytes(
        other.public_key().as_ref().try_into().unwrap(),
    );
    assert!(receipt.verify_against(&other_key).is_err());
}

#[test]
fn durable_canonical_records_reconstruct_only_after_signature_and_binding_checks() {
    let (head, entry, _) = fixture();
    let witness = signer();
    let receipt =
        WitnessReceipt::sign(&witness, &head, head.accept_signed(&entry).unwrap()).unwrap();
    let binding = SafetyBootstrapBinding::new(
        head.installation_id(),
        head.fingerprint_key_id(),
        head.continuity_proof().clone(),
        head.journal_public_key().clone(),
        receipt.witness_public_key().clone(),
    );

    let reopened_entry = SignedJournalEntry::from_canonical_bytes_and_signature(
        &entry.canonical_bytes(),
        *entry.signature(),
    )
    .unwrap();
    assert_eq!(reopened_entry.digest(), entry.digest());
    let reopened_receipt = WitnessReceipt::from_canonical_bytes_and_signature(
        &receipt.canonical_bytes(),
        *receipt.signature(),
    )
    .unwrap();
    let reopened_head = WitnessHead::from_durable_receipt(&binding, &reopened_receipt).unwrap();
    assert_eq!(reopened_head.sequence(), receipt.sequence());
    assert_eq!(reopened_head.chain_digest(), receipt.journal_digest());

    let mut corrupted_signature = *entry.signature();
    corrupted_signature[0] ^= 1;
    assert!(
        SignedJournalEntry::from_canonical_bytes_and_signature(
            &entry.canonical_bytes(),
            corrupted_signature,
        )
        .is_err()
    );
    let mut truncated = receipt.canonical_bytes();
    truncated.pop();
    assert!(
        WitnessReceipt::from_canonical_bytes_and_signature(&truncated, *receipt.signature())
            .is_err()
    );
}

#[test]
fn bootstrap_record_is_create_only_fsynced_and_requires_exact_later_binding() {
    let installation_id = InstallationId::new();
    let fingerprint_key_id = FingerprintKeyId::new();
    let fingerprint = InstallationFingerprintKey::new(
        installation_id,
        fingerprint_key_id,
        FingerprintKeyVersion::V1,
        FingerprintKey::from_bytes([3; 32]),
    );
    let journal = signer();
    let witness = signer();
    let binding = SafetyBootstrapBinding::new(
        installation_id,
        fingerprint_key_id,
        fingerprint.continuity_proof(),
        JournalPublicKey::from_bytes(journal.public_key().as_ref().try_into().unwrap()),
        WitnessPublicKey::from_bytes(witness.public_key().as_ref().try_into().unwrap()),
    );
    let root = std::env::temp_dir().join(format!("vestrace-p05-{}", uuid::Uuid::now_v7()));

    let first = SafetyBootstrapRecord::open_or_create(&root, &binding);
    #[cfg(windows)]
    if matches!(&first, Err(SafetyBootstrapError::Unavailable(_))) {
        // This filesystem cannot flush a directory handle. The implementation
        // must fail closed rather than claim the required parent durability.
        std::fs::remove_dir_all(root).unwrap();
        return;
    }
    let first = first.unwrap();
    assert_eq!(first.digest(), binding.digest());
    let reopened = SafetyBootstrapRecord::open_or_create(&root, &binding).unwrap();
    assert_eq!(first.digest(), reopened.digest());
    assert!(SafetyBootstrapRecord::path(&root).is_file());

    let other = SafetyBootstrapBinding::new(
        installation_id,
        fingerprint_key_id,
        fingerprint.continuity_proof(),
        JournalPublicKey::from_bytes([9; 32]),
        binding.witness_public_key().clone(),
    );
    assert!(matches!(
        SafetyBootstrapRecord::open_or_create(&root, &other),
        Err(SafetyBootstrapError::BindingMismatch)
    ));
    std::fs::remove_dir_all(root).unwrap();
}
