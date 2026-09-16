use std::str::FromStr;

use sqlx::{
    PgPool, Postgres, Row, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

const RUNTIME_ROLE: &str = "vestrace";
const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";

struct IntentFixture {
    workspace_id: Uuid,
    principal_id: Uuid,
    intent_id: Uuid,
    material_id: Uuid,
    material_key_id: Uuid,
    nonce: Uuid,
    attachment_id: Uuid,
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
        .expect("the guarded operation must return a database error");
    assert_eq!(database_error.code().as_deref(), Some("23514"));
    assert!(
        database_error.message().contains(expected_message),
        "{operation} must return {expected_message:?}, got {:?}",
        database_error.message(),
    );
}

fn runtime_credentials() -> (String, String) {
    let runtime_database_url = std::env::var(RUNTIME_DATABASE_URL_ENV)
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

    assert_eq!(username, RUNTIME_ROLE);
    (username.to_owned(), password.to_owned())
}

async fn runtime_pool(source: &PgPool) -> PgPool {
    let (username, password) = runtime_credentials();
    let options = source
        .connect_options()
        .as_ref()
        .clone()
        .username(&username)
        .password(&password);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("the runtime role must be able to connect to the migrated test database");
    let role = sqlx::query(
        "SELECT current_user::text AS role, rolsuper, rolbypassrls \
         FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&pool)
    .await
    .expect("the runtime role identity must be observable");
    assert_eq!(role.get::<String, _>("role"), RUNTIME_ROLE);
    assert!(!role.get::<bool, _>("rolsuper"));
    assert!(!role.get::<bool, _>("rolbypassrls"));
    pool
}

async fn context_transaction<'a>(
    pool: &'a PgPool,
    fixture: &IntentFixture,
) -> Transaction<'a, Postgres> {
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.workspace_id")
        .bind(fixture.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.principal_id")
        .bind(fixture.principal_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction
}

async fn reserve(pool: &PgPool) -> IntentFixture {
    let fixture = IntentFixture {
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
        .bind(format!("material-intent-{}", fixture.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(fixture.principal_id)
        .bind(fixture.workspace_id)
        .bind(format!(
            "material-intent-principal-{}",
            fixture.principal_id
        ))
        .execute(pool)
        .await
        .unwrap();

    let mut transaction = context_transaction(pool, &fixture).await;
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
    .bind(0_i64)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();

    fixture
}

async fn record_provisional_created(pool: &PgPool, fixture: &IntentFixture) {
    let mut transaction = context_transaction(pool, fixture).await;
    sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn record_provisional_receipt(pool: &PgPool, fixture: &IntentFixture) {
    let mut transaction = context_transaction(pool, fixture).await;
    sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1, $2)")
        .bind(fixture.intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn prepare_content(pool: &PgPool, fixture: &IntentFixture) {
    let mut transaction = context_transaction(pool, fixture).await;
    sqlx::query("SELECT vestrace_prepare_content_material($1, $2, $3, $4)")
        .bind(fixture.intent_id)
        .bind(fixture.attachment_id)
        .bind(vec![0x41_u8; 4096])
        .bind(4096_i64)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn prepare_result(pool: &PgPool, fixture: &IntentFixture) {
    let mut transaction = context_transaction(pool, fixture).await;
    sqlx::query("SELECT vestrace_prepare_result_material($1, $2, $3, $4)")
        .bind(fixture.intent_id)
        .bind(fixture.attachment_id)
        .bind(vec![0x52_u8; 4096])
        .bind(4096_i64)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn bind(pool: &PgPool, fixture: &IntentFixture) {
    let mut transaction = context_transaction(pool, fixture).await;
    sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1, $2)")
        .bind(fixture.intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn finalize_bound(pool: &PgPool, fixture: &IntentFixture) {
    let mut transaction = context_transaction(pool, fixture).await;
    sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn content_prepared(pool: &PgPool) -> IntentFixture {
    let fixture = reserve(pool).await;
    record_provisional_created(pool, &fixture).await;
    record_provisional_receipt(pool, &fixture).await;
    prepare_content(pool, &fixture).await;
    fixture
}

async fn intent_state(pool: &PgPool, fixture: &IntentFixture) -> String {
    sqlx::query_scalar("SELECT state FROM material_key_creation_intents WHERE id = $1")
        .bind(fixture.intent_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn material_is_live(pool: &PgPool, fixture: &IntentFixture) -> bool {
    let mut transaction = context_transaction(pool, fixture).await;
    let live = sqlx::query_scalar("SELECT vestrace_content_material_is_live($1)")
        .bind(fixture.material_id)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    live
}

async fn material_side_table_counts(
    pool: &PgPool,
    fixture: &IntentFixture,
) -> (i64, i64, i64, i64) {
    let content_materials =
        sqlx::query_scalar("SELECT COUNT(*) FROM content_materials WHERE intent_id = $1")
            .bind(fixture.intent_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let ciphertexts =
        sqlx::query_scalar("SELECT COUNT(*) FROM content_material_bytes WHERE intent_id = $1")
            .bind(fixture.intent_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let attachments = sqlx::query_scalar(
        "SELECT COUNT(*) FROM prepared_material_attachments WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let references = sqlx::query_scalar(
        "SELECT COUNT(*) FROM content_material_ordinary_references WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(pool)
    .await
    .unwrap();
    (content_materials, ciphertexts, attachments, references)
}

async fn assert_content_preparation_refusal(
    pool: &PgPool,
    fixture: &IntentFixture,
    expected_state: &str,
) {
    let before_counts = material_side_table_counts(pool, fixture).await;
    let mut transaction = context_transaction(pool, fixture).await;
    let refusal = sqlx::query("SELECT vestrace_prepare_content_material($1, $2, $3, $4)")
        .bind(fixture.intent_id)
        .bind(Uuid::now_v7())
        .bind(vec![0x41_u8; 4096])
        .bind(4096_i64)
        .execute(&mut *transaction)
        .await;
    assert_constraint_message(
        refusal,
        "material key creation intent must be ProvisionalReceipted",
        "content preparation from a non-ProvisionalReceipted origin",
    );
    transaction.rollback().await.unwrap();
    assert_eq!(intent_state(pool, fixture).await, expected_state);
    assert_eq!(
        material_side_table_counts(pool, fixture).await,
        before_counts
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn closed_sequence_is_enforced(pool: PgPool) {
    let fixture = reserve(&pool).await;
    let mut transaction = context_transaction(&pool, &fixture).await;
    let skipped_receipt =
        sqlx::query("SELECT vestrace_record_material_key_provisional_receipt($1, $2)")
            .bind(fixture.intent_id)
            .bind(Uuid::now_v7())
            .execute(&mut *transaction)
            .await;
    assert_sqlstate(
        skipped_receipt,
        "23514",
        "an intent skipped ProvisionalCreated on its way to ProvisionalReceipted",
    );
    transaction.rollback().await.unwrap();

    record_provisional_created(&pool, &fixture).await;
    record_provisional_receipt(&pool, &fixture).await;
    prepare_content(&pool, &fixture).await;
    bind(&pool, &fixture).await;
    finalize_bound(&pool, &fixture).await;

    assert_eq!(intent_state(&pool, &fixture).await, "live");
    assert!(material_is_live(&pool, &fixture).await);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn content_preparation_allowed_origin_matrix_observes_the_ordering_guard(pool: PgPool) {
    let provisional_created_repeat = reserve(&pool).await;
    record_provisional_created(&pool, &provisional_created_repeat).await;
    let provisional_created_repeat_counts =
        material_side_table_counts(&pool, &provisional_created_repeat).await;
    let mut transaction = context_transaction(&pool, &provisional_created_repeat).await;
    let repeat_provisional_created =
        sqlx::query("SELECT vestrace_record_material_key_provisional_created($1)")
            .bind(provisional_created_repeat.intent_id)
            .execute(&mut *transaction)
            .await;
    assert_constraint_message(
        repeat_provisional_created,
        "material key creation intent must be Reserved",
        "a ProvisionalCreated material intent recorded ProvisionalCreated again",
    );
    transaction.rollback().await.unwrap();
    assert_eq!(
        intent_state(&pool, &provisional_created_repeat).await,
        "provisional_created"
    );
    assert_eq!(
        material_side_table_counts(&pool, &provisional_created_repeat).await,
        provisional_created_repeat_counts
    );

    let reserved_bind = reserve(&pool).await;
    let reserved_bind_counts = material_side_table_counts(&pool, &reserved_bind).await;
    let mut transaction = context_transaction(&pool, &reserved_bind).await;
    let bind_reserved = sqlx::query("SELECT vestrace_bind_material_key_creation_intent($1, $2)")
        .bind(reserved_bind.intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await;
    assert_constraint_message(
        bind_reserved,
        "only prepared content may bind a material key",
        "a Reserved material intent bound a material key",
    );
    transaction.rollback().await.unwrap();
    assert_eq!(intent_state(&pool, &reserved_bind).await, "reserved");
    assert_eq!(
        material_side_table_counts(&pool, &reserved_bind).await,
        reserved_bind_counts
    );

    let content_prepared_finalizer = content_prepared(&pool).await;
    let content_prepared_finalizer_counts =
        material_side_table_counts(&pool, &content_prepared_finalizer).await;
    let mut transaction = context_transaction(&pool, &content_prepared_finalizer).await;
    let finalize_content_prepared =
        sqlx::query("SELECT vestrace_finalize_bound_content_material($1)")
            .bind(content_prepared_finalizer.intent_id)
            .execute(&mut *transaction)
            .await;
    assert_constraint_message(
        finalize_content_prepared,
        "only Bound may finalize content material publication",
        "a ContentPrepared material intent finalized publication",
    );
    transaction.rollback().await.unwrap();
    assert_eq!(
        intent_state(&pool, &content_prepared_finalizer).await,
        "content_prepared"
    );
    assert_eq!(
        material_side_table_counts(&pool, &content_prepared_finalizer).await,
        content_prepared_finalizer_counts
    );

    let reserved = reserve(&pool).await;
    assert_content_preparation_refusal(&pool, &reserved, "reserved").await;

    let provisional_created = reserve(&pool).await;
    record_provisional_created(&pool, &provisional_created).await;
    assert_content_preparation_refusal(&pool, &provisional_created, "provisional_created").await;

    let content_prepared_fixture = content_prepared(&pool).await;
    assert_content_preparation_refusal(&pool, &content_prepared_fixture, "content_prepared").await;

    let result_prepared_fixture = reserve(&pool).await;
    record_provisional_created(&pool, &result_prepared_fixture).await;
    record_provisional_receipt(&pool, &result_prepared_fixture).await;
    prepare_result(&pool, &result_prepared_fixture).await;
    assert_content_preparation_refusal(&pool, &result_prepared_fixture, "result_prepared").await;

    let bound_fixture = content_prepared(&pool).await;
    bind(&pool, &bound_fixture).await;
    assert_content_preparation_refusal(&pool, &bound_fixture, "bound").await;

    let live_fixture = content_prepared(&pool).await;
    bind(&pool, &live_fixture).await;
    finalize_bound(&pool, &live_fixture).await;
    assert_content_preparation_refusal(&pool, &live_fixture, "live").await;

    let legal_content = reserve(&pool).await;
    record_provisional_created(&pool, &legal_content).await;
    record_provisional_receipt(&pool, &legal_content).await;
    prepare_content(&pool, &legal_content).await;
    assert_eq!(
        intent_state(&pool, &legal_content).await,
        "content_prepared"
    );
    assert_eq!(
        material_side_table_counts(&pool, &legal_content).await,
        (1, 1, 1, 0)
    );

    let legal_result = reserve(&pool).await;
    record_provisional_created(&pool, &legal_result).await;
    record_provisional_receipt(&pool, &legal_result).await;
    prepare_result(&pool, &legal_result).await;
    assert_eq!(intent_state(&pool, &legal_result).await, "result_prepared");
    assert_eq!(
        material_side_table_counts(&pool, &legal_result).await,
        (1, 1, 1, 0)
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn content_prepared_is_not_referenceable(pool: PgPool) {
    let fixture = content_prepared(&pool).await;
    let live = material_is_live(&pool, &fixture).await;
    assert!(
        !live,
        "ContentPrepared material was visible as an ordinary reference"
    );

    let raw_reference = sqlx::query(
        "INSERT INTO content_material_ordinary_references \
         (id, workspace_id, material_id, intent_id, owner_kind, owner_id, output_ordinal) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.workspace_id)
    .bind(fixture.material_id)
    .bind(fixture.intent_id)
    .bind("content")
    .bind(fixture.principal_id)
    .bind(0_i64)
    .execute(&pool)
    .await;
    assert_sqlstate(
        raw_reference,
        "42501",
        "raw SQL referenced a ContentPrepared material as Live",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn content_prepared_cannot_jump_to_abandoned(pool: PgPool) {
    let fixture = content_prepared(&pool).await;
    let mut transaction = context_transaction(&pool, &fixture).await;
    let direct_finalizer = sqlx::query("SELECT vestrace_finalize_material_key_abandon($1)")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await;
    assert_sqlstate(
        direct_finalizer,
        "23514",
        "ContentPrepared finalized as Abandoned without ContentAbandonPrepared",
    );
    transaction.rollback().await.unwrap();

    let raw_jump =
        sqlx::query("UPDATE material_key_creation_intents SET state = 'abandoned' WHERE id = $1")
            .bind(fixture.intent_id)
            .execute(&pool)
            .await;
    assert_sqlstate(
        raw_jump,
        "42501",
        "raw SQL jumped ContentPrepared directly to Abandoned",
    );
    assert_eq!(intent_state(&pool, &fixture).await, "content_prepared");
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn abandon_requires_ciphertext_removal_and_witnessed_receipt(pool: PgPool) {
    let fixture = content_prepared(&pool).await;
    let mut transaction = context_transaction(&pool, &fixture).await;
    sqlx::query("SELECT vestrace_prepare_content_abandon($1)")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    let ciphertext_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM content_material_bytes WHERE intent_id = $1")
            .bind(fixture.intent_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let attachment_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM prepared_material_attachments WHERE intent_id = $1",
    )
    .bind(fixture.intent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ciphertext_count, 0);
    assert_eq!(attachment_count, 0);

    let mut transaction = context_transaction(&pool, &fixture).await;
    let missing_receipt = sqlx::query("SELECT vestrace_finalize_material_key_abandon($1)")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await;
    assert_sqlstate(
        missing_receipt,
        "23514",
        "an abandon finalizer committed without a witnessed unbound-key erase receipt",
    );
    transaction.rollback().await.unwrap();

    let mut transaction = context_transaction(&pool, &fixture).await;
    sqlx::query("SELECT vestrace_record_unbound_material_key_erasure($1, $2)")
        .bind(fixture.intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_material_key_abandon($1)")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    assert_eq!(intent_state(&pool, &fixture).await, "abandoned");
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn result_prepared_always_binds_and_never_abandons(pool: PgPool) {
    let fixture = reserve(&pool).await;
    record_provisional_created(&pool, &fixture).await;
    record_provisional_receipt(&pool, &fixture).await;
    prepare_result(&pool, &fixture).await;

    let mut transaction = context_transaction(&pool, &fixture).await;
    let abandon_result = sqlx::query("SELECT vestrace_prepare_content_abandon($1)")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await;
    assert_sqlstate(
        abandon_result,
        "23514",
        "ResultPrepared entered the content abandonment branch",
    );
    transaction.rollback().await.unwrap();

    bind(&pool, &fixture).await;
    finalize_bound(&pool, &fixture).await;
    assert_eq!(intent_state(&pool, &fixture).await, "live");
    assert!(material_is_live(&pool, &fixture).await);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn raw_sql_cannot_publish_a_prepared_identity(pool: PgPool) {
    let fixture = content_prepared(&pool).await;
    let publication = sqlx::query("UPDATE content_materials SET state = 'live' WHERE id = $1")
        .bind(fixture.material_id)
        .execute(&pool)
        .await;
    assert_sqlstate(
        publication,
        "42501",
        "raw SQL published a prepared content-material identity",
    );

    let runtime = runtime_pool(&pool).await;
    let runtime_publication =
        sqlx::query("UPDATE content_materials SET state = 'live' WHERE id = $1")
            .bind(fixture.material_id)
            .execute(&runtime)
            .await;
    runtime.close().await;
    assert_sqlstate(
        runtime_publication,
        "42501",
        "the runtime role published a prepared content-material identity",
    );

    let live = material_is_live(&pool, &fixture).await;
    assert!(!live);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn raw_sql_cannot_resurrect_an_abandoned_identity(pool: PgPool) {
    let fixture = content_prepared(&pool).await;
    let mut transaction = context_transaction(&pool, &fixture).await;
    sqlx::query("SELECT vestrace_prepare_content_abandon($1)")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_record_unbound_material_key_erasure($1, $2)")
        .bind(fixture.intent_id)
        .bind(Uuid::now_v7())
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_finalize_material_key_abandon($1)")
        .bind(fixture.intent_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    let resurrection = sqlx::query("UPDATE content_materials SET state = 'live' WHERE id = $1")
        .bind(fixture.material_id)
        .execute(&pool)
        .await;
    assert_sqlstate(
        resurrection,
        "42501",
        "raw SQL resurrected an abandoned content-material identity",
    );

    let runtime = runtime_pool(&pool).await;
    let runtime_resurrection =
        sqlx::query("UPDATE content_materials SET state = 'live' WHERE id = $1")
            .bind(fixture.material_id)
            .execute(&runtime)
            .await;
    runtime.close().await;
    assert_sqlstate(
        runtime_resurrection,
        "42501",
        "the runtime role resurrected an abandoned content-material identity",
    );
    assert_eq!(intent_state(&pool, &fixture).await, "abandoned");
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn runtime_role_direct_write_is_refused(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let result = sqlx::query(
        "INSERT INTO material_key_creation_intents \
         (id, workspace_id, material_id, material_key_id, nonce, owner_kind, owner_id, output_ordinal, state) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'reserved')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind("content")
    .bind(Uuid::now_v7())
    .bind(0_i64)
    .execute(&runtime)
    .await;
    runtime.close().await;

    assert_sqlstate(
        result,
        "42501",
        "the runtime role directly inserted a material-key creation intent",
    );
}
