//! Process-level P05-B archive command boundaries and opt-in live recovery proof.

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
        let bootstrap_url = env::var("VESTRACE_P05_RECOVERY_BOOTSTRAP_DATABASE_URL").ok()?;
        let supervisor_url = env::var("VESTRACE_P05_RECOVERY_SUPERVISOR_DATABASE_URL").ok()?;
        Some(Self {
            bootstrap_url,
            supervisor_url,
        })
    }
}

fn temporary_root() -> PathBuf {
    env::temp_dir().join(format!(
        "vestrace-p05-archive-recovery-{}",
        uuid::Uuid::now_v7()
    ))
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

async fn empty_archive_authority(pool: &PgPool) {
    sqlx::query(
        "TRUNCATE public.managed_backup_events, public.managed_backup_deletion_preparations, \
                  public.managed_backup_restore_holds, public.managed_backup_append_intents, \
                  public.managed_backup_archive_objects, public.managed_backup_archive_heads, \
                  public.managed_backup_sets, public.installation_safety_journal_events, \
                  public.installation_safety_generations, public.installation_safety_state, \
                  public.installation_fingerprint_continuity CASCADE",
    )
    .execute(pool)
    .await
    .unwrap();
}

fn parse_started_set(output: &std::process::Output) -> uuid::Uuid {
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let id = stdout
        .trim()
        .strip_prefix("managed backup started: ")
        .expect("backup begin reports its generated set id");
    uuid::Uuid::parse_str(id).expect("reported backup set id is UUID")
}

fn fake_pg_basebackup(root: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let script = root.join("fake-pg-basebackup.cmd");
        fs::write(
            &script,
            concat!(
                "@echo off\r\n",
                "setlocal EnableExtensions DisableDelayedExpansion\r\n",
                "set format=\r\n",
                "set wal=\r\n",
                "set checkpoint=\r\n",
                "set manifest=\r\n",
                "set capture=\r\n",
                ":next\r\n",
                "if \"%~1\"==\"\" goto done\r\n",
                "if \"%~1\"==\"--format=tar\" set format=1\r\n",
                "if \"%~1\"==\"--wal-method=none\" set wal=1\r\n",
                "if \"%~1\"==\"--checkpoint=fast\" set checkpoint=1\r\n",
                "if \"%~1\"==\"--manifest-force-encode\" set manifest=1\r\n",
                "if \"%~1\"==\"--pgdata\" (set \"capture=%~2\" & shift)\r\n",
                "shift\r\n",
                "goto next\r\n",
                ":done\r\n",
                "if not defined format exit /b 7\r\n",
                "if not defined wal exit /b 7\r\n",
                "if not defined checkpoint exit /b 7\r\n",
                "if not defined manifest exit /b 7\r\n",
                "if not defined capture exit /b 7\r\n",
                "mkdir \"%capture%\" >nul\r\n",
                "echo unrecognized> \"%capture%\\unrecognized\"\r\n",
                "exit /b 0\r\n"
            ),
        )
        .unwrap();
        script
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let script = root.join("fake-pg-basebackup");
        fs::write(
            &script,
            concat!(
                "#!/bin/sh\n",
                "format= wal= checkpoint= manifest= capture=\n",
                "while [ \"$#\" -gt 0 ]; do\n",
                "  case \"$1\" in\n",
                "    --format=tar) format=1 ;;\n",
                "    --wal-method=none) wal=1 ;;\n",
                "    --checkpoint=fast) checkpoint=1 ;;\n",
                "    --manifest-force-encode) manifest=1 ;;\n",
                "    --pgdata) shift; capture=\"$1\" ;;\n",
                "  esac\n",
                "  shift\n",
                "done\n",
                "[ -n \"$format\" ] && [ -n \"$wal\" ] && [ -n \"$checkpoint\" ] && [ -n \"$manifest\" ] && [ -n \"$capture\" ] || exit 7\n",
                "mkdir -p \"$capture\"\n",
                "printf unrecognized > \"$capture/unrecognized\"\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        script
    }
}

#[test]
fn append_wal_refuses_a_relative_host_segment_before_loading_any_authority_root() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args([
            "safety-supervisor",
            "backup",
            "append-wal",
            "--set-id",
            "00000000-0000-0000-0000-000000000001",
            "--segment",
            "relative-wal-segment",
            "--timeline",
            "1",
            "--start-lsn",
            "0",
            "--end-lsn",
            "1",
        ])
        .output()
        .expect("safety supervisor command starts");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("absolute host path"));
}

#[test]
fn backup_cli_exposes_hold_acquisition_but_no_host_release_action() {
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .args(["safety-supervisor", "backup", "--help"])
        .output()
        .expect("safety supervisor backup help starts");

    assert!(output.status.success(), "{output:?}");
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("append-base"));
    assert!(help.contains("capture-base"));
    assert!(help.contains("acquire-hold"));
    assert!(help.contains("begin-seal"));
    assert!(help.contains("commit-seal"));
    assert!(help.contains("prepare-delete"));
    assert!(help.contains("erase-key"));
    assert!(help.contains("finalize-delete"));
    assert!(!help.contains("release-hold"));
}

