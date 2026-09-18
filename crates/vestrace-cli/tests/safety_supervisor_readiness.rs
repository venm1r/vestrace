//! Host-only continuity readiness for the P05 safety supervisor.
//!
//! Readiness answers exactly one question: do the protected host roots and the
//! database agree, right now, about this installation's safety state? It is a
//! read. It never appends a journal entry, advances the witness, or writes a
//! safety row, and it never prints key material.
//!
//! Every refusal below exists because a readiness check that repairs what it
//! finds, or that reports health from one side alone, would be
//! indistinguishable from one that works.

use std::{
    env,
    fmt::Write as _,
    fs,
    path::PathBuf,
    process::{Command, Output},
};

use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use vestrace_domain::{
    DatabaseGenerationId, FingerprintKey, FingerprintKeyId, FingerprintKeyVersion,
    InstallationFingerprintKey, InstallationId, JournalPublicKey, SafetyBootstrapBinding,
    SafetyBootstrapRecord, WitnessPublicKey,
};

#[cfg(windows)]
use vestrace_domain::SafetyBootstrapError;

const INITIALIZER_RECEIPT: &[u8] = b"vestrace-safety-journal-initialized-v1\n";

struct DisposableDatabase {
    bootstrap_url: String,
    supervisor_url: String,
}

impl DisposableDatabase {
    fn from_environment() -> Option<Self> {
        Some(Self {
            bootstrap_url: env::var("VESTRACE_P05_RECOVERY_BOOTSTRAP_DATABASE_URL").ok()?,
            supervisor_url: env::var("VESTRACE_P05_RECOVERY_SUPERVISOR_DATABASE_URL").ok()?,
        })
    }
}

fn temporary_root() -> PathBuf {
    env::temp_dir().join(format!("vestrace-p05-readiness-{}", uuid::Uuid::now_v7()))
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

/// One initialized installation: protected host roots plus the matching
/// database rows. Each test damages exactly one of those two sides.
struct Installation {
    root: PathBuf,
    journal_root: PathBuf,
    witness_root: PathBuf,
    bootstrap_root: PathBuf,
    journal_key: PathBuf,
    installation_id: InstallationId,
}

fn supervisor_command(database: &DisposableDatabase, installation: &Installation) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vestrace"));
    command
        .env(
            "VESTRACE_SAFETY_SUPERVISOR_DATABASE_URL",
            &database.supervisor_url,
        )
        .env("VESTRACE_SAFETY_JOURNAL_ROOT", &installation.journal_root)
        .env("VESTRACE_SAFETY_WITNESS_ROOT", &installation.witness_root)
        .env(
            "VESTRACE_SAFETY_BOOTSTRAP_ROOT",
            &installation.bootstrap_root,
        )
        .env(
            "VESTRACE_SAFETY_JOURNAL_SIGNING_KEY",
            &installation.journal_key,
        )
        .env(
            "VESTRACE_BACKUP_ARCHIVE_ROOT",
            installation.root.join("archive"),
        )
        .env(
            "VESTRACE_BACKUP_ARCHIVE_KEY_ROOT",
            installation.root.join("archive-keys"),
        );
    command
}

fn readiness(database: &DisposableDatabase, installation: &Installation) -> Output {
    supervisor_command(database, installation)
        .args(["safety-supervisor", "readiness"])
        .output()
        .unwrap()
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
    sqlx::query("DELETE FROM public.installation_fingerprint_continuity")
        .execute(pool)
        .await
        .unwrap();
}

/// Builds a healthy installation. Returns `None` when this host cannot satisfy
/// the bootstrap record's durability preflight, which is recorded as blocked
/// rather than silently passed.
async fn initialized(database: &DisposableDatabase, pool: &PgPool) -> Option<Installation> {
    empty_safety_authority(pool).await;

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
        FingerprintKey::from_bytes([37; 32]),
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
            return None;
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
    .execute(pool)
    .await
    .unwrap();

    let installation = Installation {
        root,
        journal_root,
        witness_root,
        bootstrap_root,
        journal_key,
        installation_id,
    };
    let initialized = supervisor_command(database, &installation)
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
        ])
        .output()
        .unwrap();
    assert!(initialized.status.success(), "{initialized:?}");
    Some(installation)
}

