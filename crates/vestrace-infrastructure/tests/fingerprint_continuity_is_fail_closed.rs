use sqlx::{PgPool, postgres::PgPoolOptions};
use tempfile::TempDir;
use uuid::Uuid;
use vestrace_application::{ApplicationError, HealthRepository};
use vestrace_domain::installation::{
    FINGERPRINT_CONTINUITY_DOMAIN, FingerprintKey, FingerprintKeyId, FingerprintKeyVersion,
    FingerprintScope, InstallationFingerprintKey, InstallationId, continuity_proof,
    external_id_fingerprint,
};
use vestrace_domain::{PrincipalId, WorkspaceId};
use vestrace_infrastructure::{
    INSTALLATION_FINGERPRINT_RECORD_FILE, INSTALLATION_FINGERPRINT_VAULT_ROOT_ENV,
    InstallationFingerprintReadinessCause, PgInstallationFingerprintReadiness,
    PgInstallationFingerprintSupervisor, PgStore,
};

fn installation_id(value: u128) -> InstallationId {
    InstallationId::from_uuid(Uuid::from_u128(value))
}

fn fingerprint_key_id(value: u128) -> FingerprintKeyId {
    FingerprintKeyId::from_uuid(Uuid::from_u128(value))
}

fn installation_key(
    installation_id: InstallationId,
    fingerprint_key_id: FingerprintKeyId,
    bytes: [u8; 32],
) -> InstallationFingerprintKey {
    InstallationFingerprintKey::new(
        installation_id,
        fingerprint_key_id,
        FingerprintKeyVersion::V1,
        FingerprintKey::from_bytes(bytes),
    )
}

fn assert_unavailable(result: Result<(), ApplicationError>) {
    assert!(matches!(
        result,
        Err(ApplicationError::Unavailable(message))
            if message == "installation fingerprint continuity is not ready"
    ));
}