#[test]
fn capture_base_invokes_an_isolated_tool_and_retains_an_unknown_layout_before_database_access() {
    let root = temporary_root();
    fs::create_dir_all(&root).unwrap();
    let fake = fake_pg_basebackup(&root);
    let set = uuid::Uuid::now_v7();
    let output = Command::new(env!("CARGO_BIN_EXE_vestrace"))
        .env("VESTRACE_SAFETY_JOURNAL_ROOT", root.join("journal"))
        .env("VESTRACE_SAFETY_WITNESS_ROOT", root.join("witness"))
        .env("VESTRACE_SAFETY_BOOTSTRAP_ROOT", root.join("bootstrap"))
        .env(
            "VESTRACE_SAFETY_JOURNAL_SIGNING_KEY",
            root.join("journal.pk8"),
        )
        .env("VESTRACE_BACKUP_ARCHIVE_ROOT", root.join("archive"))
        .env(
            "VESTRACE_BACKUP_ARCHIVE_KEY_ROOT",
            root.join("archive-keys"),
        )
        .env("VESTRACE_PG_BASEBACKUP_BIN", &fake)
        .env(
            "VESTRACE_PG_BASEBACKUP_SOURCE_DSN",
            "postgresql://backup@example.test:5432/vestrace",
        )
        .env("VESTRACE_PG_BASEBACKUP_SOURCE_SYSTEM_IDENTIFIER", "1")
        .args([
            "safety-supervisor",
            "backup",
            "capture-base",
            "--set-id",
            &set.to_string(),
        ])
        .output()
        .expect("safety supervisor command starts");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported capture layout"));
    let captures = fs::read_dir(root.join("archive/base-capture").join(set.to_string()))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        captures.len(),
        1,
        "the tool receives one unique capture root"
    );
    assert!(captures[0].path().join("unrecognized").is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn postgres17_basebackup_format_qualification_is_opt_in() {
    let (Some(binary), Some(source_dsn), Some(expected_system_identifier)) = (
        env::var_os("VESTRACE_PG_BASEBACKUP_BIN"),
        env::var_os("VESTRACE_PG_BASEBACKUP_SOURCE_DSN"),
        env::var_os("VESTRACE_PG_BASEBACKUP_SOURCE_SYSTEM_IDENTIFIER"),
    ) else {
        eprintln!(
            "BLOCKED: set VESTRACE_PG_BASEBACKUP_BIN, VESTRACE_PG_BASEBACKUP_SOURCE_DSN, and \
             VESTRACE_PG_BASEBACKUP_SOURCE_SYSTEM_IDENTIFIER to qualify PostgreSQL 17 base capture"
        );
        return;
    };
    let root = temporary_root();
    let capture = root.join("capture");
    fs::create_dir_all(&root).unwrap();
    let output = Command::new(&binary)
        .args([
            "--dbname".into(),
            source_dsn,
            "--format=tar".into(),
            "--wal-method=none".into(),
            "--checkpoint=fast".into(),
            "--manifest-force-encode".into(),
            "--pgdata".into(),
            capture.clone().into_os_string(),
        ])
        .output()
        .expect("configured pg_basebackup starts");
    assert!(
        output.status.success(),
        "configured pg_basebackup failed without exposing its diagnostic"
    );
    assert!(capture.join("base.tar").is_file());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(capture.join("backup_manifest")).unwrap()).unwrap();
    assert_eq!(
        manifest
            .get("PostgreSQL-Backup-Manifest-Version")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    let expected_system_identifier = expected_system_identifier
        .to_str()
        .and_then(|value| value.parse::<u64>().ok())
        .expect("configured system identifier is decimal u64");
    assert_eq!(
        manifest
            .get("System-Identifier")
            .and_then(serde_json::Value::as_u64),
        Some(expected_system_identifier)
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn live_archive_lifecycle_commits_one_member_and_finalizes_its_exact_manifest() {
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
    empty_archive_authority(&pool).await;

    let root = temporary_root();
    let journal_root = root.join("journal");
    let witness_root = root.join("witness");
    let bootstrap_root = root.join("bootstrap");
    let journal_key = root.join("journal.pk8");
    let base = root.join("base.tar");
    let segment = root.join("segment-000000010000000000000001.wal");
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
    fs::write(&base, vec![0x42; 64 * 1024 + 19]).unwrap();
    fs::write(&segment, vec![0x5a; 64 * 1024 + 37]).unwrap();

    let installation_id = InstallationId::new();
    let fingerprint_key_id = FingerprintKeyId::new();
    let fingerprint = InstallationFingerprintKey::new(
        installation_id,
        fingerprint_key_id,
        FingerprintKeyVersion::V1,
        FingerprintKey::from_bytes([31; 32]),
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

    let initialized = supervisor_command(
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
    ])
    .output()
    .unwrap();
    assert!(initialized.status.success(), "{initialized:?}");

    let set = parse_started_set(
        &supervisor_command(
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
    let use_real_base_capture = [
        "VESTRACE_PG_BASEBACKUP_BIN",
        "VESTRACE_PG_BASEBACKUP_SOURCE_DSN",
        "VESTRACE_PG_BASEBACKUP_SOURCE_SYSTEM_IDENTIFIER",
    ]
    .into_iter()
    .all(|name| env::var_os(name).is_some());
    let mut base_command = supervisor_command(
        &database,
        &journal_root,
        &witness_root,
        &bootstrap_root,
        &journal_key,
    );
    if use_real_base_capture {
        base_command.args([
            "safety-supervisor",
            "backup",
            "capture-base",
            "--set-id",
            &set.to_string(),
        ]);
    } else {
        base_command.args([
            "safety-supervisor",
            "backup",
            "append-base",
            "--set-id",
            &set.to_string(),
            "--segment",
            base.to_str().unwrap(),
            "--timeline",
            "1",
            "--start-lsn",
            "0",
            "--end-lsn",
            "1",
        ]);
    }
    let base_appended = base_command.output().unwrap();
    assert!(base_appended.status.success(), "{base_appended:?}");
    let base_start_lsn: i64 = sqlx::query_scalar(
        "SELECT start_lsn FROM public.managed_backup_archive_objects \
         WHERE backup_set_id=$1 AND ordinal=1 AND object_kind=0",
    )
    .bind(set)
    .fetch_one(&pool)
    .await
    .unwrap();
    let wal_start_lsn = if use_real_base_capture {
        base_start_lsn.to_string()
    } else {
        "1".to_owned()
    };
    let wal_end_lsn = base_start_lsn
        .checked_add(65_573)
        .expect("test base LSN plus WAL range fits i64")
        .to_string();
    let appended = supervisor_command(
        &database,
        &journal_root,
        &witness_root,
        &bootstrap_root,
        &journal_key,
    )
    .args([
        "safety-supervisor",
        "backup",
        "append-wal",
        "--set-id",
        &set.to_string(),
        "--segment",
        segment.to_str().unwrap(),
        "--timeline",
        "1",
        "--start-lsn",
        &wal_start_lsn,
        "--end-lsn",
        &wal_end_lsn,
    ])
    .output()
    .unwrap();
    assert!(appended.status.success(), "{appended:?}");

    let (checkpoint_ordinal, objects, intents, archive_events, safety_sequence):
        (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT checkpoint_ordinal FROM public.managed_backup_archive_heads WHERE backup_set_id=$1), \
                (SELECT count(*) FROM public.managed_backup_archive_objects WHERE backup_set_id=$1), \
                (SELECT count(*) FROM public.managed_backup_append_intents WHERE backup_set_id=$1), \
                (SELECT count(*) FROM public.managed_backup_events WHERE backup_set_id=$1), \
                (SELECT witness_sequence FROM public.installation_safety_state)",
    )
    .bind(set)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (
            checkpoint_ordinal,
            objects,
            intents,
            archive_events,
            safety_sequence
        ),
        (2, 2, 0, 3, 4),
        "the live archive has base then WAL members and no residual staging intent"
    );
    let final_objects = root.join("archive").join("objects").join(set.to_string());
    assert_eq!(
        fs::read_dir(final_objects)
            .unwrap()
            .flatten()
            .map(|kind| fs::read_dir(kind.path()).unwrap().flatten().count())
            .sum::<usize>(),
        2,
        "base and WAL objects were promoted only after their checkpoints"
    );
    assert!(
        !root
            .join("archive")
            .join("staging")
            .join(set.to_string())
            .exists(),
        "no ciphertext spool survives a committed append"
    );

    for action in [
        "begin-seal",
        "commit-seal",
        "prepare-delete",
        "erase-key",
        "finalize-delete",
    ] {
        let output = supervisor_command(
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
        ])
        .output()
        .unwrap();
        assert!(output.status.success(), "{action} failed: {output:?}");
    }

    let (lifecycle, remaining_objects, final_events, final_sequence): (String, i64, i64, i64) =
        sqlx::query_as(
            "SELECT (SELECT lifecycle FROM public.managed_backup_sets WHERE backup_set_id=$1), \
                    (SELECT count(*) FROM public.managed_backup_archive_objects WHERE backup_set_id=$1), \
                    (SELECT count(*) FROM public.managed_backup_events WHERE backup_set_id=$1), \
                    (SELECT witness_sequence FROM public.installation_safety_state)",
        )
        .bind(set)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        (
            lifecycle.as_str(),
            remaining_objects,
            final_events,
            final_sequence
        ),
        ("deleted", 2, 9, 10),
        "finalization preserves the immutable manifest metadata while deleting only its host object"
    );
    assert_eq!(
        fs::read_dir(root.join("archive").join("objects").join(set.to_string()))
            .unwrap()
            .flatten()
            .map(|kind| fs::read_dir(kind.path()).unwrap().flatten().count())
            .sum::<usize>(),
        0,
        "the final host deletion removes the one manifest-named object"
    );

    empty_archive_authority(&pool).await;
    drop(pool);
    fs::remove_dir_all(root).unwrap();
}
