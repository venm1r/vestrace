use std::env;

use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};

use vestrace_domain::{
    BackupSetId, DatabaseGenerationId, FingerprintKey, FingerprintKeyId, FingerprintKeyVersion,
    InstallationFingerprintKey, InstallationId, JournalEntryToSign, JournalPublicKey, RequestId,
    RestoreAttemptId, RestoreAttemptProgress, RestoreCutoverError, RestoreHoldId,
    RestoreHoldReleaseReason, RestoreTargetId, RestoreTargetRoots, RestoreTerminalReceipt,
    SafetyBootstrapBinding, SafetyEventKind, SafetyJournalDigest, SignedJournalEntry,
    SourceFreezePoint, TargetActivationPlan, WitnessHead, WitnessPublicKey, WitnessReceipt,
    WitnessStateV1,
};

fn digest(label: &[u8]) -> SafetyJournalDigest {
    SafetyJournalDigest::of(label)
}

#[test]
fn target_root_cannot_be_the_source_or_overlap_it() {
    let source = env::temp_dir().join("vestrace-p05c-source");
    let inside_source = source.join("target");
    let containing_source = source.parent().unwrap().to_owned();
    let masked_containing_source = containing_source.join("masked").join("..");

    for target in [
        &source,
        &inside_source,
        &containing_source,
        &masked_containing_source,
    ] {
        assert_eq!(
            RestoreTargetRoots::new(source.clone(), target.to_owned()),
            Err(RestoreCutoverError::OverlappingRoots),
        );
    }
}

#[test]
fn activation_plan_digest_binds_the_immutable_attempt_target_and_freeze_point() {
    let source_generation = DatabaseGenerationId::new();
    let freeze = SourceFreezePoint::new(source_generation, 1, 0x100, 9).unwrap();
    assert_eq!(
        SourceFreezePoint::new(source_generation, 0, 0x100, 9),
        Err(RestoreCutoverError::InvalidFreezePoint),
    );
    assert_eq!(
        SourceFreezePoint::new(source_generation, 1, 0, 9),
        Err(RestoreCutoverError::InvalidFreezePoint),
    );

    let attempt = RestoreAttemptId::new();
    let target = RestoreTargetId::new();
    let backup_set = BackupSetId::new();
    let plan = TargetActivationPlan::new(
        attempt,
        target,
        backup_set,
        source_generation,
        DatabaseGenerationId::new(),
        freeze,
        digest(b"archive-head"),
    )
    .unwrap();
    let changed = TargetActivationPlan::new(
        attempt,
        target,
        backup_set,
        source_generation,
        DatabaseGenerationId::new(),
        freeze,
        digest(b"archive-head"),
    )
    .unwrap();

    assert_ne!(plan.digest(), changed.digest());
    assert_eq!(
        TargetActivationPlan::from_canonical_bytes(&plan.canonical_bytes()).unwrap(),
        plan,
    );
    let mut trailing = plan.canonical_bytes();
    trailing.push(0);
    assert_eq!(
        TargetActivationPlan::from_canonical_bytes(&trailing),
        Err(RestoreCutoverError::Malformed),
    );
}

#[test]
fn terminal_receipt_requires_an_exact_reason_not_a_bare_refusal_digest() {
    let refusal = digest(b"refused-target-destroyed");
    let receipt = RestoreTerminalReceipt::new(
        RestoreAttemptId::new(),
        RestoreTargetId::new(),
        RestoreHoldReleaseReason::RefusedTargetDestroyed { receipt: refusal },
    );

    assert_eq!(
        receipt.release_reason(),
        &RestoreHoldReleaseReason::RefusedTargetDestroyed { receipt: refusal }
    );
    assert_eq!(
        RestoreTerminalReceipt::from_canonical_bytes(refusal.as_bytes()),
        Err(RestoreCutoverError::Malformed),
    );
}

