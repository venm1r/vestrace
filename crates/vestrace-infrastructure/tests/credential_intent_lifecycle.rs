use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Transaction};
use tempfile::TempDir;
use tokio::sync::Barrier;
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, CredentialIntentRepository, CredentialMaterialPreparationFaultInjector,
    CredentialMaterialPreparationFaultPoint, CredentialMaterialPreparationRequest,
    CredentialMaterialPreparationState, CredentialMaterialPreparer, InstallationMutationPermit,
    MaterialKeyVault, OperatorCredential, PermitHandle, PermitMode, RequestContext,
};
use vestrace_domain::trust::{KeyPurpose, KeyReference, SecretResolutionRequest};
use vestrace_domain::{
    ConnectionId, CredentialKeyCreationIntent, CredentialKeyCreationIntentId,
    CredentialPreparedAttachmentId, CredentialRevisionId, CredentialSlotId, IntentNonce,
    MaterialKeyId, PrincipalId, WorkspaceId, ZeroizingDek,
};
use vestrace_infrastructure::{
    PgCredentialIntentRepository, PgCredentialMaterialPreparer, PgInstallationMutationPermit,
    PgStore,
    crypto::{EnvelopeCipher, HostMaterialKeyVault, MOUNTED_SECRET_STORE_PROVIDER},
};

struct ReplayObservingAllocator;

static REPLAY_POINTER: AtomicUsize = AtomicUsize::new(0);
static REPLAY_DEALLOCATION_OBSERVED: AtomicBool = AtomicBool::new(false);
static REPLAY_DEALLOCATION_ZERO: AtomicBool = AtomicBool::new(false);
const REPLAY_SECRET: &[u8] = b"terminal-replay-new-secret\xff";

// SAFETY: allocation/deallocation are forwarded unchanged. The observer reads
// only the exact registered allocation before forwarding its deallocation and
// performs atomic writes without allocating or logging.
unsafe impl GlobalAlloc for ReplayObservingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if REPLAY_POINTER.load(Ordering::SeqCst) == pointer as usize {
            let mut all_zero = true;
            for offset in 0..REPLAY_SECRET.len() {
                if unsafe { pointer.add(offset).read() } != 0 {
                    all_zero = false;
                    break;
                }
            }
            REPLAY_DEALLOCATION_ZERO.store(all_zero, Ordering::SeqCst);
            REPLAY_DEALLOCATION_OBSERVED.store(true, Ordering::SeqCst);
            REPLAY_POINTER.store(0, Ordering::SeqCst);
        }
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: ReplayObservingAllocator = ReplayObservingAllocator;

fn track_replay_deallocation(pointer: *const u8) {
    REPLAY_DEALLOCATION_OBSERVED.store(false, Ordering::SeqCst);
    REPLAY_DEALLOCATION_ZERO.store(false, Ordering::SeqCst);
    REPLAY_POINTER.store(pointer as usize, Ordering::SeqCst);
}

#[test]
fn credential_material_preparer_api_consumes_typed_zeroizing_input() {
    fn takes_preparer<P: CredentialMaterialPreparer>(_preparer: &P) {}
    fn takes_request(_request: CredentialMaterialPreparationRequest) {}
    let _ = takes_preparer::<PgCredentialMaterialPreparer<HostMaterialKeyVault>>;
    let _ = takes_request;
    let credential = OperatorCredential::new("operator-secret");
    assert_eq!(credential.expose(), b"operator-secret");
}

struct PreparationVault {
    creates: AtomicUsize,
    unwraps: AtomicUsize,
}

struct FailCredentialPreparationOnce {
    point: CredentialMaterialPreparationFaultPoint,
    fired: std::sync::atomic::AtomicBool,
}

struct RefusingInstallationPermit;

#[async_trait]
impl InstallationMutationPermit for RefusingInstallationPermit {
    async fn acquire(
        &self,
        _mode: PermitMode,
        _context: &RequestContext,
    ) -> Result<PermitHandle, ApplicationError> {
        Err(ApplicationError::Unavailable(
            "installation permit unavailable".to_owned(),
        ))
    }
}

impl CredentialMaterialPreparationFaultInjector for FailCredentialPreparationOnce {
    fn check(
        &self,
        point: CredentialMaterialPreparationFaultPoint,
    ) -> Result<(), ApplicationError> {
        if point == self.point && !self.fired.swap(true, Ordering::SeqCst) {
            Err(ApplicationError::Unavailable(format!(
                "injected credential preparation fault at {point:?}"
            )))
        } else {
            Ok(())
        }
    }
}

impl PreparationVault {
    fn counts(&self) -> (usize, usize) {
        (
            self.creates.load(Ordering::SeqCst),
            self.unwraps.load(Ordering::SeqCst),
        )
    }
}

impl MaterialKeyVault for PreparationVault {
    fn create_if_absent(
        &self,
        _key_id: MaterialKeyId,
        _nonce: IntentNonce,
    ) -> Result<vestrace_domain::VaultReceipt, vestrace_application::VaultError> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        Ok(vestrace_domain::VaultReceipt::from_uuid(Uuid::from_u128(7)))
    }

    fn unwrap(
        &self,
        _key_id: MaterialKeyId,
        use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), vestrace_application::VaultError> {
        self.unwraps.fetch_add(1, Ordering::SeqCst);
        use_dek(&ZeroizingDek::new([0x3D; 32]));
        Ok(())
    }

    fn prepare_erasure(
        &self,
        _key_id: MaterialKeyId,
    ) -> Result<vestrace_application::FenceReceipt, vestrace_application::VaultError> {
        unreachable!()
    }

    fn erase(
        &self,
        _key_id: MaterialKeyId,
    ) -> Result<vestrace_domain::ErasureReceipt, vestrace_application::VaultError> {
        unreachable!()
    }
}

fn material_preparer(
    pool: &PgPool,
    vault: Arc<PreparationVault>,
) -> PgCredentialMaterialPreparer<PreparationVault> {
    let store = PgStore::from_pool(pool.clone());
    PgCredentialMaterialPreparer::new(Arc::new(PgInstallationMutationPermit::new(store)), vault)
}

fn material_preparer_with_faults(
    pool: &PgPool,
    vault: Arc<PreparationVault>,
    faults: Arc<dyn CredentialMaterialPreparationFaultInjector>,
) -> PgCredentialMaterialPreparer<PreparationVault> {
    let store = PgStore::from_pool(pool.clone());
    PgCredentialMaterialPreparer::with_faults(
        Arc::new(PgInstallationMutationPermit::new(store)),
        vault,
        faults,
    )
}

const BOOTSTRAP_KEY_ID: &str = "credential-intent-bootstrap";
const BOOTSTRAP_SCOPE: &str = "credential-intent-bootstrap";
const BOOTSTRAP_ALGORITHM: &str = "aes-256-gcm-v1";

struct Fixture {
    workspace_id: Uuid,
    principal_id: Uuid,
    connection_id: Uuid,
    slot_id: Uuid,
    occupancy_id: Uuid,
    intent_id: Uuid,
    revision_id: Uuid,
    material_key_id: Uuid,
    nonce: Uuid,
    attachment_id: Uuid,
}