#[test]
fn absent_vault_root_retains_a_distinct_readiness_cause() {
    temp_env::with_var(
        INSTALLATION_FINGERPRINT_VAULT_ROOT_ENV,
        None::<&str>,
        || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let readiness = runtime.block_on(async {
                let pool = PgPoolOptions::new()
                    .connect_lazy("postgres://unused:unused@localhost/unused")
                    .unwrap();
                PgInstallationFingerprintReadiness::initialize_from_environment(PgStore::from_pool(
                    pool,
                ))
                .await
            });

            assert_eq!(
                readiness.retained_cause(),
                Some(InstallationFingerprintReadinessCause::VaultRootAbsent)
            );
        },
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn rejected_create_only_record_retains_a_distinct_readiness_cause(pool: PgPool) {
    let root = TempDir::new().unwrap();
    let first = PgInstallationFingerprintReadiness::initialize(
        PgStore::from_pool(pool.clone()),
        root.path(),
    )
    .await;
    assert_eq!(first.retained_cause(), None);

    sqlx::query(
        "UPDATE installation_fingerprint_continuity \
         SET fingerprint_key_version = fingerprint_key_version + 1",
    )
    .execute(&pool)
    .await
    .unwrap();

    let rejected =
        PgInstallationFingerprintReadiness::initialize(PgStore::from_pool(pool), root.path()).await;

    assert_eq!(
        rejected.retained_cause(),
        Some(InstallationFingerprintReadinessCause::CreateOnlyRecordRejected)
    );
    assert_ne!(
        rejected.retained_cause(),
        Some(InstallationFingerprintReadinessCause::VaultRootAbsent)
    );
}

async fn deployment_pool() -> Option<PgPool> {
    let Ok(url) = std::env::var("VESTRACE_P05_TEST_DATABASE_URL") else {
        eprintln!(
            "BLOCKED: set VESTRACE_P05_TEST_DATABASE_URL to a disposable database \
             taken through the three-phase P05 route"
        );
        return None;
    };
    Some(
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap(),
    )
}

#[tokio::test]
async fn production_readiness_reloads_the_host_fingerprint_record() {
    let Some(pool) = deployment_pool().await else {
        return;
    };
    let root = TempDir::new().unwrap();
    let readiness =
        PgInstallationFingerprintReadiness::initialize(PgStore::from_pool(pool), root.path()).await;

    HealthRepository::check(&readiness).await.unwrap();
    std::fs::remove_file(root.path().join(INSTALLATION_FINGERPRINT_RECORD_FILE)).unwrap();

    assert_unavailable(HealthRepository::check(&readiness).await);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn key_record_is_never_stored_in_postgres(pool: PgPool) {
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name \
         FROM information_schema.columns \
         WHERE table_schema = 'public' \
           AND table_name = 'installation_fingerprint_continuity' \
         ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        columns,
        vec![
            "installation_id",
            "fingerprint_key_id",
            "fingerprint_key_version",
            "continuity_proof",
            "created_at",
        ]
    );
}

#[test]
fn continuity_proof_uses_the_exact_domain_separation_string() {
    assert_eq!(
        FINGERPRINT_CONTINUITY_DOMAIN,
        b"vestrace-installation-fingerprint-v1"
    );
    let key = FingerprintKey::from_bytes([0xa5; 32]);
    let proof = continuity_proof(&key, &installation_id(1), &fingerprint_key_id(2));

    assert_eq!(
        proof.as_bytes(),
        &[
            0x7f, 0x1c, 0x76, 0xa6, 0x8c, 0xaa, 0x78, 0xc0, 0xcb, 0xac, 0xa4, 0x58, 0x2e, 0xcc,
            0x17, 0x4d, 0xd6, 0x1c, 0x4f, 0x5f, 0xb1, 0xd1, 0x3a, 0xc4, 0x29, 0xf8, 0x05, 0xdc,
            0x4e, 0x39, 0x15, 0xa3,
        ]
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn readiness_fails_closed_on_missing_proof(pool: PgPool) {
    let supervisor = PgInstallationFingerprintSupervisor::new(pool);
    let key = installation_key(installation_id(10), fingerprint_key_id(11), [1; 32]);

    assert_unavailable(supervisor.ensure_ready(&key).await);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn readiness_fails_closed_on_wrong_key_id(pool: PgPool) {
    let supervisor = PgInstallationFingerprintSupervisor::new(pool);
    let installation = installation_id(12);
    let stored = installation_key(installation, fingerprint_key_id(13), [2; 32]);
    supervisor.record_create_only(&stored).await.unwrap();
    let wrong_identity = installation_key(installation, fingerprint_key_id(14), [2; 32]);

    assert_unavailable(supervisor.ensure_ready(&wrong_identity).await);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn readiness_fails_closed_on_wrong_key_version(pool: PgPool) {
    let supervisor = PgInstallationFingerprintSupervisor::new(pool.clone());
    let key = installation_key(installation_id(15), fingerprint_key_id(16), [3; 32]);
    supervisor.record_create_only(&key).await.unwrap();
    sqlx::query(
        "UPDATE installation_fingerprint_continuity \
         SET fingerprint_key_version = fingerprint_key_version + 1 \
         WHERE installation_id = $1",
    )
    .bind(key.installation_id().as_uuid())
    .execute(&pool)
    .await
    .unwrap();

    assert_unavailable(supervisor.ensure_ready(&key).await);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn readiness_fails_closed_on_same_key_id_different_key(pool: PgPool) {
    let supervisor = PgInstallationFingerprintSupervisor::new(pool);
    let installation = installation_id(17);
    let key_id = fingerprint_key_id(18);
    let stored = installation_key(installation, key_id, [4; 32]);
    supervisor.record_create_only(&stored).await.unwrap();
    let different_key = installation_key(installation, key_id, [5; 32]);

    assert_unavailable(supervisor.ensure_ready(&different_key).await);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn no_rotation_or_overwrite_operation_exists(pool: PgPool) {
    let supervisor = PgInstallationFingerprintSupervisor::new(pool.clone());
    let installation = installation_id(19);
    let first = installation_key(installation, fingerprint_key_id(20), [6; 32]);
    supervisor.record_create_only(&first).await.unwrap();
    let replacement = installation_key(installation, fingerprint_key_id(21), [7; 32]);

    assert!(matches!(
        supervisor.record_create_only(&replacement).await,
        Err(ApplicationError::Conflict(message)) if message == "installation fingerprint already exists"
    ));
    let stored_key_id: Uuid = sqlx::query_scalar(
        "SELECT fingerprint_key_id FROM installation_fingerprint_continuity WHERE installation_id = $1",
    )
    .bind(installation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored_key_id, first.fingerprint_key_id().as_uuid());
}

#[test]
fn external_id_fingerprint_is_not_the_continuity_proof() {
    let key = FingerprintKey::from_bytes([8; 32]);
    let scope = FingerprintScope::new(
        WorkspaceId::from_uuid(Uuid::from_u128(22)),
        PrincipalId::from_uuid(Uuid::from_u128(23)),
        "a2a",
        "/agents",
    );
    let external = external_id_fingerprint(&key, &scope, "untrusted-thread-id");
    let proof = continuity_proof(&key, &installation_id(24), &fingerprint_key_id(25));

    assert_ne!(external.as_bytes(), proof.as_bytes());
}

#[test]
fn external_id_fingerprint_never_hashes_retention_governed_content() {
    let key = FingerprintKey::from_bytes([9; 32]);
    let first_scope = FingerprintScope::new(
        WorkspaceId::from_uuid(Uuid::from_u128(26)),
        PrincipalId::from_uuid(Uuid::from_u128(27)),
        "ag-ui",
        "/run",
    );
    let second_scope = FingerprintScope::new(
        WorkspaceId::from_uuid(Uuid::from_u128(28)),
        PrincipalId::from_uuid(Uuid::from_u128(27)),
        "ag-ui",
        "/run",
    );

    let first = external_id_fingerprint(&key, &first_scope, "external-id-only");
    let second = external_id_fingerprint(&key, &second_scope, "external-id-only");

    assert_ne!(first.as_bytes(), second.as_bytes());
}