#[test]
fn witness_state_pins_one_typed_plan_then_one_matching_terminal_receipt() {
    let source_generation = DatabaseGenerationId::new();
    let attempt = RestoreAttemptId::new();
    let target = RestoreTargetId::new();
    let backup_set = BackupSetId::new();
    let target_generation = DatabaseGenerationId::new();
    let freeze = SourceFreezePoint::new(source_generation, 1, 0x200, 12).unwrap();
    let plan = TargetActivationPlan::new(
        attempt,
        target,
        backup_set,
        source_generation,
        target_generation,
        freeze,
        digest(b"archive-head"),
    )
    .unwrap();
    let state = WitnessStateV1::genesis(source_generation)
        .with_restore_attempt_progress(
            RestoreAttemptProgress::prepare(
                attempt,
                target,
                backup_set,
                RestoreHoldId::new(),
                source_generation,
                target_generation,
            )
            .unwrap(),
        )
        .unwrap()
        .with_source_freeze(freeze)
        .unwrap();
    let planned = state.with_target_activation_plan(plan.clone()).unwrap();

    assert_eq!(planned.target_activation_plan(), Some(&plan));
    assert_eq!(
        planned.with_target_activation_plan(plan.clone()),
        Err(vestrace_domain::InstallationSafetyError::InvalidStateTransition),
    );
    let receipt = RestoreTerminalReceipt::new(
        plan.attempt_id(),
        plan.target_id(),
        RestoreHoldReleaseReason::TargetInitializationComplete {
            receipt: digest(b"target-initialized"),
        },
    );
    let terminal = planned
        .with_restore_terminal_receipt(receipt.clone())
        .unwrap();
    assert_eq!(terminal.restore_terminal_receipt(), Some(&receipt));
    assert_eq!(
        terminal.with_restore_terminal_receipt(receipt),
        Err(vestrace_domain::InstallationSafetyError::InvalidStateTransition),
    );
    let _ = SafetyEventKind::TargetActivationPlanned;
    let _ = SafetyEventKind::RestoreTerminalRecorded;
}

