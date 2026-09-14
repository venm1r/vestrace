//! Process-level recovery boundaries for P05-C restore commands.

use std::{env, process::Command};

#[cfg(unix)]
use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Output,
};

#[cfg(unix)]
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
#[cfg(unix)]
use sqlx::{PgPool, postgres::PgPoolOptions};
#[cfg(unix)]
use vestrace_domain::{
    DatabaseGenerationId, FingerprintKey, FingerprintKeyId, FingerprintKeyVersion,
    InstallationFingerprintKey, InstallationId, JournalPublicKey, SafetyBootstrapBinding,
    SafetyBootstrapRecord, WitnessPublicKey,
};

#[test]
fn restore_cli_exposes_only_terminal_activation_and_no_generic_hold_release() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["safety-supervisor", "restore", "--help"])
        .output()
        .expect("restore help starts");

    assert!(output.status.success(), "{output:?}");
    let help = String::from_utf8_lossy(&output.stdout);
    for command in [
        "prepare",
        "freeze-source",
        "materialize",
        "plan-activation",
        "activate",
        "refuse",
    ] {
        assert!(help.contains(command), "missing {command}: {help}");
    }
    assert!(!help.contains("release-hold"));
}

#[test]
fn freeze_and_activate_do_not_accept_operator_supplied_receipt_values() {
    let freeze = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["safety-supervisor", "restore", "freeze-source", "--help"])
        .output()
        .expect("freeze-source help starts");
    assert!(freeze.status.success(), "{freeze:?}");
    let freeze_help = String::from_utf8_lossy(&freeze.stdout);
    assert!(!freeze_help.contains("timeline"));
    assert!(!freeze_help.contains("watermark"));

    let activate = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["safety-supervisor", "restore", "activate", "--help"])
        .output()
        .expect("activate help starts");
    assert!(activate.status.success(), "{activate:?}");
    let activate_help = String::from_utf8_lossy(&activate.stdout);
    assert!(activate_help.contains("--target-root"));
    assert!(!activate_help.contains("--receipt"));
}

#[cfg(unix)]
const INITIALIZER_RECEIPT: &[u8] = b"vestrace-safety-journal-initialized-v1\n";

#[cfg(unix)]
struct DisposableDatabase {
    bootstrap_url: String,
    supervisor_url: String,
}

#[cfg(unix)]
impl DisposableDatabase {
    fn from_environment() -> Option<Self> {
        Some(Self {
            bootstrap_url: env::var("VESTRACE_P05_RECOVERY_BOOTSTRAP_DATABASE_URL").ok()?,
            supervisor_url: env::var("VESTRACE_P05_RECOVERY_SUPERVISOR_DATABASE_URL").ok()?,
        })
    }
}

#[cfg(unix)]
fn temporary_root() -> PathBuf {
    env::temp_dir().join(format!(
        "vestrace-p05-restore-recovery-{}",
        uuid::Uuid::now_v7()
    ))
}

#[cfg(unix)]
fn signer() -> (Ed25519KeyPair, Vec<u8>) {
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let signer = Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap();
    (signer, document.as_ref().to_vec())
}

#[cfg(unix)]
fn hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}

