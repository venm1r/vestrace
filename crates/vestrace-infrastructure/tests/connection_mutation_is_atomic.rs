use chrono::Duration;
use sqlx::PgPool;
use uuid::Uuid;
use vestrace_application::{
    ConnectionRevisionRepository, CreateConnectionRevision, IdempotencyRecord, OutboxMessage,
    RequestContext,
};
use vestrace_domain::{
    AuditEvent, ConnectionAuthMode, ConnectionId, ConnectionKind, ConnectionRevisionId,
    ConnectionTransportPolicy, ConnectorId, PrincipalId, WorkspaceId,
    connection::{Connection, ConnectionStatus},
    id::AuditEventId,
    time::now,
};
use vestrace_infrastructure::{PgConnectionRevisionRepository, PgStore};

fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

fn command(context: &RequestContext) -> CreateConnectionRevision {
    let connection = Connection {
        id: ConnectionId::new(),
        connector_id: ConnectorId::new(),
        workspace_id: context.workspace_id,
        principal_id: context.principal_id,
        name: format!("atomic-{}", Uuid::now_v7()),
        status: ConnectionStatus::Active,
        created_at: now(),
    };
    let created_at = now();
    CreateConnectionRevision {
        connection: connection.clone(),
        revision_id: ConnectionRevisionId::new(),
        execution_guard_id: Uuid::now_v7(),
        kind: ConnectionKind::LMStudioLocal,
        logical_base_url: "http://127.0.0.1:1234/v1".to_owned(),
        runtime_base_url: "http://127.0.0.1:1234/v1".to_owned(),
        adapter_profile_revision: "lm-studio-local/v1".to_owned(),
        transport_policy: ConnectionTransportPolicy::LoopbackOnly,
        auth_mode: ConnectionAuthMode::None,
        credential_slot_id: None,
        expected_head_version: 0,
        idempotency: Some(IdempotencyRecord {
            idempotency_key: format!("connection-{}", Uuid::now_v7()),
            workspace_id: context.workspace_id,
            request_hash: format!("hash-{}", Uuid::now_v7()),
            response_payload: None,
            status: "completed".to_owned(),
            created_at,
            expires_at: created_at + Duration::hours(1),
        }),
        outbox: vec![OutboxMessage::new(
            context.workspace_id,
            "connection.revision.created",
            serde_json::json!({"connection_id": connection.id}),
            created_at,
        )],
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "connection.revision.created",
            "connection",
            connection.id.as_uuid(),
            serde_json::json!({"connection_id": connection.id}),
            created_at,
        )
        .unwrap(),
    }
}

async fn seed_context(pool: &PgPool, context: &RequestContext, command: &CreateConnectionRevision) {
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("atomic-{}", context.workspace_id))
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
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(command.connection.connector_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(format!("connector-{}", command.connection.connector_id))
    .bind("local")
    .execute(pool)
    .await
    .unwrap();
}

