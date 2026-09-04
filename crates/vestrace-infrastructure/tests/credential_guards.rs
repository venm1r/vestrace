use std::sync::Arc;

use sqlx::{PgPool, Postgres, Row, Transaction};
use tokio::sync::Barrier;
use uuid::Uuid;
use vestrace_application::{ApplicationError, RequestContext};
use vestrace_domain::{ConnectionId, CredentialSlotId, PrincipalId, WorkspaceId};
use vestrace_infrastructure::{PgCredentialGuardRepository, PgStore};

const CANONICAL_LOCK_ORDER: &[&str] = &[
    "connection_execution_guard",
    "credential_activation_guard",
    "credential_slot",
    "revision_material",
];

#[derive(Clone, Copy)]
struct GuardFixture {
    connection_id: ConnectionId,
    slot_id: CredentialSlotId,
    execution_guard_id: Uuid,
}

fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

async fn seed_context(pool: &PgPool, context: &RequestContext) {
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("credential-guards-{}", context.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(context.principal_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(format!("principal-{}", context.principal_id))
        .execute(pool)
        .await
        .unwrap();
}

async fn begin_scoped<'a>(pool: &'a PgPool, context: &RequestContext) -> Transaction<'a, Postgres> {
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.workspace_id")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
        .bind("vestrace.principal_id")
        .bind(context.principal_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction
}

async fn ensure_execution_guard(
    pool: &PgPool,
    context: &RequestContext,
    guard_id: Uuid,
    connection_id: ConnectionId,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = begin_scoped(pool, context).await;
    let result =
        sqlx::query_scalar("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
            .bind(guard_id)
            .bind(context.workspace_id.as_uuid())
            .bind(connection_id.as_uuid())
            .fetch_one(&mut *transaction)
            .await;
    if result.is_ok() {
        transaction.commit().await.unwrap();
    }
    result
}

async fn reserve_slot(
    pool: &PgPool,
    context: &RequestContext,
    slot_id: CredentialSlotId,
    connection_id: ConnectionId,
) -> Result<(), sqlx::Error> {
    let mut transaction = begin_scoped(pool, context).await;
    let result = sqlx::query("SELECT vestrace_reserve_credential_slot($1, $2, $3, $4, $5)")
        .bind(slot_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(connection_id.as_uuid())
        .bind("provider")
        .bind("primary")
        .execute(&mut *transaction)
        .await;
    if result.is_ok() {
        transaction.commit().await.unwrap();
    }
    result.map(|_| ())
}

async fn ensure_activation_guard(
    pool: &PgPool,
    context: &RequestContext,
    activation_guard_id: Uuid,
    connection_id: ConnectionId,
    slot_id: CredentialSlotId,
) -> Result<Uuid, sqlx::Error> {
    let mut transaction = begin_scoped(pool, context).await;
    let result =
        sqlx::query_scalar("SELECT vestrace_ensure_credential_activation_guard($1, $2, $3, $4)")
            .bind(activation_guard_id)
            .bind(context.workspace_id.as_uuid())
            .bind(connection_id.as_uuid())
            .bind(slot_id.as_uuid())
            .fetch_one(&mut *transaction)
            .await;
    if result.is_ok() {
        transaction.commit().await.unwrap();
    }
    result
}

async fn reserve_preparing_occupancy(
    pool: &PgPool,
    context: &RequestContext,
    occupancy_id: Uuid,
    connection_id: ConnectionId,
    slot_id: CredentialSlotId,
) -> Result<(), sqlx::Error> {
    let mut transaction = begin_scoped(pool, context).await;
    let result =
        sqlx::query("SELECT vestrace_reserve_credential_preparing_occupancy($1, $2, $3, $4)")
            .bind(occupancy_id)
            .bind(context.workspace_id.as_uuid())
            .bind(connection_id.as_uuid())
            .bind(slot_id.as_uuid())
            .execute(&mut *transaction)
            .await;
    if result.is_ok() {
        transaction.commit().await.unwrap();
    }
    result.map(|_| ())
}

async fn seed_guard_fixture(pool: &PgPool, context: &RequestContext) -> GuardFixture {
    seed_context(pool, context).await;
    let connector_id = Uuid::now_v7();
    let connection_id = ConnectionId::new();
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(connector_id)
    .bind(context.workspace_id.as_uuid())
    .bind(format!("connector-{connector_id}"))
    .bind("openai")
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections (id, connector_id, workspace_id, principal_id, name) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(connection_id.as_uuid())
    .bind(connector_id)
    .bind(context.workspace_id.as_uuid())
    .bind(context.principal_id.as_uuid())
    .bind(format!("connection-{connection_id}"))
    .execute(pool)
    .await
    .unwrap();

    let execution_guard_id = Uuid::now_v7();
    ensure_execution_guard(pool, context, execution_guard_id, connection_id)
        .await
        .unwrap();
    let slot_id = CredentialSlotId::new();
    reserve_slot(pool, context, slot_id, connection_id)
        .await
        .unwrap();
    let activation_guard_id = Uuid::now_v7();
    ensure_activation_guard(pool, context, activation_guard_id, connection_id, slot_id)
        .await
        .unwrap();

    GuardFixture {
        connection_id,
        slot_id,
        execution_guard_id,
    }
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

#[sqlx::test(migrations = "../../migrations")]
async fn execution_guard_is_permanent(pool: PgPool) {
    let context = context();
    let fixture = seed_guard_fixture(&pool, &context).await;

    let replayed = ensure_execution_guard(&pool, &context, Uuid::now_v7(), fixture.connection_id)
        .await
        .unwrap();
    assert_eq!(replayed, fixture.execution_guard_id);

    let result =
        sqlx::query("UPDATE connection_execution_guards SET connection_id = $1 WHERE id = $2")
            .bind(Uuid::now_v7())
            .bind(fixture.execution_guard_id)
            .execute(&pool)
            .await;
    assert_sqlstate(
        result,
        "42501",
        "a raw update attempted to reuse a permanent execution guard",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn activation_guard_precedes_candidate_material(pool: PgPool) {
    let context = context();
    seed_context(&pool, &context).await;
    let connector_id = Uuid::now_v7();
    let connection_id = ConnectionId::new();
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(connector_id)
    .bind(context.workspace_id.as_uuid())
    .bind(format!("connector-{connector_id}"))
    .bind("openai")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO connections (id, connector_id, workspace_id, principal_id, name) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(connection_id.as_uuid())
    .bind(connector_id)
    .bind(context.workspace_id.as_uuid())
    .bind(context.principal_id.as_uuid())
    .bind(format!("connection-{connection_id}"))
    .execute(&pool)
    .await
    .unwrap();
    ensure_execution_guard(&pool, &context, Uuid::now_v7(), connection_id)
        .await
        .unwrap();
    let slot_id = CredentialSlotId::new();
    reserve_slot(&pool, &context, slot_id, connection_id)
        .await
        .unwrap();

    assert_sqlstate(
        reserve_preparing_occupancy(&pool, &context, Uuid::now_v7(), connection_id, slot_id).await,
        "23514",
        "a credential preparation without its activation guard",
    );

    ensure_activation_guard(&pool, &context, Uuid::now_v7(), connection_id, slot_id)
        .await
        .unwrap();
    let repository = PgCredentialGuardRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .reserve_preparing(&context, connection_id, slot_id, Uuid::now_v7())
        .await
        .expect("candidate preparation must be possible only after the activation guard exists");
}

#[sqlx::test(migrations = "../../migrations")]
async fn occupancy_permits_at_most_one_nonterminal_candidate(pool: PgPool) {
    let context = context();
    let fixture = seed_guard_fixture(&pool, &context).await;

    reserve_preparing_occupancy(
        &pool,
        &context,
        Uuid::now_v7(),
        fixture.connection_id,
        fixture.slot_id,
    )
    .await
    .unwrap();
    assert_sqlstate(
        reserve_preparing_occupancy(
            &pool,
            &context,
            Uuid::now_v7(),
            fixture.connection_id,
            fixture.slot_id,
        )
        .await,
        "23505",
        "a second nonterminal credential preparation under one activation guard",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn second_concurrent_preparing_returns_a_typed_conflict(pool: PgPool) {
    let context = context();
    let fixture = seed_guard_fixture(&pool, &context).await;
    let repository = PgCredentialGuardRepository::new(PgStore::from_pool(pool));
    let barrier = Arc::new(Barrier::new(2));

    let first = {
        let repository = repository.clone();
        let context = context.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            repository
                .reserve_preparing(
                    &context,
                    fixture.connection_id,
                    fixture.slot_id,
                    Uuid::now_v7(),
                )
                .await
        })
    };
    let second = {
        let repository = repository.clone();
        let context = context.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            repository
                .reserve_preparing(
                    &context,
                    fixture.connection_id,
                    fixture.slot_id,
                    Uuid::now_v7(),
                )
                .await
        })
    };

    let first = first.await.unwrap();
    let second = second.await.unwrap();
    let results = [first, second];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(results.iter().any(|result| {
        matches!(
            result,
            Err(ApplicationError::Conflict(message)) if message == "CREDENTIAL_GUARD_OCCUPIED"
        )
    }));
}

#[sqlx::test(migrations = "../../migrations")]
async fn lock_order_helper_refuses_an_inverted_acquisition(pool: PgPool) {
    let context = context();
    let fixture = seed_guard_fixture(&pool, &context).await;
    let mut transaction = begin_scoped(&pool, &context).await;
    let result = sqlx::query("SELECT vestrace_acquire_credential_lock_chain($1, $2, $3, $4)")
        .bind(context.workspace_id.as_uuid())
        .bind(fixture.connection_id.as_uuid())
        .bind(fixture.slot_id.as_uuid())
        .bind([
            "credential_slot",
            "credential_activation_guard",
            "connection_execution_guard",
            "revision_material",
        ])
        .execute(&mut *transaction)
        .await;
    assert_sqlstate(result, "23514", "an inverted credential guard acquisition");
    drop(transaction);

    let mut transaction = begin_scoped(&pool, &context).await;
    sqlx::query("SELECT vestrace_acquire_credential_lock_chain($1, $2, $3, $4)")
        .bind(context.workspace_id.as_uuid())
        .bind(fixture.connection_id.as_uuid())
        .bind(fixture.slot_id.as_uuid())
        .bind(CANONICAL_LOCK_ORDER)
        .execute(&mut *transaction)
        .await
        .expect("the canonical credential lock order must acquire successfully");
    transaction.commit().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn slot_holds_no_ciphertext_column(pool: PgPool) {
    let columns = sqlx::query(
        "SELECT column_name, data_type, udt_name \
         FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = 'credential_slots' \
         ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let column_names: Vec<String> = columns
        .iter()
        .map(|column| column.get("column_name"))
        .collect();
    assert_eq!(
        column_names,
        vec![
            "id",
            "workspace_id",
            "connection_id",
            "purpose",
            "name",
            "current_revision_id",
            "current_revision_version",
            "tombstone_version",
            "tombstoned_at",
            "created_at",
            "updated_at",
        ],
        "the slot schema must remain only its stable identity, CAS, and tombstone fields",
    );
    assert!(columns.iter().all(|column| {
        column.get::<String, _>("udt_name") != "bytea"
            && !column
                .get::<String, _>("column_name")
                .contains("ciphertext")
    }));
}