#[cfg(unix)]
fn supervisor_command(
    database: &DisposableDatabase,
    journal_root: &Path,
    witness_root: &Path,
    bootstrap_root: &Path,
    journal_key: &Path,
) -> Command {
    let root = bootstrap_root.parent().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_vestrace"));
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

#[cfg(unix)]
fn successful(output: Output, action: &str) {
    assert!(
        output.status.success(),
        "{action} failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[cfg(unix)]
fn started_set(output: Output) -> uuid::Uuid {
    assert!(
        output.status.success(),
        "backup begin failed: status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    uuid::Uuid::parse_str(
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .strip_prefix("managed backup started: ")
            .expect("backup begin reports its generated set ID"),
    )
    .expect("reported backup set ID is a UUID")
}

#[cfg(unix)]
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

#[cfg(unix)]
fn fake_target_restore(root: &Path, runs: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let tool = root.join("target-restore");
    fs::write(
        &tool,
        format!(
            concat!(
                "#!/bin/sh\n",
                "while [ \"$#\" -gt 0 ]; do\n",
                "  case \"$1\" in\n",
                "    --target-receipt) receipt=\"$2\"; shift ;;\n",
                "    --plan-digest) digest=\"$2\"; shift ;;\n",
                "    --freeze-timeline) timeline=\"$2\"; shift ;;\n",
                "    --freeze-lsn) lsn=\"$2\"; shift ;;\n",
                "  esac\n",
                "  shift\n",
                "done\n",
                "printf 'plan_digest=%s\\ntimeline=%s\\nfinal_lsn=%s\\n' \"$digest\" \"$timeline\" \"$lsn\" > \"$receipt\"\n",
                "printf x >> '{}'\n"
            ),
            runs.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
    tool
}

#[cfg(unix)]
async fn empty_restore_authority(pool: &PgPool) {
    sqlx::query(
        "TRUNCATE public.installation_safety_journal_events, \
                  public.installation_safety_generations, \
                  public.installation_safety_state, \
                  public.installation_fingerprint_continuity CASCADE",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Exercises three supervisor process deaths after durable P05-C boundaries.
/// Every restarted command must consume the same signed plan and release only
/// the hold bound to its terminal target receipt.
#[cfg(unix)]
#[tokio::test]
async fn restore_crashes_recover_the_same_encrypted_manifest_and_single_target_activation() {
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
    empty_restore_authority(&pool).await;

    let root = temporary_root();
    let journal_root = root.join("journal");
    let witness_root = root.join("witness");
    let bootstrap_root = root.join("bootstrap");
    let journal_key = root.join("signing").join("journal.pk8");
    let source_root = root.join("source");
    let target_root = root.join("target");
    let base = root.join("base.tar");
    let wal = root.join("000000010000000000000001.wal");
    let restore_runs = root.join("target-restore-runs");
    fs::create_dir_all(journal_root.join("entries")).unwrap();
    fs::create_dir_all(journal_key.parent().unwrap()).unwrap();
    fs::write(
        journal_root.join(".initializer-receipt"),
        INITIALIZER_RECEIPT,
    )
    .unwrap();
    fs::create_dir_all(&witness_root).unwrap();
    fs::create_dir_all(&source_root).unwrap();
    fs::write(&base, vec![0x42; 64 * 1024 + 19]).unwrap();
    fs::write(&wal, vec![0x5a; 64 * 1024 + 37]).unwrap();
    let source_tool = fake_source_quiescer(&root);
    let target_tool = fake_target_restore(&root, &restore_runs);
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
        FingerprintKey::from_bytes([47; 32]),
    );
    let source_generation = DatabaseGenerationId::new();
    let target_generation = DatabaseGenerationId::new();
    let binding = SafetyBootstrapBinding::new(
        installation_id,
        fingerprint_key_id,
        fingerprint.continuity_proof(),
        JournalPublicKey::from_bytes(journal_signer.public_key().as_ref().try_into().unwrap()),
        WitnessPublicKey::from_bytes(witness_signer.public_key().as_ref().try_into().unwrap()),
    );
    SafetyBootstrapRecord::open_or_create(&bootstrap_root, &binding).unwrap();
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

    successful(
        supervisor_command(
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
            &source_generation.as_uuid().to_string(),
        ])
        .output()
        .unwrap(),
        "initialize",
    );
    let set = started_set(
        supervisor_command(
            &database,
            &journal_root,
            &witness_root,
            &bootstrap_root,
            &journal_key,
        )
        .args(["safety-supervisor", "backup", "begin"])
        .output()
        .unwrap(),
    );
    for (action, segment, start_lsn, end_lsn) in [
        ("append-base", &base, "0", "1"),
        ("append-wal", &wal, "1", "256"),
    ] {
        successful(
            supervisor_command(
                &database,
                &journal_root,
                &witness_root,
                &bootstrap_root,
                &journal_key,
            )
            .args([
                "safety-supervisor",
                "backup",
                action,
                "--set-id",
                &set.to_string(),
                "--segment",
                segment.to_str().unwrap(),
                "--timeline",
                "1",
                "--start-lsn",
                start_lsn,
                "--end-lsn",
                end_lsn,
            ])
            .output()
            .unwrap(),
            action,
        );
    }
    let hold = uuid::Uuid::now_v7();
    successful(
        supervisor_command(
            &database,
            &journal_root,
            &witness_root,
            &bootstrap_root,
            &journal_key,
        )
        .args([
            "safety-supervisor",
            "backup",
            "acquire-hold",
            "--set-id",
            &set.to_string(),
            "--hold-id",
            &hold.to_string(),
        ])
        .output()
        .unwrap(),
        "acquire restore hold",
    );

    let attempt = uuid::Uuid::now_v7();
    let target_id = uuid::Uuid::now_v7();
    successful(
        supervisor_command(
            &database,
            &journal_root,
            &witness_root,
            &bootstrap_root,
            &journal_key,
        )
        .env("VESTRACE_RESTORE_SOURCE_ROOT", &source_root)
        .args([
            "safety-supervisor",
            "restore",
            "prepare",
            "--attempt-id",
            &attempt.to_string(),
            "--backup-set-id",
            &set.to_string(),
            "--hold-id",
            &hold.to_string(),
            "--target-id",
            &target_id.to_string(),
            "--target-generation-id",
            &target_generation.as_uuid().to_string(),
            "--target-root",
            target_root.to_str().unwrap(),
        ])
        .output()
        .unwrap(),
        "prepare restore",
    );
    successful(
        supervisor_command(
            &database,
            &journal_root,
            &witness_root,
            &bootstrap_root,
            &journal_key,
        )
        .env("VESTRACE_RESTORE_SOURCE_ROOT", &source_root)
        .env("VESTRACE_PG_SOURCE_FREEZE_BIN", &source_tool)
        .args(["safety-supervisor", "restore", "freeze-source"])
        .output()
        .unwrap(),
        "freeze source",
    );
    successful(
        supervisor_command(
            &database,
            &journal_root,
            &witness_root,
            &bootstrap_root,
            &journal_key,
        )
        .args(["safety-supervisor", "restore", "plan-activation"])
        .output()
        .unwrap(),
        "pin activation plan",
    );

    let materialize_fault = supervisor_command(
        &database,
        &journal_root,
        &witness_root,
        &bootstrap_root,
        &journal_key,
    )
    .env("VESTRACE_RESTORE_SOURCE_ROOT", &source_root)
    .env("VESTRACE_PG_RESTORE_BIN", &target_tool)
    .env(
        "VESTRACE_P05_RESTORE_FAULT_POINT",
        "before-target-initialization",
    )
    .args([
        "safety-supervisor",
        "restore",
        "materialize",
        "--target-root",
        target_root.to_str().unwrap(),
    ])
    .output()
    .unwrap();
    assert_eq!(
        materialize_fault.status.code(),
        Some(87),
        "{materialize_fault:?}"
    );
    successful(
        supervisor_command(
            &database,
            &journal_root,
            &witness_root,
            &bootstrap_root,
            &journal_key,
        )
        .env("VESTRACE_RESTORE_SOURCE_ROOT", &source_root)
        .env("VESTRACE_PG_RESTORE_BIN", &target_tool)
        .args([
            "safety-supervisor",
            "restore",
            "materialize",
            "--target-root",
            target_root.to_str().unwrap(),
        ])
        .output()
        .unwrap(),
        "recover materialization receipt",
    );
    assert_eq!(
        fs::read_to_string(&restore_runs).unwrap().len(),
        1,
        "receipt replay must not invoke the target restore tool again"
    );

    for fault_point in ["after-target-initialized", "after-target-activating"] {
        let crashed = supervisor_command(
            &database,
            &journal_root,
            &witness_root,
            &bootstrap_root,
            &journal_key,
        )
        .env("VESTRACE_RESTORE_SOURCE_ROOT", &source_root)
        .env("VESTRACE_P05_RESTORE_FAULT_POINT", fault_point)
        .args([
            "safety-supervisor",
            "restore",
            "activate",
            "--target-root",
            target_root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
        assert_eq!(
            crashed.status.code(),
            Some(87),
            "{fault_point}: {crashed:?}"
        );
    }
    successful(
        supervisor_command(
            &database,
            &journal_root,
            &witness_root,
            &bootstrap_root,
            &journal_key,
        )
        .env("VESTRACE_RESTORE_SOURCE_ROOT", &source_root)
        .args([
            "safety-supervisor",
            "restore",
            "activate",
            "--target-root",
            target_root.to_str().unwrap(),
        ])
        .output()
        .unwrap(),
        "recover target activation",
    );

    let active_generation: uuid::Uuid =
        sqlx::query_scalar("SELECT active_generation_id FROM public.installation_safety_state")
            .fetch_one(&pool)
            .await
            .unwrap();
    let state: String =
        sqlx::query_scalar("SELECT state FROM public.managed_restore_attempts WHERE attempt_id=$1")
            .bind(attempt)
            .fetch_one(&pool)
            .await
            .unwrap();
    let live_holds: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.managed_backup_restore_holds WHERE hold_id=$1 AND released_at IS NULL",
    )
    .bind(hold)
    .fetch_one(&pool)
    .await
    .unwrap();
    let initialization_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.managed_restore_events \
         WHERE attempt_id=$1 AND event_kind='target_initialized'",
    )
    .bind(attempt)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_generation, target_generation.as_uuid());
    assert_eq!(state, "released");
    assert_eq!(
        live_holds, 0,
        "only the terminal receipt releases the exact hold"
    );
    assert_eq!(
        initialization_events, 1,
        "target initialization is idempotent across restarts"
    );

    empty_restore_authority(&pool).await;
    drop(pool);
    fs::remove_dir_all(root).unwrap();
}