fn preparation_request(
    fixture: &Fixture,
    attachment_id: Uuid,
    secret: impl AsRef<[u8]>,
) -> CredentialMaterialPreparationRequest {
    CredentialMaterialPreparationRequest {
        connection_id: ConnectionId::from_uuid(fixture.connection_id),
        credential_slot_id: CredentialSlotId::from_uuid(fixture.slot_id),
        credential_revision_id: CredentialRevisionId::from_uuid(fixture.revision_id),
        material_key_id: MaterialKeyId::from_uuid(fixture.material_key_id),
        intent_id: CredentialKeyCreationIntentId::from_uuid(fixture.intent_id),
        intent_nonce: IntentNonce::from_uuid(fixture.nonce),
        prepared_attachment_id: CredentialPreparedAttachmentId::from_uuid(attachment_id),
        credential: OperatorCredential::new(secret),
    }
}

struct VaultFixture {
    bootstrap_root: TempDir,
    vault_root: TempDir,
    bootstrap_reference: KeyReference,
    bootstrap_request: SecretResolutionRequest,
}

impl VaultFixture {
    fn new() -> Self {
        let bootstrap_root = TempDir::new().expect("bootstrap mount");
        write_bootstrap_key(bootstrap_root.path());
        Self {
            vault_root: TempDir::new().expect("material vault"),
            bootstrap_reference: KeyReference::new(
                MOUNTED_SECRET_STORE_PROVIDER,
                BOOTSTRAP_KEY_ID,
                "v1",
                KeyPurpose::Storage,
                BOOTSTRAP_SCOPE,
                BOOTSTRAP_ALGORITHM,
            )
            .expect("bootstrap reference"),
            bootstrap_request: SecretResolutionRequest::new(
                WorkspaceId::new(),
                BOOTSTRAP_SCOPE,
                "test://credential-intent-vault",
            ),
            bootstrap_root,
        }
    }

    fn vault(&self) -> HostMaterialKeyVault {
        HostMaterialKeyVault::new(
            self.vault_root.path(),
            self.bootstrap_root.path(),
            self.bootstrap_reference.clone(),
            self.bootstrap_request.clone(),
        )
        .expect("separate host vault")
    }
}

fn write_bootstrap_key(root: &Path) {
    let key_root = root.join(BOOTSTRAP_KEY_ID);
    let version_root = key_root.join("v1");
    fs::create_dir_all(&version_root).expect("bootstrap key directory");
    fs::write(key_root.join("scope"), BOOTSTRAP_SCOPE).expect("bootstrap scope");
    fs::write(key_root.join("purpose"), "storage").expect("bootstrap purpose");
    fs::write(key_root.join("algorithm"), BOOTSTRAP_ALGORITHM).expect("bootstrap algorithm");
    fs::write(version_root.join("state"), "active").expect("bootstrap state");
    fs::write(version_root.join("private.pkcs8"), [0x5A; 32]).expect("bootstrap key material");
}

fn assert_sqlstate<T: std::fmt::Debug>(
    result: Result<T, sqlx::Error>,
    expected: &str,
    operation: &str,
) {
    let error = result.expect_err(operation);
    let sqlstate = error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code.into_owned());
    assert_eq!(
        sqlstate.as_deref(),
        Some(expected),
        "{operation} must return SQLSTATE {expected}, got {}",
        sqlstate.as_deref().unwrap_or("no SQLSTATE"),
    );
}

fn assert_constraint_message<T: std::fmt::Debug>(
    result: Result<T, sqlx::Error>,
    expected_message: &str,
    operation: &str,
) {
    let error = result.expect_err(operation);
    let database_error = error
        .as_database_error()
        .expect("the guarded operation must return a database error");
    assert_eq!(database_error.code().as_deref(), Some("23514"));
    assert!(
        database_error.message().contains(expected_message),
        "{operation} must return {expected_message:?}, got {:?}",
        database_error.message(),
    );
}

fn context(fixture: &Fixture) -> RequestContext {
    RequestContext::new(
        WorkspaceId::from_uuid(fixture.workspace_id),
        PrincipalId::from_uuid(fixture.principal_id),
    )
}

async fn context_transaction<'a>(pool: &'a PgPool, fixture: &Fixture) -> Transaction<'a, Postgres> {
    let mut transaction = pool.begin().await.unwrap();
    for (setting, value) in [
        ("vestrace.workspace_id", fixture.workspace_id.to_string()),
        ("vestrace.principal_id", fixture.principal_id.to_string()),
    ] {
        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind(setting)
            .bind(value)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    }
    transaction
}

async fn execute_scoped(
    pool: &PgPool,
    fixture: &Fixture,
    query: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
) -> Result<(), sqlx::Error> {
    let mut transaction = context_transaction(pool, fixture).await;
    match query.execute(&mut *transaction).await {
        Ok(_) => transaction.commit().await,
        Err(error) => Err(error),
    }
}