#[test]
fn witness_accepts_only_the_exact_activation_plan_successor() {
    let installation_id = InstallationId::new();
    let fingerprint_key_id = FingerprintKeyId::new();
    let fingerprint = InstallationFingerprintKey::new(
        installation_id,
        fingerprint_key_id,
        FingerprintKeyVersion::V1,
        FingerprintKey::from_bytes([3; 32]),
    );
    let source_generation = DatabaseGenerationId::new();
    let journal_document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let journal = Ed25519KeyPair::from_pkcs8(journal_document.as_ref()).unwrap();
    let witness_document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let witness = Ed25519KeyPair::from_pkcs8(witness_document.as_ref()).unwrap();
    let head = WitnessHead::genesis(
        installation_id,
        fingerprint_key_id,
        fingerprint.continuity_proof(),
        source_generation,
        JournalPublicKey::from_bytes(journal.public_key().as_ref().try_into().unwrap()),
    );
    let binding = SafetyBootstrapBinding::new(
        installation_id,
        fingerprint_key_id,
        fingerprint.continuity_proof(),
        JournalPublicKey::from_bytes(journal.public_key().as_ref().try_into().unwrap()),
        WitnessPublicKey::from_bytes(witness.public_key().as_ref().try_into().unwrap()),
    );
    let plan = TargetActivationPlan::new(
        RestoreAttemptId::new(),
        RestoreTargetId::new(),
        BackupSetId::new(),
        source_generation,
        DatabaseGenerationId::new(),
        SourceFreezePoint::new(source_generation, 1, 0x300, 15).unwrap(),
        digest(b"archive-head"),
    )
    .unwrap();
    let prepared_state = head
        .state()
        .with_restore_attempt_progress(
            RestoreAttemptProgress::prepare(
                plan.attempt_id(),
                plan.target_id(),
                plan.backup_set_id(),
                RestoreHoldId::new(),
                plan.source_generation_id(),
                plan.target_generation_id(),
            )
            .unwrap(),
        )
        .unwrap();
    let prepared = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id,
            fingerprint_key_id,
            continuity_proof: fingerprint.continuity_proof(),
            request_id: RequestId::new(),
            sequence: 1,
            previous_digest: head.chain_digest(),
            event_kind: SafetyEventKind::RestoreAttemptPrepared,
            generation_id: source_generation,
            activation_epoch: 0,
            state: prepared_state,
        },
    );
    let prepared_receipt =
        WitnessReceipt::sign(&witness, &head, head.accept_signed(&prepared).unwrap()).unwrap();
    let prepared_head = WitnessHead::from_durable_receipt(&binding, &prepared_receipt).unwrap();
    let frozen_state = prepared_head
        .state()
        .with_source_freeze(plan.source_freeze())
        .unwrap();
    let frozen = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id,
            fingerprint_key_id,
            continuity_proof: fingerprint.continuity_proof(),
            request_id: RequestId::new(),
            sequence: 2,
            previous_digest: prepared_receipt.journal_digest(),
            event_kind: SafetyEventKind::SourceFreezeRecorded,
            generation_id: source_generation,
            activation_epoch: 0,
            state: frozen_state,
        },
    );
    let frozen_receipt = WitnessReceipt::sign(
        &witness,
        &prepared_head,
        prepared_head.accept_signed(&frozen).unwrap(),
    )
    .unwrap();
    let frozen_head = WitnessHead::from_durable_receipt(&binding, &frozen_receipt).unwrap();
    let state = frozen_head
        .state()
        .with_target_activation_plan(plan.clone())
        .unwrap();
    let input = JournalEntryToSign {
        installation_id,
        fingerprint_key_id,
        continuity_proof: fingerprint.continuity_proof(),
        request_id: RequestId::new(),
        sequence: 3,
        previous_digest: frozen_receipt.journal_digest(),
        event_kind: SafetyEventKind::TargetActivationPlanned,
        generation_id: source_generation,
        activation_epoch: 0,
        state: state.clone(),
    };
    let planned = SignedJournalEntry::sign(&journal, input.clone());
    assert!(frozen_head.accept_signed(&planned).is_ok());

    let wrong_event = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            event_kind: SafetyEventKind::InstallationInitialized,
            ..input
        },
    );
    assert!(frozen_head.accept_signed(&wrong_event).is_err());

    let planned_receipt = WitnessReceipt::sign(
        &witness,
        &frozen_head,
        frozen_head.accept_signed(&planned).unwrap(),
    )
    .unwrap();
    let planned_head = WitnessHead::from_durable_receipt(&binding, &planned_receipt).unwrap();
    let target_generation = planned_head
        .state()
        .target_activation_plan()
        .unwrap()
        .target_generation_id();
    let premature_activation = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id,
            fingerprint_key_id,
            continuity_proof: fingerprint.continuity_proof(),
            request_id: RequestId::new(),
            sequence: 4,
            previous_digest: planned_receipt.journal_digest(),
            event_kind: SafetyEventKind::GenerationRegistered,
            generation_id: target_generation,
            activation_epoch: 1,
            state: planned_head
                .state()
                .register_generation(target_generation)
                .unwrap(),
        },
    );
    assert!(planned_head.accept_signed(&premature_activation).is_err());
    let terminal_state = planned_head
        .state()
        .with_restore_terminal_receipt(RestoreTerminalReceipt::new(
            plan.attempt_id(),
            plan.target_id(),
            RestoreHoldReleaseReason::TargetInitializationComplete {
                receipt: digest(b"target-initialized"),
            },
        ))
        .unwrap();
    let terminal = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id,
            fingerprint_key_id,
            continuity_proof: fingerprint.continuity_proof(),
            request_id: RequestId::new(),
            sequence: 4,
            previous_digest: planned_receipt.journal_digest(),
            event_kind: SafetyEventKind::RestoreTerminalRecorded,
            generation_id: source_generation,
            activation_epoch: 0,
            state: terminal_state,
        },
    );
    let terminal_receipt = WitnessReceipt::sign(
        &witness,
        &planned_head,
        planned_head.accept_signed(&terminal).unwrap(),
    )
    .unwrap();
    let terminal_head = WitnessHead::from_durable_receipt(&binding, &terminal_receipt).unwrap();
    let activation = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id,
            fingerprint_key_id,
            continuity_proof: fingerprint.continuity_proof(),
            request_id: RequestId::new(),
            sequence: 5,
            previous_digest: terminal_receipt.journal_digest(),
            event_kind: SafetyEventKind::GenerationRegistered,
            generation_id: target_generation,
            activation_epoch: 1,
            state: terminal_head
                .state()
                .register_generation(target_generation)
                .unwrap(),
        },
    );
    assert!(terminal_head.accept_signed(&activation).is_err());
    let activating_state = terminal_head
        .state()
        .with_target_activation_started()
        .unwrap();
    let activating = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id,
            fingerprint_key_id,
            continuity_proof: fingerprint.continuity_proof(),
            request_id: RequestId::new(),
            sequence: 5,
            previous_digest: terminal_receipt.journal_digest(),
            event_kind: SafetyEventKind::TargetActivating,
            generation_id: source_generation,
            activation_epoch: 0,
            state: activating_state,
        },
    );
    let activating_receipt = WitnessReceipt::sign(
        &witness,
        &terminal_head,
        terminal_head.accept_signed(&activating).unwrap(),
    )
    .unwrap();
    let activating_head = WitnessHead::from_durable_receipt(&binding, &activating_receipt).unwrap();
    let activation = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id,
            fingerprint_key_id,
            continuity_proof: fingerprint.continuity_proof(),
            request_id: RequestId::new(),
            sequence: 6,
            previous_digest: activating_receipt.journal_digest(),
            event_kind: SafetyEventKind::GenerationRegistered,
            generation_id: target_generation,
            activation_epoch: 1,
            state: activating_head
                .state()
                .register_generation(target_generation)
                .unwrap(),
        },
    );
    assert!(activating_head.accept_signed(&activation).is_ok());
    let different_generation = DatabaseGenerationId::new();
    let wrong_target = SignedJournalEntry::sign(
        &journal,
        JournalEntryToSign {
            installation_id,
            fingerprint_key_id,
            continuity_proof: fingerprint.continuity_proof(),
            request_id: RequestId::new(),
            sequence: 6,
            previous_digest: activating_receipt.journal_digest(),
            event_kind: SafetyEventKind::GenerationRegistered,
            generation_id: different_generation,
            activation_epoch: 1,
            state: activating_head
                .state()
                .register_generation(different_generation)
                .unwrap(),
        },
    );
    assert!(activating_head.accept_signed(&wrong_target).is_err());
}

#[test]
fn restore_attempt_progress_is_monotonic_and_binds_the_source_freeze() {
    let source_generation = DatabaseGenerationId::new();
    let target_generation = DatabaseGenerationId::new();
    let prepared = RestoreAttemptProgress::prepare(
        RestoreAttemptId::new(),
        RestoreTargetId::new(),
        BackupSetId::new(),
        RestoreHoldId::new(),
        source_generation,
        target_generation,
    )
    .unwrap();
    let frozen = prepared
        .record_source_freeze(SourceFreezePoint::new(source_generation, 1, 0x400, 18).unwrap())
        .unwrap();

    assert_eq!(prepared.source_freeze(), None);
    assert_eq!(frozen.source_freeze().unwrap().lsn(), 0x400);
    assert_eq!(
        frozen
            .record_source_freeze(SourceFreezePoint::new(source_generation, 1, 0x401, 19).unwrap()),
        Err(RestoreCutoverError::InvalidAttemptTransition),
    );
}
