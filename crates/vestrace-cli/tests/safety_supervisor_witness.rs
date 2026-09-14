use std::{fs, path::Path};

use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use vestrace_application::{InstallationSafetyWitness, SafetyJournal};
use vestrace_domain::{
    DatabaseGenerationId, FingerprintKey, FingerprintKeyId, FingerprintKeyVersion,
    InstallationFingerprintKey, InstallationId, JournalPublicKey, RequestId,
    SafetyBootstrapBinding, SafetyEventKind, SignedJournalEntry, WitnessError, WitnessHead,
    WitnessPublicKey, WitnessStateV1,
};
use vestrace_infrastructure::safety::{FileInstallationSafetyWitness, FileSafetyJournal};

fn signer() -> (Ed25519KeyPair, Vec<u8>) {
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let pair = Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap();
    (pair, document.as_ref().to_vec())
}

fn temporary_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("vestrace-p05-{label}-{}", uuid::Uuid::now_v7()))
}

struct Fixture {
    witness_root: std::path::PathBuf,
    journal_root: std::path::PathBuf,
    witness: FileInstallationSafetyWitness,
    journal: FileSafetyJournal,
    head: WitnessHead,
    entry: SignedJournalEntry,
}

fn fixture() -> Result<Fixture, WitnessError> {
    let witness_root = temporary_root("witness");
    let journal_root = temporary_root("journal");
    fs::create_dir_all(journal_root.join("entries"))
        .map_err(|error| WitnessError::Unavailable(error.to_string()))?;
    fs::write(
        journal_root.join(".initializer-receipt"),
        b"vestrace-safety-journal-initialized-v1\n",
    )
    .map_err(|error| WitnessError::Unavailable(error.to_string()))?;
    let (journal_signer, _) = signer();
    let (witness_signer, witness_pk8) = signer();
    fs::create_dir_all(&witness_root)
        .map_err(|error| WitnessError::Unavailable(error.to_string()))?;
    fs::write(
        witness_root.join("installation-safety-witness.pk8"),
        witness_pk8,
    )
    .map_err(|error| WitnessError::Unavailable(error.to_string()))?;

    let installation = InstallationId::new();
    let fingerprint_key_id = FingerprintKeyId::new();
    let fingerprint = InstallationFingerprintKey::new(
        installation,
        fingerprint_key_id,
        FingerprintKeyVersion::V1,
        FingerprintKey::from_bytes([11; 32]),
    );
    let generation = DatabaseGenerationId::new();
    let binding = SafetyBootstrapBinding::new(
        installation,
        fingerprint_key_id,
        fingerprint.continuity_proof(),
        JournalPublicKey::from_bytes(journal_signer.public_key().as_ref().try_into().unwrap()),
        WitnessPublicKey::from_bytes(witness_signer.public_key().as_ref().try_into().unwrap()),
    );
    let witness =
        FileInstallationSafetyWitness::open_or_create(&witness_root, binding, generation)?;
    let journal = FileSafetyJournal::open(&journal_root)?;
    let head = WitnessHead::genesis(
        installation,
        fingerprint_key_id,
        fingerprint.continuity_proof(),
        generation,
        JournalPublicKey::from_bytes(journal_signer.public_key().as_ref().try_into().unwrap()),
    );
    let entry = SignedJournalEntry::sign(
        &journal_signer,
        vestrace_domain::JournalEntryToSign {
            installation_id: installation,
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
    Ok(Fixture {
        witness_root,
        journal_root,
        witness,
        journal,
        head,
        entry,
    })
}

fn cleanup(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn immutable_journal_precedes_fsynced_witness_and_reopens_exact_head() {
    let fixture = match fixture() {
        Ok(fixture) => fixture,
        #[cfg(windows)]
        Err(WitnessError::Unavailable(_)) => return,
        Err(error) => panic!("fixture failed: {error}"),
    };

    fixture.journal.append(&fixture.entry).await.unwrap();
    assert!(fixture.journal.entry_path(&fixture.entry).is_file());
    fixture.journal.append(&fixture.entry).await.unwrap();
    let receipt = fixture
        .witness
        .compare_and_advance(
            fixture.head.clone(),
            fixture.head.accept_signed(&fixture.entry).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(receipt.sequence(), 1);

    drop(fixture.witness);
    let reopened = FileInstallationSafetyWitness::open(&fixture.witness_root).unwrap();
    assert_eq!(reopened.read_head().await.unwrap().sequence(), 1);
    let stale_advance = fixture.head.accept_signed(&fixture.entry).unwrap();
    assert!(matches!(
        reopened
            .compare_and_advance(fixture.head, stale_advance)
            .await,
        Err(WitnessError::Conflict)
    ));
    cleanup(&fixture.witness_root);
    cleanup(&fixture.journal_root);
}

#[tokio::test]
async fn corrupt_witness_record_fails_closed_on_reopen() {
    let fixture = match fixture() {
        Ok(fixture) => fixture,
        #[cfg(windows)]
        Err(WitnessError::Unavailable(_)) => return,
        Err(error) => panic!("fixture failed: {error}"),
    };
    fs::write(
        fixture
            .witness_root
            .join("installation-safety-witness-v1.cbor"),
        b"truncated",
    )
    .unwrap();
    drop(fixture.witness);
    assert!(matches!(
        FileInstallationSafetyWitness::open(&fixture.witness_root),
        Err(WitnessError::Unavailable(_))
    ));
    cleanup(&fixture.witness_root);
    cleanup(&fixture.journal_root);
}
