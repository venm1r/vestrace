use std::{
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use chrono::{Duration, Utc};
use sqlx::{
    PgPool, Postgres, Row, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;
use vestrace_application::{
    FenceReceipt, MaterialErasureService, MaterialKeyVault, RequestContext, VaultError,
};
use vestrace_domain::{
    ContentMaterialId, ErasureReceipt, IntentNonce, MaterialKeyId, PrincipalId, VaultReceipt,
    WorkspaceId, ZeroizingDek,
};
use vestrace_infrastructure::{PgMaterialErasureRepository, PgStore};

struct ContentFixture {
    workspace_id: Uuid,
    principal_id: Uuid,
    intent_id: Uuid,
    material_id: Uuid,
    material_key_id: Uuid,
    nonce: Uuid,
    attachment_id: Uuid,
}

struct CredentialFixture {
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

#[derive(Default)]
struct VaultCounters {
    prepare_calls: AtomicUsize,
    erase_calls: AtomicUsize,
}

struct CountingVault {
    counters: Arc<VaultCounters>,
}

impl MaterialKeyVault for CountingVault {
    fn create_if_absent(
        &self,
        _key_id: MaterialKeyId,
        _nonce: IntentNonce,
    ) -> Result<VaultReceipt, VaultError> {
        Err(VaultError::Unavailable)
    }

    fn unwrap(
        &self,
        _key_id: MaterialKeyId,
        _use_dek: &mut dyn FnMut(&ZeroizingDek),
    ) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }

    fn prepare_erasure(&self, _key_id: MaterialKeyId) -> Result<FenceReceipt, VaultError> {
        self.counters.prepare_calls.fetch_add(1, Ordering::SeqCst);
        Ok(FenceReceipt::from_uuid(Uuid::from_u128(1)))
    }

    fn erase(&self, _key_id: MaterialKeyId) -> Result<ErasureReceipt, VaultError> {
        self.counters.erase_calls.fetch_add(1, Ordering::SeqCst);
        Ok(ErasureReceipt::from_uuid(Uuid::from_u128(2)))
    }
}

fn assert_sqlstate<T>(result: Result<T, sqlx::Error>, expected: &str, operation: &str) {
    let error = match result {
        Ok(_) => panic!("{operation} unexpectedly succeeded"),
        Err(error) => error,
    };
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

fn assert_constraint_message<T>(
    result: Result<T, sqlx::Error>,
    expected_message: &str,
    operation: &str,
) {
    let error = match result {
        Ok(_) => panic!("{operation} unexpectedly succeeded"),
        Err(error) => error,
    };
    let database_error = error
        .as_database_error()
        .expect("the one-way trigger must return a database error");
    assert_eq!(database_error.code().as_deref(), Some("23514"));
    assert!(
        database_error.message().contains(expected_message),
        "{operation} must return the material-intent erasure-tail trigger message {expected_message:?}, got {:?}",
        database_error.message()
    );
}

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_database_url = std::env::var("VESTRACE_RUNTIME_DATABASE_URL")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must authenticate as the restricted runtime role");
    PgConnectOptions::from_str(&runtime_database_url)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must be a PostgreSQL URL");
    let authority = runtime_database_url
        .split_once("://")
        .expect("VESTRACE_RUNTIME_DATABASE_URL must include a scheme")
        .1;
    let credentials = authority
        .rsplit_once('@')
        .expect("VESTRACE_RUNTIME_DATABASE_URL must include runtime credentials")
        .0;
    let (username, password) = credentials
        .split_once(':')
        .expect("VESTRACE_RUNTIME_DATABASE_URL must include a runtime password");
    assert_eq!(username, "vestrace");

    let options = source
        .connect_options()
        .as_ref()
        .clone()
        .username(username)
        .password(password);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("the runtime role must connect to the per-test migrated database");
    let role = sqlx::query(
        "SELECT current_user::text AS role, rolsuper, rolbypassrls \
         FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(role.get::<String, _>("role"), "vestrace");
    assert!(!role.get::<bool, _>("rolsuper"));
    assert!(!role.get::<bool, _>("rolbypassrls"));
    pool
}

fn content_context(fixture: &ContentFixture) -> RequestContext {
    RequestContext::new(
        WorkspaceId::from_uuid(fixture.workspace_id),
        PrincipalId::from_uuid(fixture.principal_id),
    )
}

async fn content_transaction<'a>(
    pool: &'a PgPool,
    fixture: &ContentFixture,
) -> Transaction<'a, Postgres> {
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

async fn credential_transaction<'a>(
    pool: &'a PgPool,
    fixture: &CredentialFixture,
) -> Transaction<'a, Postgres> {
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

async fn execute_content(
    pool: &PgPool,
    fixture: &ContentFixture,
    query: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
) -> Result<(), sqlx::Error> {
    let mut transaction = content_transaction(pool, fixture).await;
    match query.execute(&mut *transaction).await {
        Ok(_) => transaction.commit().await,
        Err(error) => Err(error),
    }
}

async fn execute_credential(
    pool: &PgPool,
    fixture: &CredentialFixture,
    query: sqlx::query::Query<'_, sqlx::Postgres, sqlx::postgres::PgArguments>,
) -> Result<(), sqlx::Error> {
    let mut transaction = credential_transaction(pool, fixture).await;
    match query.execute(&mut *transaction).await {
        Ok(_) => transaction.commit().await,
        Err(error) => Err(error),
    }
}

async fn live_content(pool: &PgPool) -> ContentFixture {
    let fixture = ContentFixture {
        workspace_id: Uuid::now_v7(),
        principal_id: Uuid::now_v7(),
        intent_id: Uuid::now_v7(),
        material_id: Uuid::now_v7(),
        material_key_id: Uuid::now_v7(),
        nonce: Uuid::now_v7(),
        attachment_id: Uuid::now_v7(),
    };
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(fixture.workspace_id)
        .bind(format!("erasure-content-{}", fixture.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(fixture.principal_id)
        .bind(fixture.workspace_id)
        .bind(format!(
            "erasure-content-principal-{}",
            fixture.principal_id
        ))
        .execute(pool)
        .await
        .unwrap();

    execute_content(
        pool,
        &fixture,
        sqlx::query(
            "SELECT vestrace_reserve_material_key_creation_intent($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(fixture.intent_id)
        .bind(fixture.workspace_id)
        .bind(fixture.material_id)
        .bind(fixture.material_key_id)
        .bind(fixture.nonce)
        .bind("content")
        .bind(fixture.principal_id)
        .bind(0_i64),
    )
    .await
    .unwrap();
    execute_content(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
            .bind(fixture.intent_id),
    )
    .await
    .unwrap();
    execute_content(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_content(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_prepare_content_material($1, $2, $3, $4)")
            .bind(fixture.intent_id)
            .bind(fixture.attachment_id)
            .bind(vec![0xA5_u8; 4096])
            .bind(4096_i64),
    )
    .await
    .unwrap();
    execute_content(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_content(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_bound_content_material($1)").bind(fixture.intent_id),
    )
    .await
    .unwrap();
    fixture
}

async fn prepare_content_erasure(pool: &PgPool, fixture: &ContentFixture) -> Uuid {
    let mut transaction = content_transaction(pool, fixture).await;
    let preparation_id: Uuid = sqlx::query_scalar(
        "SELECT preparation_id FROM vestrace_prepare_content_material_erasure($1)",
    )
    .bind(fixture.material_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    preparation_id
}

async fn fence_content_erasure(pool: &PgPool, fixture: &ContentFixture, preparation_id: Uuid) {
    execute_content(
        pool,
        fixture,
        sqlx::query("SELECT vestrace_record_material_erasure_fence($1, $2)")
            .bind(preparation_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
}

async fn tombstone_content(pool: &PgPool, fixture: &ContentFixture) -> (Uuid, Uuid) {
    let preparation_id = prepare_content_erasure(pool, fixture).await;
    fence_content_erasure(pool, fixture, preparation_id).await;
    let receipt = Uuid::now_v7();
    let mut transaction = content_transaction(pool, fixture).await;
    let final_receipt: Uuid =
        sqlx::query_scalar("SELECT vestrace_finalize_content_material_erasure($1, $2)")
            .bind(preparation_id)
            .bind(receipt)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(final_receipt, receipt);
    (preparation_id, receipt)
}

async fn candidate_credential(pool: &PgPool) -> CredentialFixture {
    let fixture = CredentialFixture {
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
        .bind(format!("erasure-credential-{}", fixture.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(fixture.principal_id)
        .bind(fixture.workspace_id)
        .bind(format!(
            "erasure-credential-principal-{}",
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
    .bind(format!("erasure-credential-connector-{connector_id}"))
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
        "erasure-credential-connection-{}",
        fixture.connection_id
    ))
    .execute(pool)
    .await
    .unwrap();

    let mut transaction = credential_transaction(pool, &fixture).await;
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

    execute_credential(
        pool,
        &fixture,
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
    .unwrap();
    execute_credential(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_credential_key_provisional_created($1)")
            .bind(fixture.intent_id),
    )
    .await
    .unwrap();
    execute_credential(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_credential_key_provisional_receipt($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_credential(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_create_credential_prepared_material($1, $2, $3)")
            .bind(fixture.intent_id)
            .bind(fixture.attachment_id)
            .bind(b"encrypted credential material".as_slice()),
    )
    .await
    .unwrap();
    execute_credential(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_bind_credential_key_creation_intent($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    execute_credential(
        pool,
        &fixture,
        sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
            .bind(fixture.intent_id),
    )
    .await
    .unwrap();
    fixture
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn live_cannot_return_from_erasure_prepared(pool: PgPool) {
    let fixture = live_content(&pool).await;
    prepare_content_erasure(&pool, &fixture).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM content_materials WHERE id = $1")
            .bind(fixture.material_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "erasure_prepared"
    );
    assert_sqlstate(
        execute_content(
            &pool,
            &fixture,
            sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
                .bind(fixture.intent_id),
        )
        .await,
        "23514",
        "a material erasure preparation returned to the Bound publication finalizer",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn tombstoned_cannot_return_to_live(pool: PgPool) {
    let fixture = live_content(&pool).await;
    tombstone_content(&pool, &fixture).await;
    assert_sqlstate(
        execute_content(
            &pool,
            &fixture,
            sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
                .bind(fixture.intent_id),
        )
        .await,
        "23514",
        "a tombstoned content material returned to Live publication",
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM content_materials WHERE id = $1")
            .bind(fixture.material_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "tombstoned"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn raw_sql_cannot_resurrect_a_tombstoned_identity(pool: PgPool) {
    let fixture = live_content(&pool).await;
    tombstone_content(&pool, &fixture).await;
    assert_sqlstate(
        sqlx::query("UPDATE content_materials SET state = 'live' WHERE id = $1")
            .bind(fixture.material_id)
            .execute(&pool)
            .await,
        "42501",
        "raw SQL resurrected a tombstoned content material",
    );
    assert_sqlstate(
        sqlx::query("UPDATE material_key_creation_intents SET state = 'live' WHERE id = $1")
            .bind(fixture.intent_id)
            .execute(&pool)
            .await,
        "42501",
        "raw SQL resurrected a tombstoned material-key identity",
    );

    let runtime = runtime_pool(&pool).await;
    let runtime_content_resurrection =
        sqlx::query("UPDATE content_materials SET state = 'live' WHERE id = $1")
            .bind(fixture.material_id)
            .execute(&runtime)
            .await;
    assert_sqlstate(
        runtime_content_resurrection,
        "42501",
        "the runtime role resurrected a tombstoned content material",
    );
    let runtime_intent_resurrection =
        sqlx::query("UPDATE material_key_creation_intents SET state = 'live' WHERE id = $1")
            .bind(fixture.intent_id)
            .execute(&runtime)
            .await;
    runtime.close().await;
    assert_sqlstate(
        runtime_intent_resurrection,
        "42501",
        "the runtime role resurrected a tombstoned material-key identity",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn content_erasure_keyed_operations_require_a_fenced_preparation_and_replay_its_receipt(
    pool: PgPool,
) {
    let fixture = live_content(&pool).await;
    let preparation_id = prepare_content_erasure(&pool, &fixture).await;
    let before_finalization: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
             (SELECT COUNT(*) FROM content_material_bytes WHERE material_id = $1), \
             (SELECT COUNT(*) FROM content_material_ordinary_references WHERE material_id = $1), \
             (SELECT COUNT(*) FROM material_erasure_events WHERE preparation_id = $2)",
    )
    .bind(fixture.material_id)
    .bind(preparation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_constraint_message(
        execute_content(
            &pool,
            &fixture,
            sqlx::query("SELECT vestrace_finalize_content_material_erasure($1, $2)")
                .bind(preparation_id)
                .bind(Uuid::now_v7()),
        )
        .await,
        "content material erasure requires its exact fenced preparation",
        "an unfenced preparation finalized content erasure",
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM content_materials WHERE id = $1")
            .bind(fixture.material_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "erasure_prepared"
    );
    let after_refusal: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
             (SELECT COUNT(*) FROM content_material_bytes WHERE material_id = $1), \
             (SELECT COUNT(*) FROM content_material_ordinary_references WHERE material_id = $1), \
             (SELECT COUNT(*) FROM material_erasure_events WHERE preparation_id = $2)",
    )
    .bind(fixture.material_id)
    .bind(preparation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after_refusal, before_finalization);

    let mut transaction = content_transaction(&pool, &fixture).await;
    let replay_preparation_id: Uuid = sqlx::query_scalar(
        "SELECT preparation_id FROM vestrace_prepare_content_material_erasure($1)",
    )
    .bind(fixture.material_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(replay_preparation_id, preparation_id);

    let fence_receipt = Uuid::now_v7();
    for _ in 0..2 {
        execute_content(
            &pool,
            &fixture,
            sqlx::query("SELECT vestrace_record_material_erasure_fence($1, $2)")
                .bind(preparation_id)
                .bind(fence_receipt),
        )
        .await
        .unwrap();
    }

    let erasure_receipt = Uuid::now_v7();
    for _ in 0..2 {
        let mut transaction = content_transaction(&pool, &fixture).await;
        let replayed_receipt: Uuid =
            sqlx::query_scalar("SELECT vestrace_finalize_content_material_erasure($1, $2)")
                .bind(preparation_id)
                .bind(erasure_receipt)
                .fetch_one(&mut *transaction)
                .await
                .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(replayed_receipt, erasure_receipt);
    }
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn replay_returns_the_original_receipt_without_a_second_erase(pool: PgPool) {
    let fixture = live_content(&pool).await;
    let repository = PgMaterialErasureRepository::new(PgStore::from_pool(pool.clone()));
    let counters = Arc::new(VaultCounters::default());
    let service = MaterialErasureService::new(
        repository,
        CountingVault {
            counters: Arc::clone(&counters),
        },
    );
    let context = content_context(&fixture);
    let first = service
        .erase_content(&context, ContentMaterialId::from_uuid(fixture.material_id))
        .await
        .unwrap();
    let second = service
        .erase_content(&context, ContentMaterialId::from_uuid(fixture.material_id))
        .await
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.as_uuid(), Uuid::from_u128(2));
    assert_eq!(counters.prepare_calls.load(Ordering::SeqCst), 1);
    assert_eq!(counters.erase_calls.load(Ordering::SeqCst), 1);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn blocker_refuses_preparation(pool: PgPool) {
    for (blocker_kind, usable_until) in [
        ("effect", None),
        ("lease", Some(Utc::now() + Duration::hours(1))),
        ("intent", None),
    ] {
        let fixture = live_content(&pool).await;
        execute_content(
            &pool,
            &fixture,
            sqlx::query(
                "SELECT vestrace_record_material_erasure_blocker($1, 'content', $2, NULL, $3, $4)",
            )
            .bind(Uuid::now_v7())
            .bind(fixture.material_id)
            .bind(blocker_kind)
            .bind(usable_until),
        )
        .await
        .unwrap();
        let mut transaction = content_transaction(&pool, &fixture).await;
        let result = sqlx::query("SELECT vestrace_prepare_content_material_erasure($1)")
            .bind(fixture.material_id)
            .execute(&mut *transaction)
            .await;
        assert_sqlstate(
            result,
            "23514",
            &format!("a nonterminal {blocker_kind} blocker allowed erasure preparation"),
        );
        transaction.rollback().await.unwrap();
    }
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn blocker_writer_refuses_after_waiting_for_content_erasure_preparation(pool: PgPool) {
    let fixture = live_content(&pool).await;
    let runtime = runtime_pool(&pool).await;

    // Phase one owns this row lock until its transaction commits. The runtime
    // writer therefore cannot read the old Live state before the preparation
    // becomes durable.
    let mut preparation = content_transaction(&pool, &fixture).await;
    sqlx::query("SELECT vestrace_prepare_content_material_erasure($1)")
        .bind(fixture.material_id)
        .execute(&mut *preparation)
        .await
        .unwrap();

    let (started, started_receiver) = tokio::sync::oneshot::channel();
    let workspace_id = fixture.workspace_id;
    let principal_id = fixture.principal_id;
    let material_id = fixture.material_id;
    let blocker = tokio::spawn(async move {
        let mut transaction = runtime.begin().await.unwrap();
        for (setting, value) in [
            ("vestrace.workspace_id", workspace_id.to_string()),
            ("vestrace.principal_id", principal_id.to_string()),
        ] {
            sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
                .bind(setting)
                .bind(value)
                .fetch_one(&mut *transaction)
                .await
                .unwrap();
        }
        started.send(()).unwrap();
        let result = sqlx::query(
            "SELECT vestrace_record_material_erasure_blocker($1, 'content', $2, NULL, 'effect', NULL)",
        )
        .bind(Uuid::now_v7())
        .bind(material_id)
        .execute(&mut *transaction)
        .await;
        match result {
            Ok(_) => transaction.commit().await,
            Err(error) => Err(error),
        }
    });
    started_receiver.await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        !blocker.is_finished(),
        "the independent runtime-role writer must wait on phase one's content row lock"
    );

    preparation.commit().await.unwrap();
    let result = blocker.await.unwrap();
    assert_sqlstate(
        result,
        "55000",
        "the runtime role recorded a blocker after content erasure preparation committed",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn metadata_and_audit_survive_erasure(pool: PgPool) {
    let fixture = live_content(&pool).await;
    let (preparation_id, _receipt) = tombstone_content(&pool, &fixture).await;
    let material_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM content_materials WHERE id = $1")
            .bind(fixture.material_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let intent_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM material_key_creation_intents WHERE id = $1")
            .bind(fixture.intent_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let reference_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM content_material_ordinary_references WHERE material_id = $1",
    )
    .bind(fixture.material_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let ciphertext_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM content_material_bytes WHERE material_id = $1")
            .bind(fixture.material_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let lifecycle_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM material_erasure_events WHERE preparation_id = $1",
    )
    .bind(preparation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    // The tombstone is evidence about an entry in the repository's own Audit
    // authority, so the assertion follows its foreign key into audit_events
    // rather than stopping at the table this migration created.
    let (audit_event_id, tombstone_receipt): (Uuid, Uuid) = sqlx::query_as(
        "SELECT audit_event_id, erasure_receipt FROM material_erasure_audit_tombstones \
         WHERE preparation_id = $1",
    )
    .bind(preparation_id)
    .fetch_one(&pool)
    .await
    .expect("the erasure must record exactly one audit tombstone");
    let (action, resource_type, resource_id, payload): (String, String, Uuid, serde_json::Value) =
        sqlx::query_as(
            "SELECT action, resource_type, resource_id, payload FROM audit_events WHERE id = $1",
        )
        .bind(audit_event_id)
        .fetch_one(&pool)
        .await
        .expect("the tombstone must reference a real audit event");

    assert_eq!((material_count, intent_count, reference_count), (1, 1, 1));
    assert_eq!(ciphertext_count, 0, "only ciphertext is removed");
    assert_eq!(lifecycle_count, 2);
    assert_eq!(action, "material.content.tombstoned");
    assert_eq!(resource_type, "content_material");
    assert_eq!(resource_id, fixture.material_id);
    assert_eq!(
        payload["erasure_receipt"].as_str().unwrap(),
        tombstone_receipt.to_string(),
        "the audit entry and the tombstone must name the same witnessed receipt"
    );
    assert_eq!(payload["size_class"].as_i64(), Some(4096));
    assert_eq!(
        payload["padded_storage_bytes_reclaimed"].as_i64(),
        Some(4096)
    );
    for forbidden in ["plaintext", "plaintext_length", "byte_count", "digest"] {
        assert!(
            payload.get(forbidden).is_none(),
            "the erasure audit payload must not carry {forbidden}"
        );
    }
}

/// The reject_raw triggers refuse an unprivileged caller, which proves nothing
/// about the guarded owner those triggers exempt. `SET ROLE` reaches that owner
/// even though it cannot log in, so this exercises the one-way trigger against
/// the only role that could otherwise reverse an erasure.
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn state_reversal_is_refused_for_the_guarded_owner(pool: PgPool) {
    let fixture = live_content(&pool).await;
    tombstone_content(&pool, &fixture).await;

    // The workspace context comes first: content_materials forces RLS, so
    // without it the statement would match no row and refuse nothing.
    let mut transaction = content_transaction(&pool, &fixture).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT current_user::text")
            .fetch_one(&mut *transaction)
            .await
            .unwrap(),
        "vestrace_guarded_owner",
        "the reversal must be attempted as the role the reject_raw trigger exempts"
    );
    let reversal = sqlx::query("UPDATE content_materials SET state = 'live' WHERE id = $1")
        .bind(fixture.material_id)
        .execute(&mut *transaction)
        .await;
    assert_sqlstate(
        reversal,
        "23514",
        "the guarded owner returned a tombstoned content material to Live",
    );
    drop(transaction);

    let mut transaction = content_transaction(&pool, &fixture).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    let deletion = sqlx::query("DELETE FROM content_materials WHERE id = $1")
        .bind(fixture.material_id)
        .execute(&mut *transaction)
        .await;
    assert_sqlstate(
        deletion,
        "23514",
        "the guarded owner deleted a tombstoned content material",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn guarded_owner_cannot_skip_content_erasure_preparation(pool: PgPool) {
    let fixture = live_content(&pool).await;
    let mut transaction = content_transaction(&pool, &fixture).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();

    let skip = sqlx::query("UPDATE content_materials SET state = 'tombstoned' WHERE id = $1")
        .bind(fixture.material_id)
        .execute(&mut *transaction)
        .await;
    assert_sqlstate(
        skip,
        "23514",
        "the guarded owner skipped Live -> ErasurePrepared before Tombstoned",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn guarded_owner_cannot_skip_credential_erasure_preparation(pool: PgPool) {
    let fixture = candidate_credential(&pool).await;
    let mut transaction = credential_transaction(&pool, &fixture).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();

    let skip =
        sqlx::query("UPDATE credential_key_creation_intents SET state = 'destroyed' WHERE id = $1")
            .bind(fixture.intent_id)
            .execute(&mut *transaction)
            .await;
    assert_sqlstate(
        skip,
        "23514",
        "the guarded owner skipped Candidate -> ErasurePrepared before Destroyed",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn guarded_owner_cannot_skip_material_intent_erasure_preparation(pool: PgPool) {
    let fixture = live_content(&pool).await;
    let mut transaction = content_transaction(&pool, &fixture).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    let result =
        sqlx::query("UPDATE material_key_creation_intents SET state = 'tombstoned' WHERE id = $1")
            .bind(fixture.intent_id)
            .execute(&mut *transaction)
            .await;
    assert_constraint_message(
        result,
        "material key creation intent may enter Tombstoned only from ErasurePrepared",
        "the guarded owner skipped Live -> ErasurePrepared before Tombstoned for a material intent",
    );
    drop(transaction);

    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM material_key_creation_intents WHERE id = $1",
        )
        .bind(fixture.intent_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "live"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn guarded_owner_cannot_reverse_a_tombstoned_material_intent(pool: PgPool) {
    let fixture = live_content(&pool).await;
    tombstone_content(&pool, &fixture).await;
    let mut transaction = content_transaction(&pool, &fixture).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    let result =
        sqlx::query("UPDATE material_key_creation_intents SET state = 'live' WHERE id = $1")
            .bind(fixture.intent_id)
            .execute(&mut *transaction)
            .await;
    assert_constraint_message(
        result,
        "Tombstoned is terminal for material key creation intent",
        "the guarded owner returned a tombstoned material intent to Live",
    );
    drop(transaction);

    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM material_key_creation_intents WHERE id = $1",
        )
        .bind(fixture.intent_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "tombstoned"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn guarded_owner_cannot_delete_a_tombstoned_material_intent(pool: PgPool) {
    let fixture = live_content(&pool).await;
    tombstone_content(&pool, &fixture).await;
    let mut transaction = content_transaction(&pool, &fixture).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    let result = sqlx::query("DELETE FROM material_key_creation_intents WHERE id = $1")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await;
    assert_constraint_message(
        result,
        "an erasure-prepared or tombstoned material key creation intent cannot be deleted",
        "the guarded owner deleted a tombstoned material intent",
    );
    drop(transaction);

    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM material_key_creation_intents WHERE id = $1",
        )
        .bind(fixture.intent_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "tombstoned"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn guarded_owner_cannot_reverse_an_erasure_prepared_material_intent(pool: PgPool) {
    let fixture = live_content(&pool).await;
    prepare_content_erasure(&pool, &fixture).await;
    let mut transaction = content_transaction(&pool, &fixture).await;
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();
    let result =
        sqlx::query("UPDATE material_key_creation_intents SET state = 'live' WHERE id = $1")
            .bind(fixture.intent_id)
            .execute(&mut *transaction)
            .await;
    assert_constraint_message(
        result,
        "ErasurePrepared material key creation intent may only advance to Tombstoned",
        "the guarded owner reversed an ErasurePrepared material intent",
    );
    drop(transaction);

    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM material_key_creation_intents WHERE id = $1",
        )
        .bind(fixture.intent_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "erasure_prepared"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn credential_candidate_can_only_advance_to_destroyed(pool: PgPool) {
    let fixture = candidate_credential(&pool).await;
    let mut transaction = credential_transaction(&pool, &fixture).await;
    let preparation_id: Uuid = sqlx::query_scalar(
        "SELECT preparation_id FROM vestrace_prepare_credential_material_erasure($1)",
    )
    .bind(fixture.intent_id)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    execute_credential(
        &pool,
        &fixture,
        sqlx::query("SELECT vestrace_record_material_erasure_fence($1, $2)")
            .bind(preparation_id)
            .bind(Uuid::now_v7()),
    )
    .await
    .unwrap();
    let receipt = Uuid::now_v7();
    let mut transaction = credential_transaction(&pool, &fixture).await;
    let final_receipt: Uuid =
        sqlx::query_scalar("SELECT vestrace_finalize_credential_material_erasure($1, $2)")
            .bind(preparation_id)
            .bind(receipt)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(final_receipt, receipt);
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM credential_key_creation_intents WHERE id = $1",
        )
        .bind(fixture.intent_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "destroyed"
    );
    assert_sqlstate(
        execute_credential(
            &pool,
            &fixture,
            sqlx::query("SELECT vestrace_finalize_bound_credential_candidate($1)")
                .bind(fixture.intent_id),
        )
        .await,
        "23514",
        "a destroyed credential returned to Candidate",
    );
    let ciphertext_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credential_prepared_materials WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ciphertext_count, 0);
}
