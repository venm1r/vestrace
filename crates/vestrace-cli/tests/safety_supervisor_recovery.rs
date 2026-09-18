//! Subprocess recovery qualification for the host-only P05 supervisor.

use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use vestrace_application::InstallationSafetyWitness;
use vestrace_domain::{
    DatabaseGenerationId, FingerprintKey, FingerprintKeyId, FingerprintKeyVersion,
    InstallationFingerprintKey, InstallationId, JournalPublicKey, SafetyBootstrapBinding,
    SafetyBootstrapRecord, WitnessPublicKey,
};
use vestrace_infrastructure::safety::{FileInstallationSafetyWitness, FileSafetyJournal};

#[cfg(windows)]
use vestrace_domain::SafetyBootstrapError;

const INITIALIZER_RECEIPT: &[u8] = b"vestrace-safety-journal-initialized-v1\n";

struct DisposableDatabase {
    bootstrap_url: String,
    supervisor_url: String,
}

impl DisposableDatabase {
    fn from_environment() -> Option<Self> {
        let bootstrap_url = env::var("VESTRACE_P05_RECOVERY_BOOTSTRAP_DATABASE_URL").ok()?;
        let supervisor_url = env::var("VESTRACE_P05_RECOVERY_SUPERVISOR_DATABASE_URL").ok()?;
        Some(Self {
            bootstrap_url,
            supervisor_url,
        })
    }
}

fn temporary_root() -> PathBuf {
    env::temp_dir().join(format!("vestrace-p05-recovery-{}", uuid::Uuid::now_v7()))
}

fn signer() -> (Ed25519KeyPair, Vec<u8>) {
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let signer = Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap();
    (signer, document.as_ref().to_vec())
}

fn hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn entries(root: &Path) -> usize {
    fs::read_dir(root.join("entries")).unwrap().count()
}

fn supervisor_command(
    database: &DisposableDatabase,
    journal_root: &Path,
    witness_root: &Path,
    bootstrap_root: &Path,
    journal_key: &Path,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vestrace"));
    let root = bootstrap_root
        .parent()
        .expect("bootstrap root is inside the disposable test root");
    command
        .env(
            "VESTRACE_SAFETY_SUPERVISOR_DATABASE_URL",
            &database.supervisor_url,
        )
        .env("VESTRACE_SAFETY_JOURNAL_ROOT", journal_root)
        .env("VESTRACE_SAFETY_WITNESS_ROOT", witness_root)
        .env("VESTRACE_SAFETY_BOOTSTRAP_ROOT", bootstrap_root)
        .env("VESTRACE_SAFETY_JOURNAL_SIGNING_KEY", journal_key)
        .env("VESTRACE_BACKUP_ARCHIVE_ROOT", root.join("archive"))
        .env(
            "VESTRACE_BACKUP_ARCHIVE_KEY_ROOT",
            root.join("archive-keys"),
        );
    command
}