async fn cleanup(pool: &PgPool, installation: Installation) {
    empty_safety_authority(pool).await;
    fs::remove_dir_all(installation.root).unwrap();
}

async fn pool(database: &DisposableDatabase) -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect(&database.bootstrap_url)
        .await
        .unwrap()
}

macro_rules! require_database {
    () => {
        match DisposableDatabase::from_environment() {
            Some(database) => database,
            None => {
                eprintln!(
                    "BLOCKED: set VESTRACE_P05_RECOVERY_BOOTSTRAP_DATABASE_URL and \
                     VESTRACE_P05_RECOVERY_SUPERVISOR_DATABASE_URL to one disposable P05 database"
                );
                return;
            }
        }
    };
}

/// Roots are environment-only and must be absolute. A readiness check that
/// defaulted a missing root would report on a directory nobody designated.
#[test]
fn absent_host_roots_refuse_readiness() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["safety-supervisor", "readiness"])
        .env_remove("VESTRACE_SAFETY_JOURNAL_ROOT")
        .env_remove("VESTRACE_SAFETY_WITNESS_ROOT")
        .env_remove("VESTRACE_SAFETY_BOOTSTRAP_ROOT")
        .env_remove("VESTRACE_SAFETY_JOURNAL_SIGNING_KEY")
        .env_remove("VESTRACE_BACKUP_ARCHIVE_ROOT")
        .env_remove("VESTRACE_BACKUP_ARCHIVE_KEY_ROOT")
        .env_remove("VESTRACE_SAFETY_SUPERVISOR_DATABASE_URL")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("VESTRACE_SAFETY_JOURNAL_ROOT"),
        "readiness must name the root it lacks: {stderr}"
    );
}

/// A relative root is refused for the same reason an absent one is: it resolves
/// against whatever directory the supervisor happened to start in.
#[test]
fn a_relative_host_root_refuses_readiness() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["safety-supervisor", "readiness"])
        .env("VESTRACE_SAFETY_JOURNAL_ROOT", "relative/journal")
        .env("VESTRACE_SAFETY_WITNESS_ROOT", "relative/witness")
        .env("VESTRACE_SAFETY_BOOTSTRAP_ROOT", "relative/bootstrap")
        .env(
            "VESTRACE_SAFETY_JOURNAL_SIGNING_KEY",
            "relative/journal.pk8",
        )
        .env("VESTRACE_BACKUP_ARCHIVE_ROOT", "relative/archive")
        .env("VESTRACE_BACKUP_ARCHIVE_KEY_ROOT", "relative/archive-keys")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
}

#[tokio::test]
async fn an_exact_healthy_state_reports_ready() {
    let database = require_database!();
    let pool = pool(&database).await;
    let Some(installation) = initialized(&database, &pool).await else {
        return;
    };

    let output = readiness(&database, &installation);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ready"), "{stdout}");
    assert!(
        stdout.contains(&installation.installation_id.as_uuid().to_string()),
        "readiness must name the installation it checked: {stdout}"
    );

    // A readiness result that prints signing material has disclosed it to
    // every log that captures supervisor output.
    let signing_key = fs::read(&installation.journal_key).unwrap();
    assert!(
        !stdout.contains(&hex(&signing_key)),
        "readiness printed journal signing key material"
    );

    cleanup(&pool, installation).await;
}

/// Readiness is a read. Running it must leave the journal, the witness, and
/// the database exactly as it found them.
#[tokio::test]
async fn readiness_never_mutates_the_authority() {
    let database = require_database!();
    let pool = pool(&database).await;
    let Some(installation) = initialized(&database, &pool).await else {
        return;
    };

    let before = authority_fingerprint(&pool, &installation).await;
    assert!(readiness(&database, &installation).status.success());
    assert!(readiness(&database, &installation).status.success());
    let after = authority_fingerprint(&pool, &installation).await;
    assert_eq!(before, after, "readiness mutated the safety authority");

    cleanup(&pool, installation).await;
}