async fn fixture(pool: &PgPool) -> Fixture {
    let fixture = Fixture {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        connection_id: Uuid::now_v7(),
        slot_id: Uuid::now_v7(),
        occupancy_id: Uuid::now_v7(),
        intent_id: Uuid::now_v7(),
        revision_id: Uuid::now_v7(),
        material_key_id: Uuid::now_v7(),
        nonce: Uuid::now_v7(),
        attachment_id: Uuid::now_v7(),
    };
    let connector_id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(fixture.workspace_id)
        .bind(format!("credential-intent-{}", fixture.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(fixture.principal_id)
        .bind(fixture.workspace_id)
        .bind(format!(
            "credential-intent-principal-{}",
            fixture.principal_id
        ))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(connector_id)
    .bind(fixture.workspace_id)
    .bind(format!("credential-intent-connector-{connector_id}"))
    .bind("openai")
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections (id, connector_id, workspace_id, principal_id, name) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(fixture.connection_id)
    .bind(connector_id)
    .bind(fixture.workspace_id)
    .bind(fixture.principal_id)
    .bind(format!(
        "credential-intent-connection-{}",
        fixture.connection_id
    ))
    .execute(pool)
    .await
    .unwrap();

    let mut transaction = context_transaction(pool, &fixture).await;
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
        .bind(Uuid::now_v7())
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_slot($1, $2, $3, $4, $5)")
        .bind(fixture.slot_id)
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind("provider")
        .bind("primary")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1, $2, $3, $4)")
        .bind(Uuid::now_v7())
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(fixture.slot_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1, $2, $3, $4)")
        .bind(fixture.occupancy_id)
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(fixture.slot_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    fixture
}

async fn reserve(pool: &PgPool, fixture: &Fixture) -> Result<(), sqlx::Error> {
    execute_scoped(
        pool,
        fixture,
        sqlx::query(
            "SELECT vestrace_reserve_credential_key_creation_intent(\
             $1, $2, $3, $4, $5, $6, $7, $8, 'credential_v2')",
        )
        .bind(fixture.intent_id)
        .bind(fixture.workspace_id)
        .bind(fixture.connection_id)
        .bind(fixture.slot_id)
        .bind(fixture.occupancy_id)
        .bind(fixture.revision_id)
        .bind(fixture.material_key_id)
        .bind(fixture.nonce),
    )
    .await
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn credential_material_preparation_is_framed_and_exact_replay_is_state_first(pool: PgPool) {
    let fixture = fixture(&pool).await;
    reserve(&pool, &fixture).await.unwrap();
    let vault = Arc::new(PreparationVault {
        creates: AtomicUsize::new(0),
        unwraps: AtomicUsize::new(0),
    });
    let preparer = material_preparer(&pool, Arc::clone(&vault));

    let prepared = preparer
        .prepare(
            &context(&fixture),
            preparation_request(&fixture, fixture.attachment_id, "operator-secret"),
        )
        .await
        .unwrap();
    assert_eq!(
        prepared.prepared_attachment_id,
        Some(CredentialPreparedAttachmentId::from_uuid(
            fixture.attachment_id
        ))
    );
    assert_eq!(vault.counts(), (1, 1));
    let (state, frame): (String, Vec<u8>) = sqlx::query_as(
        "SELECT intent.state, material.ciphertext
           FROM credential_key_creation_intents AS intent
           JOIN credential_prepared_materials AS material ON material.intent_id=intent.id
          WHERE intent.id=$1",
    )
    .bind(fixture.intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "credential_prepared");
    assert_eq!(frame.len(), 4096);
    assert_eq!(&frame[..5], b"VCRF\x02");

    let replay = preparer
        .prepare(
            &context(&fixture),
            preparation_request(
                &fixture,
                fixture.attachment_id,
                "new-input-must-be-zeroized-without-vault-use",
            ),
        )
        .await
        .unwrap();
    assert_eq!(replay, prepared);
    assert_eq!(vault.counts(), (1, 1));

    let refused = preparer
        .prepare(
            &context(&fixture),
            preparation_request(&fixture, Uuid::now_v7(), "unequal-replay"),
        )
        .await;
    assert!(matches!(refused, Err(ApplicationError::Policy(_))));
    assert_eq!(vault.counts(), (1, 1));
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM credential_prepared_materials WHERE intent_id=$1),
            (SELECT COUNT(*) FROM credential_prepared_attachments WHERE intent_id=$1)",
    )
    .bind(fixture.intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn credential_material_replay_reports_every_safe_effective_lifecycle_state(pool: PgPool) {
    let credential_prepared = credential_prepared(&pool).await;
    let bound = bound(&pool).await;
    let candidate = candidate(&pool).await;
    let active = active(&pool).await;
    let retired = retire_active_as_guarded_owner(&pool, false).await;
    let revoked = retire_active_as_guarded_owner(&pool, true).await;
    let abandon_prepared = credential_abandon_prepared(&pool).await;
    let (erasure_prepared, _) = erasure_prepared(&pool).await;
    let destroyed = destroyed(&pool).await;
    let abandoned = abandoned(&pool).await;
    let cases = [
        (
            credential_prepared,
            CredentialMaterialPreparationState::CredentialPrepared,
            true,
        ),
        (bound, CredentialMaterialPreparationState::Bound, true),
        (
            candidate,
            CredentialMaterialPreparationState::Candidate,
            false,
        ),
        (active, CredentialMaterialPreparationState::Active, false),
        (retired, CredentialMaterialPreparationState::Retired, false),
        (revoked, CredentialMaterialPreparationState::Revoked, false),
        (
            abandon_prepared,
            CredentialMaterialPreparationState::CredentialAbandonPrepared,
            false,
        ),
        (
            erasure_prepared,
            CredentialMaterialPreparationState::ErasurePrepared,
            false,
        ),
        (
            destroyed,
            CredentialMaterialPreparationState::Destroyed,
            false,
        ),
        (
            abandoned,
            CredentialMaterialPreparationState::Abandoned,
            false,
        ),
    ];

    for (fixture, expected_state, has_attachment) in cases {
        let before = credential_replay_snapshot(&pool, &fixture).await;
        let vault = Arc::new(PreparationVault {
            creates: AtomicUsize::new(0),
            unwraps: AtomicUsize::new(0),
        });
        let replay_request = preparation_request(&fixture, fixture.attachment_id, REPLAY_SECRET);
        track_replay_deallocation(replay_request.credential.expose().as_ptr());
        let replay = material_preparer(&pool, Arc::clone(&vault))
            .prepare(&context(&fixture), replay_request)
            .await
            .unwrap();

        assert_eq!(replay.state, expected_state);
        assert_eq!(
            replay.intent_id,
            CredentialKeyCreationIntentId::from_uuid(fixture.intent_id)
        );
        assert_eq!(
            replay.credential_revision_id,
            CredentialRevisionId::from_uuid(fixture.revision_id)
        );
        assert_eq!(
            replay.material_key_id,
            MaterialKeyId::from_uuid(fixture.material_key_id)
        );
        assert_eq!(replay.prepared_attachment_id.is_some(), has_attachment);
        assert_eq!(vault.counts(), (0, 0), "{expected_state:?}");
        assert!(
            REPLAY_DEALLOCATION_OBSERVED.load(Ordering::SeqCst),
            "{expected_state:?}: new replay credential allocation was not deallocated"
        );
        assert!(
            REPLAY_DEALLOCATION_ZERO.load(Ordering::SeqCst),
            "{expected_state:?}: new replay credential allocation was not zero before deallocation"
        );
        assert_eq!(
            credential_replay_snapshot(&pool, &fixture).await,
            before,
            "{expected_state:?} replay wrote state"
        );

        let mut unequal = preparation_request(&fixture, fixture.attachment_id, REPLAY_SECRET);
        unequal.material_key_id = MaterialKeyId::new();
        track_replay_deallocation(unequal.credential.expose().as_ptr());
        let refused = material_preparer(&pool, Arc::clone(&vault))
            .prepare(&context(&fixture), unequal)
            .await;
        assert!(
            matches!(refused, Err(ApplicationError::Policy(_))),
            "{expected_state:?}: unequal persisted identity replay was accepted"
        );
        assert_eq!(vault.counts(), (0, 0), "{expected_state:?}");
        assert!(REPLAY_DEALLOCATION_OBSERVED.load(Ordering::SeqCst));
        assert!(REPLAY_DEALLOCATION_ZERO.load(Ordering::SeqCst));
        assert_eq!(credential_replay_snapshot(&pool, &fixture).await, before);
    }
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn exclusive_installation_permit_blocks_preparation_before_vault_activity(pool: PgPool) {
    let fixture = Arc::new(fixture(&pool).await);
    reserve(&pool, &fixture).await.unwrap();
    let vault = Arc::new(PreparationVault {
        creates: AtomicUsize::new(0),
        unwraps: AtomicUsize::new(0),
    });
    let store = PgStore::from_pool(pool.clone());
    let permit = Arc::new(PgInstallationMutationPermit::new(store.clone()));
    let exclusive = permit
        .acquire(PermitMode::Exclusive, &context(&fixture))
        .await
        .unwrap();
    let preparer = Arc::new(PgCredentialMaterialPreparer::new(
        permit,
        Arc::clone(&vault),
    ));
    let task_fixture = Arc::clone(&fixture);
    let mut task = tokio::spawn(async move {
        preparer
            .prepare(
                &context(&task_fixture),
                preparation_request(
                    &task_fixture,
                    task_fixture.attachment_id,
                    "exclusive-block-secret",
                ),
            )
            .await
    });

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), &mut task)
            .await
            .is_err(),
        "preparation did not wait behind the exclusive installation permit"
    );
    assert_eq!(vault.counts(), (0, 0));
    exclusive.rollback().await.unwrap();
    let prepared = tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .expect("preparation remained blocked after the exclusive permit released")
        .unwrap()
        .unwrap();
    assert_eq!(
        prepared.state,
        vestrace_application::CredentialMaterialPreparationState::CredentialPrepared
    );
    assert_eq!(vault.counts(), (1, 1));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn installation_permit_error_propagates_before_preparation_side_effects(pool: PgPool) {
    let fixture = fixture(&pool).await;
    reserve(&pool, &fixture).await.unwrap();
    let vault = Arc::new(PreparationVault {
        creates: AtomicUsize::new(0),
        unwraps: AtomicUsize::new(0),
    });
    let preparer =
        PgCredentialMaterialPreparer::new(Arc::new(RefusingInstallationPermit), Arc::clone(&vault));

    let result = preparer
        .prepare(
            &context(&fixture),
            preparation_request(&fixture, fixture.attachment_id, "permit-error-secret"),
        )
        .await;
    assert!(matches!(
        result,
        Err(ApplicationError::Unavailable(ref message))
            if message == "installation permit unavailable"
    ));
    assert_eq!(vault.counts(), (0, 0));
    let unchanged: (String, i64, i64) = sqlx::query_as(
        "SELECT intent.state,
                (SELECT COUNT(*) FROM credential_prepared_materials WHERE intent_id=intent.id),
                (SELECT COUNT(*) FROM credential_prepared_attachments WHERE intent_id=intent.id)
           FROM credential_key_creation_intents AS intent WHERE intent.id=$1",
    )
    .bind(fixture.intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unchanged, ("reserved".to_owned(), 0, 0));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn concurrent_credential_material_preparation_writes_one_frame_and_loser_uses_no_vault(
    pool: PgPool,
) {
    let fixture = Arc::new(fixture(&pool).await);
    reserve(&pool, &fixture).await.unwrap();
    let vault = Arc::new(PreparationVault {
        creates: AtomicUsize::new(0),
        unwraps: AtomicUsize::new(0),
    });
    let first = material_preparer(&pool, Arc::clone(&vault));
    let second = material_preparer(&pool, Arc::clone(&vault));
    let first_fixture = Arc::clone(&fixture);
    let second_fixture = Arc::clone(&fixture);
    let outcomes = tokio::time::timeout(std::time::Duration::from_secs(10), async move {
        let first_context = context(&first_fixture);
        let second_context = context(&second_fixture);
        tokio::join!(
            first.prepare(
                &first_context,
                preparation_request(&first_fixture, first_fixture.attachment_id, "race-secret")
            ),
            second.prepare(
                &second_context,
                preparation_request(&second_fixture, second_fixture.attachment_id, "race-secret")
            )
        )
    })
    .await
    .expect("credential preparation race exceeded its bounded timeout");
    assert_eq!(outcomes.0.unwrap(), outcomes.1.unwrap());
    assert_eq!(vault.counts(), (1, 1));
    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM credential_prepared_materials WHERE intent_id=$1")
            .bind(fixture.intent_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn credential_material_preparation_resumes_every_crash_boundary(pool: PgPool) {
    for point in [
        CredentialMaterialPreparationFaultPoint::AfterVaultCreate,
        CredentialMaterialPreparationFaultPoint::AfterProvisionalCreated,
        CredentialMaterialPreparationFaultPoint::AfterProvisionalReceipt,
        CredentialMaterialPreparationFaultPoint::AfterSeal,
        CredentialMaterialPreparationFaultPoint::AfterPrepared,
    ] {
        let fixture = fixture(&pool).await;
        reserve(&pool, &fixture).await.unwrap();
        let vault = Arc::new(PreparationVault {
            creates: AtomicUsize::new(0),
            unwraps: AtomicUsize::new(0),
        });
        let faulted = material_preparer_with_faults(
            &pool,
            Arc::clone(&vault),
            Arc::new(FailCredentialPreparationOnce {
                point,
                fired: std::sync::atomic::AtomicBool::new(false),
            }),
        );
        let first = faulted
            .prepare(
                &context(&fixture),
                preparation_request(&fixture, fixture.attachment_id, "resumable-secret"),
            )
            .await;
        assert!(
            matches!(first, Err(ApplicationError::Unavailable(_))),
            "{point:?}"
        );

        let resumed = material_preparer(&pool, Arc::clone(&vault))
            .prepare(
                &context(&fixture),
                preparation_request(&fixture, fixture.attachment_id, "resumable-secret"),
            )
            .await
            .unwrap();
        assert_eq!(
            resumed.prepared_attachment_id,
            Some(CredentialPreparedAttachmentId::from_uuid(
                fixture.attachment_id
            )),
            "{point:?}"
        );
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT
                (SELECT COUNT(*) FROM credential_prepared_materials WHERE intent_id=$1),
                (SELECT COUNT(*) FROM credential_prepared_attachments WHERE intent_id=$1)",
        )
        .bind(fixture.intent_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counts, (1, 1), "{point:?}");
    }
}

async fn record_provisional_created(pool: &PgPool, fixture: &Fixture) {
    execute_scoped(
        pool,
        fixture,
        sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
            .bind(fixture.intent_id),
    )
    .await
    .unwrap();
}

async fn record_provisional_receipt(pool: &PgPool, fixture: &Fixture, receipt: Uuid) {
    execute_scoped(
        pool,
        fixture,
        sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1, $2)")
            .bind(fixture.intent_id)
            .bind(receipt),
    )
    .await
    .unwrap();
}

async fn prepare(pool: &PgPool, fixture: &Fixture, ciphertext: &[u8]) {
    execute_scoped(
        pool,
        fixture,
        sqlx::query("SELECT vestrace_create_credential_prepared_material($1, $2, $3)")
            .bind(fixture.intent_id)
            .bind(fixture.attachment_id)
            .bind(ciphertext),
    )
    .await
    .unwrap();
}

async fn credential_prepared(pool: &PgPool) -> Fixture {
    let fixture = fixture(pool).await;
    reserve(pool, &fixture).await.unwrap();
    record_provisional_created(pool, &fixture).await;
    record_provisional_receipt(pool, &fixture, Uuid::now_v7()).await;
    prepare(pool, &fixture, b"encrypted credential material").await;
    fixture
}

async fn candidate(pool: &PgPool) -> Fixture {
    let fixture = credential_prepared(pool).await;
    execute_scoped(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_scoped(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
            .bind(fixture.intent_id),
    )
    .await
    .unwrap();
    fixture
}

async fn bound(pool: &PgPool) -> Fixture {
    let fixture = credential_prepared(pool).await;
    execute_scoped(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    fixture
}

async fn active(pool: &PgPool) -> Fixture {
    let fixture = candidate(pool).await;
    activate_candidate_as_guarded_owner(pool, &fixture, None, 0)
        .await
        .unwrap();
    fixture
}

async fn retire_active_as_guarded_owner(pool: &PgPool, revoked: bool) -> Fixture {
    let fixture = active(pool).await;
    let mut transaction = context_transaction(pool, &fixture).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    if revoked {
        sqlx::query(
            "UPDATE credential_slots
                SET current_revision_id=NULL,
                    current_revision_version=current_revision_version+1,
                    tombstone_version=current_revision_version+1,
                    tombstoned_at=NOW()
              WHERE workspace_id=$1 AND id=$2",
        )
        .bind(fixture.workspace_id)
        .bind(fixture.slot_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    } else {
        sqlx::query(
            "UPDATE credential_slots
                SET current_revision_id=NULL,
                    current_revision_version=current_revision_version+1,
                    updated_at=NOW()
              WHERE workspace_id=$1 AND id=$2",
        )
        .bind(fixture.workspace_id)
        .bind(fixture.slot_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE credential_key_creation_intents
            SET state='retired', updated_at=NOW()
          WHERE workspace_id=$1 AND id=$2",
    )
    .bind(fixture.workspace_id)
    .bind(fixture.intent_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    fixture
}

async fn credential_abandon_prepared(pool: &PgPool) -> Fixture {
    let fixture = credential_prepared(pool).await;
    prepare_abandon(pool, &fixture, 1).await.unwrap();
    fixture
}

async fn abandoned(pool: &PgPool) -> Fixture {
    let fixture = credential_abandon_prepared(pool).await;
    execute_scoped(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_credential_unbound_key_erasure($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_scoped(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_credential_key_abandon($1)").bind(fixture.intent_id),
    )
    .await
    .unwrap();
    fixture
}

async fn erasure_prepared(pool: &PgPool) -> (Fixture, Uuid) {
    let fixture = candidate(pool).await;
    let mut transaction = context_transaction(pool, &fixture).await;
    let preparation_id = sqlx::query_scalar(
        "SELECT preparation_id FROM vestrace_prepare_credential_material_erasure($1)",
    )
    .bind(fixture.intent_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    (fixture, preparation_id)
}

async fn destroyed(pool: &PgPool) -> Fixture {
    let (fixture, preparation_id) = erasure_prepared(pool).await;
    execute_scoped(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_material_erasure_fence($1, $2)")
            .bind(preparation_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_scoped(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_credential_material_erasure($1, $2)")
            .bind(preparation_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    fixture
}

async fn credential_replay_snapshot(pool: &PgPool, fixture: &Fixture) -> serde_json::Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
             'intent_state', intent.state,
             'intent_updated_at', intent.updated_at,
             'material_count', (SELECT COUNT(*) FROM credential_prepared_materials WHERE intent_id=intent.id),
             'attachment_count', (SELECT COUNT(*) FROM credential_prepared_attachments WHERE intent_id=intent.id),
             'occupancy_state', occupancy.state,
             'association_version', occupancy.association_version,
             'current_revision_id', slot.current_revision_id,
             'current_revision_version', slot.current_revision_version,
             'tombstone_version', slot.tombstone_version)
           FROM credential_key_creation_intents AS intent
           JOIN credential_guard_occupancies AS occupancy ON occupancy.id=intent.occupancy_id
           JOIN credential_slots AS slot ON slot.id=intent.credential_slot_id
          WHERE intent.id=$1",
    )
    .bind(fixture.intent_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn candidate_for_existing_slot(pool: &PgPool, existing: &Fixture) -> Fixture {
    let fixture = Fixture {
        workspace_id: existing.workspace_id,
        principal_id: existing.principal_id,
        connection_id: existing.connection_id,
        slot_id: existing.slot_id,
        occupancy_id: Uuid::now_v7(),
        intent_id: Uuid::now_v7(),
        revision_id: Uuid::now_v7(),
        material_key_id: Uuid::now_v7(),
        nonce: Uuid::now_v7(),
        attachment_id: Uuid::now_v7(),
    };
    execute_scoped(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1, $2, $3, $4)")
            .bind(fixture.occupancy_id)
            .bind(fixture.workspace_id)
            .bind(fixture.connection_id)
            .bind(fixture.slot_id),
    )
    .await
    .unwrap();
    reserve(pool, &fixture).await.unwrap();
    record_provisional_created(pool, &fixture).await;
    record_provisional_receipt(pool, &fixture, Uuid::now_v7()).await;
    prepare(pool, &fixture, b"second encrypted credential material").await;
    execute_scoped(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_scoped(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
            .bind(fixture.intent_id),
    )
    .await
    .unwrap();
    fixture
}

async fn activate_candidate_as_guarded_owner(
    pool: &PgPool,
    fixture: &Fixture,
    expected_current_revision_id: Option<Uuid>,
    expected_current_revision_version: i64,
) -> Result<(), sqlx::Error> {
    let mut transaction = context_transaction(pool, fixture).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("UPDATE credential_guard_occupancies SET state = 'activated' WHERE id = $1")
        .bind(fixture.occupancy_id)
        .execute(&mut *transaction)
        .await?;
    let slot_update = sqlx::query(
        "UPDATE credential_slots \
         SET current_revision_id = $1, current_revision_version = current_revision_version + 1, updated_at = NOW() \
         WHERE workspace_id = $2 AND id = $3 \
           AND current_revision_id IS NOT DISTINCT FROM $4 \
           AND current_revision_version = $5",
    )
    .bind(fixture.revision_id)
    .bind(fixture.workspace_id)
    .bind(fixture.slot_id)
    .bind(expected_current_revision_id)
    .bind(expected_current_revision_version)
    .execute(&mut *transaction)
    .await?;
    assert_eq!(
        slot_update.rows_affected(),
        1,
        "test-only activation construction must use the expected slot CAS"
    );
    sqlx::query("UPDATE credential_key_creation_intents SET state = 'active' WHERE id = $1")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await
}

async fn prepare_abandon(
    pool: &PgPool,
    fixture: &Fixture,
    expected_version: i64,
) -> Result<(), sqlx::Error> {
    execute_scoped(
        pool,
        fixture,
        sqlx::query("SELECT vestrace_prepare_credential_pre_live_abandon($1, $2)")
            .bind(fixture.intent_id)
            .bind(expected_version),
    )
    .await
}

async fn intent_state(pool: &PgPool, fixture: &Fixture) -> String {
    sqlx::query_scalar("SELECT state FROM credential_key_creation_intents WHERE id = $1")
        .bind(fixture.intent_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn candidate_is_visible(pool: &PgPool, fixture: &Fixture) -> bool {
    let mut transaction = context_transaction(pool, fixture).await;
    let candidate = sqlx::query_scalar("SELECT vestrace_credential_revision_is_candidate($1)")
        .bind(fixture.revision_id)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    candidate
}

async fn candidate_count(pool: &PgPool, fixture: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM credential_lifecycle_events WHERE intent_id = $1")
        .bind(fixture.intent_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn erasure_receipt_count(pool: &PgPool, fixture: &Fixture) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_key_creation_intent_erasure_receipts WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn credential_side_table_counts(pool: &PgPool, fixture: &Fixture) -> (i64, i64, i64, i64) {
    let prepared_materials = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_prepared_materials WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let prepared_attachments = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_prepared_attachments WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let association_events = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_association_events WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let lifecycle_events =
        sqlx::query_scalar("SELECT COUNT(*) FROM credential_lifecycle_events WHERE intent_id = $1")
            .bind(fixture.intent_id)
            .fetch_one(pool)
            .await
            .unwrap();
    (
        prepared_materials,
        prepared_attachments,
        association_events,
        lifecycle_events,
    )
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn closed_sequence_is_enforced(pool: PgPool) {
    let fixture = fixture(&pool).await;
    reserve(&pool, &fixture).await.unwrap();
    assert_sqlstate(
        execute_scoped(
            &pool,
            &fixture,
            sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1, $2)")
                .bind(fixture.intent_id)
                .bind(Uuid::now_v7()),
        )
        .await,
        "23514",
        "an intent skipped ProvisionalCreated on its way to ProvisionalReceipted",
    );

    record_provisional_created(&pool, &fixture).await;
    let provisional_created_counts = credential_side_table_counts(&pool, &fixture).await;
    assert_constraint_message(
        execute_scoped(
            &pool,
            &fixture,
            sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
                .bind(fixture.intent_id),
        )
        .await,
        "credential key creation intent must be Reserved",
        "a ProvisionalCreated credential intent recorded ProvisionalCreated again",
    );
    assert_eq!(intent_state(&pool, &fixture).await, "provisional_created");
    assert_eq!(
        credential_side_table_counts(&pool, &fixture).await,
        provisional_created_counts
    );

    record_provisional_receipt(&pool, &fixture, Uuid::now_v7()).await;
    prepare(&pool, &fixture, b"encrypted credential material").await;
    let credential_prepared_counts = credential_side_table_counts(&pool, &fixture).await;
    assert_constraint_message(
        execute_scoped(
            &pool,
            &fixture,
            sqlx::query("SELECT vestrace_create_credential_prepared_material($1, $2, $3)")
                .bind(fixture.intent_id)
                .bind(fixture.attachment_id)
                .bind(b"second encrypted credential material".as_slice()),
        )
        .await,
        "credential key creation intent must be ProvisionalReceipted",
        "a CredentialPrepared intent created prepared credential material again",
    );
    assert_eq!(intent_state(&pool, &fixture).await, "credential_prepared");
    assert_eq!(
        credential_side_table_counts(&pool, &fixture).await,
        credential_prepared_counts
    );

    let reserved_bind = crate::fixture(&pool).await;
    reserve(&pool, &reserved_bind).await.unwrap();
    let reserved_bind_counts = credential_side_table_counts(&pool, &reserved_bind).await;
    assert_constraint_message(
        execute_scoped(
            &pool,
            &reserved_bind,
            sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
                .bind(reserved_bind.intent_id)
                .bind(Uuid::now_v7()),
        )
        .await,
        "only CredentialPrepared may bind a credential key",
        "a Reserved credential intent bound a credential key",
    );
    assert_eq!(intent_state(&pool, &reserved_bind).await, "reserved");
    assert_eq!(
        credential_side_table_counts(&pool, &reserved_bind).await,
        reserved_bind_counts
    );

    execute_scoped(
        &pool,
        &fixture,
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_scoped(
        &pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
            .bind(fixture.intent_id),
    )
    .await
    .unwrap();

    assert_eq!(intent_state(&pool, &fixture).await, "candidate");
    assert_eq!(candidate_count(&pool, &fixture).await, 1);
    assert!(candidate_is_visible(&pool, &fixture).await);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn abort_branch_never_creates_candidate(pool: PgPool) {
    let fixture = credential_prepared(&pool).await;
    prepare_abandon(&pool, &fixture, 1).await.unwrap();

    assert_eq!(
        intent_state(&pool, &fixture).await,
        "credential_abandon_prepared"
    );
    assert_eq!(candidate_count(&pool, &fixture).await, 0);
    assert!(
        !candidate_is_visible(&pool, &fixture).await,
        "a pre-live abort must never make a credential revision visible as Candidate"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn abort_branch_never_creates_erasure_prepared(pool: PgPool) {
    let fixture = credential_prepared(&pool).await;
    prepare_abandon(&pool, &fixture, 1).await.unwrap();

    assert_eq!(erasure_receipt_count(&pool, &fixture).await, 0);
    let event_kinds: Vec<String> = sqlx::query_scalar(
        "SELECT event_kind FROM credential_lifecycle_events WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(event_kinds.is_empty());
    let allowed_event_kinds: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT event_kind FROM credential_lifecycle_events ORDER BY event_kind",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        allowed_event_kinds.iter().all(|kind| kind == "candidate"),
        "Task 11's lifecycle schema has no ErasurePrepared fact"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn prepared_row_cannot_jump_to_abandoned(pool: PgPool) {
    let fixture = credential_prepared(&pool).await;
    assert_sqlstate(
        execute_scoped(
            &pool,
            &fixture,
            sqlx::query("SELECT vestrace_finalize_credential_key_abandon($1)")
                .bind(fixture.intent_id),
        )
        .await,
        "23514",
        "CredentialPrepared finalized as Abandoned without CredentialAbandonPrepared",
    );
    assert_sqlstate(
        sqlx::query("UPDATE credential_key_creation_intents SET state = 'abandoned' WHERE id = $1")
            .bind(fixture.intent_id)
            .execute(&pool)
            .await,
        "42501",
        "raw SQL jumped CredentialPrepared directly to Abandoned",
    );
    assert_eq!(intent_state(&pool, &fixture).await, "credential_prepared");
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn sql_rollback_does_not_erase_a_key_already_created_in_the_vault(pool: PgPool) {
    let fixture = fixture(&pool).await;
    reserve(&pool, &fixture).await.unwrap();
    record_provisional_created(&pool, &fixture).await;

    let vault_fixture = VaultFixture::new();
    let vault = vault_fixture.vault();
    let key_id = MaterialKeyId::from_uuid(fixture.material_key_id);
    let nonce = IntentNonce::from_uuid(fixture.nonce);
    let receipt = vault
        .create_if_absent(key_id, nonce)
        .expect("the host vault creates independently of PostgreSQL");

    let mut transaction = context_transaction(&pool, &fixture).await;
    sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1, $2)")
        .bind(fixture.intent_id)
        .bind(receipt.as_uuid())
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.rollback().await.unwrap();

    let mut exposed = false;
    vault
        .unwrap(key_id, &mut |_| exposed = true)
        .expect("a SQL rollback must not erase a key already created in the external vault");
    assert!(exposed);
    assert_eq!(intent_state(&pool, &fixture).await, "provisional_created");
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn revision_identity_is_allocated_before_encryption(pool: PgPool) {
    let fixture = fixture(&pool).await;
    reserve(&pool, &fixture).await.unwrap();
    let allocated_revision: Uuid = sqlx::query_scalar(
        "SELECT credential_revision_id FROM credential_key_creation_intents WHERE id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(allocated_revision, fixture.revision_id);

    let vault_fixture = VaultFixture::new();
    let vault = vault_fixture.vault();
    let key_id = MaterialKeyId::from_uuid(fixture.material_key_id);
    let receipt = vault
        .create_if_absent(key_id, IntentNonce::from_uuid(fixture.nonce))
        .expect("create provisional vault key after identity allocation");
    record_provisional_created(&pool, &fixture).await;
    record_provisional_receipt(&pool, &fixture, receipt.as_uuid()).await;

    let cipher = EnvelopeCipher::new();
    let mut ciphertext = None;
    vault
        .unwrap(key_id, &mut |dek| {
            ciphertext = Some(dek.expose(|key| {
                cipher
                    .seal(key, b"credential-intent-v1", b"credential plaintext")
                    .expect("encrypt credential bytes")
                    .ciphertext
            }));
        })
        .expect("use provisional vault key for encryption");
    prepare(
        &pool,
        &fixture,
        ciphertext
            .as_deref()
            .expect("encryption produced ciphertext"),
    )
    .await;

    let prepared_revision: Uuid = sqlx::query_scalar(
        "SELECT credential_revision_id FROM credential_prepared_materials WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(prepared_revision, allocated_revision);
    assert!(
        !candidate_is_visible(&pool, &fixture).await,
        "allocated identity is still not visible before the exact Bound finalizer"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn concurrent_creation_returns_a_typed_conflict(pool: PgPool) {
    let fixture = Arc::new(fixture(&pool).await);
    let database = pool.clone();
    let repository = PgCredentialIntentRepository::new(PgStore::from_pool(pool));
    let barrier = Arc::new(Barrier::new(2));
    let create = |intent_id: Uuid, revision_id: Uuid, material_key_id: Uuid, nonce: Uuid| {
        let repository = repository.clone();
        let fixture = fixture.clone();
        let barrier = barrier.clone();
        async move {
            barrier.wait().await;
            repository
                .reserve(
                    &context(&fixture),
                    &CredentialKeyCreationIntent::reserve(
                        CredentialKeyCreationIntentId::from_uuid(intent_id),
                        WorkspaceId::from_uuid(fixture.workspace_id),
                        ConnectionId::from_uuid(fixture.connection_id),
                        CredentialSlotId::from_uuid(fixture.slot_id),
                        fixture.occupancy_id,
                        CredentialRevisionId::from_uuid(revision_id),
                        MaterialKeyId::from_uuid(material_key_id),
                        IntentNonce::from_uuid(nonce),
                    ),
                )
                .await
        }
    };
    let first = tokio::spawn(create(
        fixture.intent_id,
        fixture.revision_id,
        fixture.material_key_id,
        fixture.nonce,
    ));
    let second_intent_id = Uuid::now_v7();
    let second_revision_id = Uuid::now_v7();
    let second_material_key_id = Uuid::now_v7();
    let second_nonce = Uuid::now_v7();
    let second = tokio::spawn(create(
        second_intent_id,
        second_revision_id,
        second_material_key_id,
        second_nonce,
    ));
    let results = [first.await.unwrap(), second.await.unwrap()];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(results.iter().any(|result| {
        matches!(
            result,
            Err(ApplicationError::Conflict(message)) if message == "CREDENTIAL_GUARD_OCCUPIED"
        )
    }));
    let reserved_revision: Uuid = sqlx::query_scalar(
        "SELECT credential_revision_id FROM credential_key_creation_intents WHERE occupancy_id = $1",
    )
    .bind(fixture.occupancy_id)
    .fetch_one(&database)
    .await
    .unwrap();
    assert!(
        [fixture.revision_id, second_revision_id].contains(&reserved_revision),
        "exactly one preallocated revision identity won before any encryption could begin"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn raw_sql_cannot_forge_a_prepared_attachment(pool: PgPool) {
    let fixture = credential_prepared(&pool).await;
    assert_sqlstate(
        sqlx::query(
            "INSERT INTO credential_prepared_attachments \
             (id, workspace_id, intent_id, credential_revision_id) VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::now_v7())
        .bind(fixture.workspace_id)
        .bind(fixture.intent_id)
        .bind(fixture.revision_id)
        .execute(&pool)
        .await,
        "42501",
        "raw SQL forged a CredentialPrepared attachment",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn raw_sql_cannot_forge_a_candidate_event_or_bound_receipt(pool: PgPool) {
    let fixture = credential_prepared(&pool).await;
    assert_sqlstate(
        sqlx::query(
            "INSERT INTO credential_lifecycle_events \
             (id, workspace_id, intent_id, credential_revision_id, event_kind) \
             VALUES ($1, $2, $3, $4, 'candidate')",
        )
        .bind(Uuid::now_v7())
        .bind(fixture.workspace_id)
        .bind(fixture.intent_id)
        .bind(fixture.revision_id)
        .execute(&pool)
        .await,
        "42501",
        "raw SQL forged a Candidate lifecycle event",
    );
    assert_sqlstate(
        sqlx::query("UPDATE credential_key_creation_intents SET bound_receipt = $1 WHERE id = $2")
            .bind(Uuid::now_v7())
            .bind(fixture.intent_id)
            .execute(&pool)
            .await,
        "42501",
        "raw SQL forged a bound receipt",
    );
    assert!(
        !candidate_is_visible(&pool, &fixture).await,
        "a prepared identity cannot be made visible by raw SQL"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn raw_sql_cannot_forge_a_cancellation_or_visible_destroyed_identity(pool: PgPool) {
    let fixture = credential_prepared(&pool).await;
    prepare_abandon(&pool, &fixture, 1).await.unwrap();
    execute_scoped(
        &pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_credential_unbound_key_erasure($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_scoped(
        &pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_credential_key_abandon($1)").bind(fixture.intent_id),
    )
    .await
    .unwrap();

    assert_sqlstate(
        sqlx::query(
            "INSERT INTO credential_association_events \
             (id, workspace_id, occupancy_id, intent_id, event_kind, expected_version, resulting_version) \
             VALUES ($1, $2, $3, $4, 'credential_association_cancelled', 3, 4)",
        )
        .bind(Uuid::now_v7())
        .bind(fixture.workspace_id)
        .bind(fixture.occupancy_id)
        .bind(fixture.intent_id)
        .execute(&pool)
        .await,
        "42501",
        "raw SQL forged a CredentialAssociationCancelled event",
    );
    assert_sqlstate(
        sqlx::query("UPDATE credential_key_creation_intents SET state = 'candidate' WHERE id = $1")
            .bind(fixture.intent_id)
            .execute(&pool)
            .await,
        "42501",
        "raw SQL made a destroyed identity visible as Candidate",
    );
    assert!(
        !candidate_is_visible(&pool, &fixture).await,
        "a destroyed identity is never visible as a credential Candidate"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn occupancy_releases_only_after_cancelled_plus_witnessed_terminal_receipt(pool: PgPool) {
    let fixture = credential_prepared(&pool).await;
    prepare_abandon(&pool, &fixture, 1).await.unwrap();
    assert_sqlstate(
        execute_scoped(
            &pool,
            &fixture,
            sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1, $2, $3, $4)")
                .bind(Uuid::now_v7())
                .bind(fixture.workspace_id)
                .bind(fixture.connection_id)
                .bind(fixture.slot_id),
        )
        .await,
        "23505",
        "a Cancelled association released its occupancy before a terminal receipt",
    );

    execute_scoped(
        &pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_credential_unbound_key_erasure($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_scoped(
        &pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_credential_key_abandon($1)").bind(fixture.intent_id),
    )
    .await
    .unwrap();
    assert_eq!(intent_state(&pool, &fixture).await, "abandoned");
    assert_eq!(erasure_receipt_count(&pool, &fixture).await, 1);
    execute_scoped(
        &pool,
        &fixture,
        sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1, $2, $3, $4)")
            .bind(Uuid::now_v7())
            .bind(fixture.workspace_id)
            .bind(fixture.connection_id)
            .bind(fixture.slot_id),
    )
    .await
    .expect("the terminal cancellation receipt releases the permanent guard occupancy");
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn pre_live_abort_requires_the_expected_association_version(pool: PgPool) {
    let fixture = credential_prepared(&pool).await;
    assert_sqlstate(
        prepare_abandon(&pool, &fixture, 2).await,
        "23514",
        "an abort used a stale association expected version",
    );
    assert_eq!(intent_state(&pool, &fixture).await, "credential_prepared");
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn pre_live_abort_accepts_each_pre_live_state(pool: PgPool) {
    let reserved = fixture(&pool).await;
    reserve(&pool, &reserved).await.unwrap();
    prepare_abandon(&pool, &reserved, 1).await.unwrap();

    let provisional_created = fixture(&pool).await;
    reserve(&pool, &provisional_created).await.unwrap();
    record_provisional_created(&pool, &provisional_created).await;
    prepare_abandon(&pool, &provisional_created, 1)
        .await
        .unwrap();

    let provisional_receipted = fixture(&pool).await;
    reserve(&pool, &provisional_receipted).await.unwrap();
    record_provisional_created(&pool, &provisional_receipted).await;
    record_provisional_receipt(&pool, &provisional_receipted, Uuid::now_v7()).await;
    prepare_abandon(&pool, &provisional_receipted, 1)
        .await
        .unwrap();

    let prepared = credential_prepared(&pool).await;
    prepare_abandon(&pool, &prepared, 1).await.unwrap();

    for fixture in [
        &reserved,
        &provisional_created,
        &provisional_receipted,
        &prepared,
    ] {
        assert_eq!(
            intent_state(&pool, fixture).await,
            "credential_abandon_prepared"
        );
        let cancellations: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM credential_association_events WHERE intent_id = $1",
        )
        .bind(fixture.intent_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cancellations, 1);
    }
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn pre_live_abort_refusal_matrix_observes_its_state_and_association_guards(pool: PgPool) {
    let bound = credential_prepared(&pool).await;
    execute_scoped(
        &pool,
        &bound,
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
            .bind(bound.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    let bound_counts = credential_side_table_counts(&pool, &bound).await;
    assert_constraint_message(
        prepare_abandon(&pool, &bound, 1).await,
        "only a pre-live credential intent without facts may prepare abandonment",
        "a Bound credential intent prepared a pre-live abandonment",
    );
    assert_eq!(intent_state(&pool, &bound).await, "bound");
    assert_eq!(
        credential_side_table_counts(&pool, &bound).await,
        bound_counts
    );

    let candidate = credential_prepared(&pool).await;
    execute_scoped(
        &pool,
        &candidate,
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
            .bind(candidate.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_scoped(
        &pool,
        &candidate,
        sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
            .bind(candidate.intent_id),
    )
    .await
    .unwrap();
    let candidate_counts = credential_side_table_counts(&pool, &candidate).await;
    assert_constraint_message(
        prepare_abandon(&pool, &candidate, 1).await,
        "credential association is no longer the expected Preparing version",
        "a Candidate credential intent prepared a pre-live abandonment",
    );
    assert_eq!(intent_state(&pool, &candidate).await, "candidate");
    assert_eq!(
        credential_side_table_counts(&pool, &candidate).await,
        candidate_counts
    );

    let abandoned = credential_prepared(&pool).await;
    prepare_abandon(&pool, &abandoned, 1).await.unwrap();
    execute_scoped(
        &pool,
        &abandoned,
        sqlx::query("SELECT vestrace_record_credential_unbound_key_erasure($1, $2)")
            .bind(abandoned.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_scoped(
        &pool,
        &abandoned,
        sqlx::query("SELECT vestrace_finalize_credential_key_abandon($1)")
            .bind(abandoned.intent_id),
    )
    .await
    .unwrap();
    let abandoned_counts = credential_side_table_counts(&pool, &abandoned).await;
    assert_constraint_message(
        prepare_abandon(&pool, &abandoned, 1).await,
        "credential association is no longer the expected Preparing version",
        "an Abandoned credential intent prepared a pre-live abandonment",
    );
    assert_eq!(intent_state(&pool, &abandoned).await, "abandoned");
    assert_eq!(
        credential_side_table_counts(&pool, &abandoned).await,
        abandoned_counts
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn pre_live_abort_state_clause_is_independently_reachable(pool: PgPool) {
    let fixture = credential_prepared(&pool).await;
    let mut transaction = context_transaction(&pool, &fixture).await;

    sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
        .bind(fixture.intent_id)
        .bind(Option::<Uuid>::None)
        .execute(&mut *transaction)
        .await
        .unwrap();

    let state: String =
        sqlx::query_scalar("SELECT state FROM credential_key_creation_intents WHERE id = $1")
            .bind(fixture.intent_id)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    let bound_receipt: Option<Uuid> = sqlx::query_scalar(
        "SELECT bound_receipt FROM credential_key_creation_intents WHERE id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    let (occupancy_state, association_version): (String, i64) = sqlx::query_as(
        "SELECT state, association_version FROM credential_guard_occupancies WHERE id = $1",
    )
    .bind(fixture.occupancy_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    let lifecycle_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM credential_lifecycle_events WHERE intent_id = $1")
            .bind(fixture.intent_id)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    let erasure_receipts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_key_creation_intent_erasure_receipts WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();

    assert_eq!(state, "bound");
    assert_eq!(bound_receipt, None);
    assert_eq!(occupancy_state, "preparing");
    assert_eq!(association_version, 1);
    assert_eq!(lifecycle_events, 0);
    assert_eq!(erasure_receipts, 0);
    assert_constraint_message(
        sqlx::query("SELECT vestrace_prepare_credential_pre_live_abandon($1, $2)")
            .bind(fixture.intent_id)
            .bind(1_i64)
            .execute(&mut *transaction)
            .await,
        "only a pre-live credential intent without facts may prepare abandonment",
        "the Bound-without-receipt state reached the state-list refusal in isolation",
    );

    transaction.rollback().await.unwrap();
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn abandonment_requires_a_witnessed_unbound_key_erase(pool: PgPool) {
    let fixture = credential_prepared(&pool).await;
    prepare_abandon(&pool, &fixture, 1).await.unwrap();
    assert_sqlstate(
        execute_scoped(
            &pool,
            &fixture,
            sqlx::query("SELECT vestrace_finalize_credential_key_abandon($1)")
                .bind(fixture.intent_id),
        )
        .await,
        "23514",
        "CredentialAbandonPrepared finalized without a witnessed unbound-key erase",
    );
    execute_scoped(
        &pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_credential_unbound_key_erasure($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_scoped(
        &pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_credential_key_abandon($1)").bind(fixture.intent_id),
    )
    .await
    .unwrap();
    assert_eq!(intent_state(&pool, &fixture).await, "abandoned");
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn active_releases_the_guard_but_cannot_duplicate_an_untouched_live_slot(pool: PgPool) {
    let first = candidate(&pool).await;
    activate_candidate_as_guarded_owner(&pool, &first, None, 0)
        .await
        .unwrap();
    let current_slot: (Option<Uuid>, i64) = sqlx::query_as(
        "SELECT current_revision_id, current_revision_version \
         FROM credential_slots WHERE workspace_id = $1 AND id = $2",
    )
    .bind(first.workspace_id)
    .bind(first.slot_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(current_slot, (Some(first.revision_id), 1));

    let second = candidate_for_existing_slot(&pool, &first).await;
    // The second transaction must not update `first`: that would mask the
    // row-trigger hole the Active partial unique index closes.
    assert_sqlstate(
        activate_candidate_as_guarded_owner(&pool, &second, Some(first.revision_id), 1).await,
        "23505",
        "a second Active intent committed after the first Active intent was left untouched",
    );
    assert_eq!(intent_state(&pool, &first).await, "active");
    assert_eq!(intent_state(&pool, &second).await, "candidate");
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn active_requires_its_exact_current_slot_pointer_at_commit(pool: PgPool) {
    let fixture = candidate(&pool).await;
    let mut transaction = context_transaction(&pool, &fixture).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("UPDATE credential_guard_occupancies SET state = 'activated' WHERE id = $1")
        .bind(fixture.occupancy_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("UPDATE credential_key_creation_intents SET state = 'active' WHERE id = $1")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();

    assert_constraint_message(
        transaction.commit().await,
        "Active requires exact Candidate publication and current slot pointer",
        "an Active intent committed without its exact current slot pointer",
    );
}