/// Seeds only what a second command in the same workspace needs. The workspace
/// and principal already exist; re-inserting them is a primary-key violation
/// rather than a fixture.
async fn seed_connector(
    pool: &PgPool,
    context: &RequestContext,
    command: &CreateConnectionRevision,
) {
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(command.connection.connector_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(format!("connector-{}", command.connection.connector_id))
    .bind("local")
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn connection_mutation_rolls_back_when_audit_fails(pool: PgPool) {
    let context = context();
    let command = command(&context);
    seed_context(&pool, &context, &command).await;
    sqlx::query("INSERT INTO audit_events (id, workspace_id, principal_id, action, resource_type, resource_id, payload, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)")
        .bind(command.audit.id.as_uuid()).bind(context.workspace_id.as_uuid()).bind(context.principal_id.as_uuid())
        .bind("already.exists").bind("connection").bind(Uuid::now_v7()).bind(serde_json::json!({})).bind(now())
        .execute(&pool).await.unwrap();

    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));
    assert!(
        repository
            .create_governed(context, command.clone())
            .await
            .is_err()
    );
    let connections: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM connections WHERE id = $1")
        .bind(command.connection.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    let guards: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM connection_execution_guards WHERE connection_id = $1",
    )
    .bind(command.connection.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((connections, guards), (0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_rolls_back_when_connection_mutation_fails(pool: PgPool) {
    let context = context();
    let created = command(&context);
    seed_context(&pool, &context, &created).await;
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .create_governed(context.clone(), created.clone())
        .await
        .unwrap();
    let mut stale = created.clone();
    stale.revision_id = ConnectionRevisionId::new();
    stale.expected_head_version = 0;
    stale.audit.id = AuditEventId::new();
    stale.idempotency.as_mut().unwrap().idempotency_key = format!("connection-{}", Uuid::now_v7());
    stale.outbox = vec![OutboxMessage::new(
        stale.connection.workspace_id,
        "connection.revision.created",
        serde_json::json!({"connection_id": stale.connection.id}),
        now(),
    )];

    assert!(
        repository
            .revise_governed(context, stale.clone())
            .await
            .is_err()
    );
    let audits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE id = $1")
        .bind(stale.audit.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    let revisions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM connection_revisions WHERE connection_id = $1")
            .bind(created.connection.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audits, 0);
    assert_eq!(revisions, 1);
}

/// One key names one request. The governed boundary writes idempotency keys
/// with `ON CONFLICT DO NOTHING`, which discards the second request's hash
/// without comparing it, so the comparison has to happen before anything is
/// applied — otherwise a single key could carry two different mutations and
/// only the mutation's own guard would notice.
#[sqlx::test(migrations = "../../migrations")]
async fn a_reused_idempotency_key_carrying_a_different_request_is_refused(pool: PgPool) {
    let context = context();
    let created = command(&context);
    seed_context(&pool, &context, &created).await;
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .create_governed(context.clone(), created.clone())
        .await
        .expect("the first governed revision must commit");

    // The same key, a different request: a second connection entirely.
    let mut smuggled = command(&context);
    smuggled.idempotency.as_mut().unwrap().idempotency_key = created
        .idempotency
        .as_ref()
        .unwrap()
        .idempotency_key
        .clone();
    seed_connector(&pool, &context, &smuggled).await;

    let error = repository
        .create_governed(context.clone(), smuggled.clone())
        .await
        .expect_err("a reused key with a different request must not commit");
    assert!(
        format!("{error:?}").contains("IDEMPOTENCY_KEY_REUSE_CONFLICT"),
        "{error:?}"
    );

    // Nothing of the smuggled request survives: not the connection, not its
    // audit event, not its outbox message.
    let observed: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM connection_revisions WHERE id = $1),
                (SELECT COUNT(*) FROM audit_events WHERE id = $2),
                (SELECT COUNT(*) FROM outbox WHERE id = $3)",
    )
    .bind(smuggled.revision_id.as_uuid())
    .bind(smuggled.audit.id.as_uuid())
    .bind(smuggled.outbox[0].id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(observed, (0, 0, 0));
}

/// The same key with the same request is a redelivery, not a second request.
/// It must not be refused on the key alone; what stops it publishing twice is
/// the immutable head guard, and this test pins that it is the guard doing the
/// work rather than an accidental key collision.
#[sqlx::test(migrations = "../../migrations")]
async fn a_redelivered_identical_request_is_refused_by_its_head_guard_not_by_its_key(pool: PgPool) {
    let context = context();
    let created = command(&context);
    seed_context(&pool, &context, &created).await;
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .create_governed(context.clone(), created.clone())
        .await
        .expect("the first governed revision must commit");

    let error = repository
        .create_governed(context.clone(), created.clone())
        .await
        .expect_err("a redelivery must not publish a second revision");
    assert!(
        !format!("{error:?}").contains("IDEMPOTENCY_KEY_REUSE_CONFLICT"),
        "an identical redelivery is not a key reuse conflict: {error:?}"
    );

    let revisions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM connection_revisions WHERE connection_id = $1")
            .bind(created.connection.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(revisions, 1);
}

/// The outbox is part of the same transaction as the mutation it announces. A
/// message that survived a rolled-back revision would tell every subscriber
/// about a Connection that does not exist.
#[sqlx::test(migrations = "../../migrations")]
async fn the_outbox_message_rolls_back_with_its_mutation(pool: PgPool) {
    let context = context();
    let created = command(&context);
    seed_context(&pool, &context, &created).await;
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .create_governed(context.clone(), created.clone())
        .await
        .unwrap();

    // A stale head version fails the mutation after its evidence was built.
    let mut stale = command(&context);
    stale.connection = created.connection.clone();
    stale.expected_head_version = 0;
    // The connector is the one the created connection already uses; seeding it
    // again would be a primary-key violation rather than a fixture.

    assert!(
        repository
            .revise_governed(context, stale.clone())
            .await
            .is_err()
    );
    let messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox WHERE id = $1")
        .bind(stale.outbox[0].id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(messages, 0);
}