async fn empty_safety_authority(pool: &PgPool) {
    sqlx::query(
        "TRUNCATE public.installation_safety_journal_events, \
                  public.installation_safety_generations, \
                  public.installation_safety_state CASCADE",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn fault_after_witness_advance_reconciles_the_exact_single_entry() {
    let Some(database) = DisposableDatabase::from_environment() else {
        eprintln!(
            "BLOCKED: set VESTRACE_P05_RECOVERY_BOOTSTRAP_DATABASE_URL and \
             VESTRACE_P05_RECOVERY_SUPERVISOR_DATABASE_URL to one disposable P05 database"
        );
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database.bootstrap_url)
        .await
        .unwrap();
    empty_safety_authority(&pool).await;

    let root = temporary_root();
    let journal_root = root.join("journal");
    let witness_root = root.join("witness");
    let bootstrap_root = root.join("bootstrap");
    let journal_key = root.join("journal.pk8");
    fs::create_dir_all(journal_root.join("entries")).unwrap();
    fs::write(
        journal_root.join(".initializer-receipt"),
        INITIALIZER_RECEIPT,
    )
    .unwrap();
    fs::create_dir_all(&witness_root).unwrap();
    let (journal_signer, journal_pk8) = signer();
    let (witness_signer, witness_pk8) = signer();
    fs::write(&journal_key, journal_pk8).unwrap();
    fs::write(
        witness_root.join("installation-safety-witness.pk8"),
        witness_pk8,
    )
    .unwrap();

    let installation_id = InstallationId::new();
    let fingerprint_key_id = FingerprintKeyId::new();
    let fingerprint = InstallationFingerprintKey::new(
        installation_id,
        fingerprint_key_id,
        FingerprintKeyVersion::V1,
        FingerprintKey::from_bytes([29; 32]),
    );
    let generation = DatabaseGenerationId::new();
    let binding = SafetyBootstrapBinding::new(
        installation_id,
        fingerprint_key_id,
        fingerprint.continuity_proof(),
        JournalPublicKey::from_bytes(journal_signer.public_key().as_ref().try_into().unwrap()),
        WitnessPublicKey::from_bytes(witness_signer.public_key().as_ref().try_into().unwrap()),
    );
    match SafetyBootstrapRecord::open_or_create(&bootstrap_root, &binding) {
        Ok(_) => {}
        #[cfg(windows)]
        Err(SafetyBootstrapError::Unavailable(error)) => {
            eprintln!(
                "BLOCKED: this Windows filesystem cannot flush the bootstrap parent directory: {error}"
            );
            fs::remove_dir_all(root).unwrap();
            return;
        }
        Err(error) => panic!("bootstrap durability preflight failed: {error}"),
    }
    sqlx::query(
        "INSERT INTO public.installation_fingerprint_continuity \
         (installation_id, fingerprint_key_id, fingerprint_key_version, continuity_proof) \
         VALUES ($1, $2, 1, $3)",
    )
    .bind(installation_id.as_uuid())
    .bind(fingerprint_key_id.as_uuid())
    .bind(fingerprint.continuity_proof().as_bytes().as_slice())
    .execute(&pool)
    .await
    .unwrap();
    let fault = supervisor_command(
        &database,
        &journal_root,
        &witness_root,
        &bootstrap_root,
        &journal_key,
    )
    .args([
        "safety-supervisor",
        "initialize",
        "--installation-id",
        &installation_id.as_uuid().to_string(),
        "--fingerprint-key-id",
        &fingerprint_key_id.as_uuid().to_string(),
        "--continuity-proof-hex",
        &hex(fingerprint.continuity_proof().as_bytes()),
        "--generation-id",
        &generation.as_uuid().to_string(),
        "--fault-after-witness-advance",
    ])
    .output()
    .unwrap();
    assert_eq!(fault.status.code(), Some(86), "{fault:?}");

    let journal = FileSafetyJournal::open(&journal_root).unwrap();
    let witness = FileInstallationSafetyWitness::open(&witness_root).unwrap();
    let receipt = witness.durable_receipt().unwrap();
    assert_eq!(receipt.sequence(), 1);
    assert_eq!(witness.read_head().await.unwrap().sequence(), 1);
    assert_eq!(entries(&journal_root), 1);
    journal
        .read_exact(receipt.sequence(), receipt.journal_digest())
        .unwrap();
    let state_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM public.installation_safety_state")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state_count, 0, "fault must precede guarded SQL");

    let reconciled = supervisor_command(
        &database,
        &journal_root,
        &witness_root,
        &bootstrap_root,
        &journal_key,
    )
    .args(["safety-supervisor", "reconcile"])
    .output()
    .unwrap();
    assert!(reconciled.status.success(), "{reconciled:?}");

    let (sequence, events): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT witness_sequence FROM public.installation_safety_state), \
                (SELECT count(*) FROM public.installation_safety_journal_events)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((sequence, events), (1, 1));
    assert_eq!(
        entries(&journal_root),
        1,
        "reconcile must not append a new entry"
    );

    empty_safety_authority(&pool).await;
    drop(pool);
    fs::remove_dir_all(root).unwrap();
}