async fn authority_fingerprint(pool: &PgPool, installation: &Installation) -> (i64, i64, String) {
    let (sequence, events, digest): (i64, i64, String) = sqlx::query_as(
        "SELECT (SELECT witness_sequence FROM public.installation_safety_state), \
                (SELECT count(*) FROM public.installation_safety_journal_events), \
                (SELECT encode(journal_digest, 'hex') FROM public.installation_safety_state)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let entries = fs::read_dir(installation.journal_root.join("entries"))
        .unwrap()
        .count() as i64;
    (sequence + entries, events, digest)
}

/// A journal entry whose bytes no longer match its signed digest breaks the
/// chain the witness receipt points into. Readiness must refuse rather than
/// fall back to the database's own account of the same sequence.
#[tokio::test]
async fn a_corrupt_journal_entry_refuses_readiness() {
    let database = require_database!();
    let pool = pool(&database).await;
    let Some(installation) = initialized(&database, &pool).await else {
        return;
    };
    assert!(readiness(&database, &installation).status.success());

    let entries = installation.journal_root.join("entries");
    let entry = fs::read_dir(&entries)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut bytes = fs::read(&entry).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    fs::write(&entry, bytes).unwrap();

    let output = readiness(&database, &installation);
    assert!(!output.status.success(), "corrupt journal reported ready");

    cleanup(&pool, installation).await;
}

/// A witness record re-signed by a key the bootstrap binding does not name is a
/// forgery, however well formed it is.
#[tokio::test]
async fn a_forged_witness_record_refuses_readiness() {
    let database = require_database!();
    let pool = pool(&database).await;
    let Some(installation) = initialized(&database, &pool).await else {
        return;
    };
    assert!(readiness(&database, &installation).status.success());

    let (_, forged_pk8) = signer();
    fs::write(
        installation
            .witness_root
            .join("installation-safety-witness.pk8"),
        forged_pk8,
    )
    .unwrap();

    let output = readiness(&database, &installation);
    assert!(!output.status.success(), "forged witness reported ready");

    cleanup(&pool, installation).await;
}

/// The bootstrap record binds this installation to one fingerprint key. A
/// database whose continuity proof says otherwise is a different installation.
#[tokio::test]
async fn a_mismatched_fingerprint_refuses_readiness() {
    let database = require_database!();
    let pool = pool(&database).await;
    let Some(installation) = initialized(&database, &pool).await else {
        return;
    };
    assert!(readiness(&database, &installation).status.success());

    sqlx::query(
        "UPDATE public.installation_safety_state \
         SET fingerprint_continuity_proof = decode($1, 'hex')",
    )
    .bind(hex(&[0x5a; 32]))
    .execute(&pool)
    .await
    .unwrap();

    let output = readiness(&database, &installation);
    assert!(
        !output.status.success(),
        "mismatched fingerprint reported ready"
    );

    cleanup(&pool, installation).await;
}

/// A database head behind the witness head is the exact condition a crash
/// between the durable receipt and the guarded commit leaves behind. It is a
/// job for `reconcile`, and readiness must say so rather than pass.
#[tokio::test]
async fn a_stale_database_sequence_and_digest_refuse_readiness() {
    let database = require_database!();
    let pool = pool(&database).await;
    let Some(installation) = initialized(&database, &pool).await else {
        return;
    };
    assert!(readiness(&database, &installation).status.success());

    sqlx::query(
        "UPDATE public.installation_safety_state \
         SET witness_sequence = witness_sequence - 1, \
             journal_digest = decode($1, 'hex')",
    )
    .bind(hex(&[0x11; 32]))
    .execute(&pool)
    .await
    .unwrap();

    let output = readiness(&database, &installation);
    assert!(!output.status.success(), "stale database reported ready");

    cleanup(&pool, installation).await;
}

/// An uninitialized authority is not a failure of the host roots; it is simply
/// not ready. It must still refuse rather than report health.
#[tokio::test]
async fn an_uninitialized_authority_refuses_readiness() {
    let database = require_database!();
    let pool = pool(&database).await;
    let Some(installation) = initialized(&database, &pool).await else {
        return;
    };
    assert!(readiness(&database, &installation).status.success());

    sqlx::query(
        "TRUNCATE public.installation_safety_journal_events, \
                  public.installation_safety_generations, \
                  public.installation_safety_state CASCADE",
    )
    .execute(&pool)
    .await
    .unwrap();

    let output = readiness(&database, &installation);
    assert!(
        !output.status.success(),
        "an empty authority reported ready"
    );

    cleanup(&pool, installation).await;
}
