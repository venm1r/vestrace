use chrono::Duration;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, ConnectionRevisionRepository, CreateConnectionRevision, IdempotencyRecord,
    OutboxMessage, RequestContext,
};
use vestrace_domain::{
    AuditEvent, ConnectionAuthMode, ConnectionId, ConnectionKind, ConnectionRevisionId,
    ConnectionTransportPolicy, ConnectorId, PrincipalId, WorkspaceId,
    connection::{Connection, ConnectionStatus},
    id::AuditEventId,
    time::now,
};
use vestrace_infrastructure::{PgConnectionRevisionRepository, PgStore};

const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";

fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

fn command(context: &RequestContext) -> CreateConnectionRevision {
    let connection = Connection {
        id: ConnectionId::new(),
        connector_id: ConnectorId::new(),
        workspace_id: context.workspace_id,
        principal_id: context.principal_id,
        name: format!("revision-{}", Uuid::now_v7()),
        status: ConnectionStatus::Active,
        created_at: now(),
    };
    let revision_id = ConnectionRevisionId::new();
    let created_at = now();
    CreateConnectionRevision {
        connection: connection.clone(),
        revision_id,
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
            response_payload: Some(serde_json::json!({"connection_id": connection.id})),
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
        .bind(format!("connection-{}", context.workspace_id))
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

async fn runtime_pool(source: &PgPool) -> PgPool {
    let runtime_database_url = std::env::var(RUNTIME_DATABASE_URL_ENV)
        .expect("VESTRACE_RUNTIME_DATABASE_URL must supply restricted runtime credentials");
    let authority = runtime_database_url
        .split_once("://")
        .and_then(|(_, value)| value.rsplit_once('@'))
        .expect("VESTRACE_RUNTIME_DATABASE_URL must contain credentials");
    let (username, password) = authority
        .0
        .split_once(':')
        .expect("VESTRACE_RUNTIME_DATABASE_URL must contain a password");
    let options = source
        .connect_options()
        .as_ref()
        .clone()
        .username(username)
        .password(password);
    let runtime = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let role = sqlx::query("SELECT current_user::text AS role, rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user")
        .fetch_one(&runtime)
        .await
        .unwrap();
    assert_eq!(role.get::<String, _>("role"), "vestrace");
    assert!(!role.get::<bool, _>("rolsuper"));
    assert!(!role.get::<bool, _>("rolbypassrls"));
    runtime
}

async fn counts(pool: &PgPool, connection_id: ConnectionId) -> (i64, i64, i64, i64) {
    let id = connection_id.as_uuid();
    (
        sqlx::query_scalar("SELECT COUNT(*) FROM connections WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap(),
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM connection_execution_guards WHERE connection_id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap(),
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM connection_revision_heads WHERE connection_id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap(),
        sqlx::query_scalar("SELECT COUNT(*) FROM connection_revisions WHERE connection_id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap(),
    )
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn connection_creation_atomically_creates_its_permanent_guard(pool: PgPool) {
    let context = context();
    let command = command(&context);
    seed_context(&pool, &context, &command).await;
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));

    repository
        .create_governed(context, command.clone())
        .await
        .unwrap();

    assert_eq!(counts(&pool, command.connection.id).await, (1, 1, 1, 1));
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn no_auth_revision_atomically_creates_one_no_auth_binding(pool: PgPool) {
    let context = context();
    let command = command(&context);
    seed_context(&pool, &context, &command).await;
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));

    repository
        .create_governed(context, command.clone())
        .await
        .unwrap();

    let bindings: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM no_auth_binding_revisions WHERE connection_revision_id = $1",
    )
    .bind(command.revision_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(bindings, 1);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn safe_connection_projection_marks_legacy_and_unqualified_heads_non_executable(
    pool: PgPool,
) {
    let context = context();
    let command = command(&context);
    seed_context(&pool, &context, &command).await;
    let owner_repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));
    owner_repository
        .create_governed(context.clone(), command.clone())
        .await
        .unwrap();

    let legacy_connection_id = ConnectionId::new();
    sqlx::query(
        "INSERT INTO connections (id,connector_id,workspace_id,principal_id,name,status) \
         VALUES ($1,$2,$3,$4,'legacy-only','active')",
    )
    .bind(legacy_connection_id.as_uuid())
    .bind(command.connection.connector_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(context.principal_id.as_uuid())
    .execute(&pool)
    .await
    .unwrap();

    let runtime = runtime_pool(&pool).await;
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(runtime.clone()));
    let projections = repository
        .list_safe_connections(&context)
        .await
        .expect("restricted runtime lists opaque governed connection projections");
    assert_eq!(projections.len(), 2);
    let governed = projections
        .iter()
        .find(|projection| projection.id == command.connection.id)
        .unwrap();
    assert_eq!(governed.revision_id, Some(command.revision_id));
    assert_eq!(governed.state, "blocked");
    assert_eq!(governed.qualification_state, "missing");
    assert_eq!(governed.blockers, ["qualification_required"]);
    let legacy = projections
        .iter()
        .find(|projection| projection.id == legacy_connection_id)
        .unwrap();
    assert_eq!(legacy.revision_id, None);
    assert_eq!(legacy.state, "legacy");
    assert_eq!(legacy.qualification_state, "missing");
    assert_eq!(legacy.blockers, ["missing_governed_connection_revision"]);
    runtime.close().await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn auth_required_revision_cannot_own_a_no_auth_binding(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let workspace_id = Uuid::now_v7();
    let principal_id = Uuid::now_v7();
    let connector_id = Uuid::now_v7();
    let connection_id = Uuid::now_v7();
    let guard_id = Uuid::now_v7();
    let slot_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id)
        .bind(format!("connection-{workspace_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(principal_id)
        .bind(workspace_id)
        .bind(format!("principal-{principal_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(connector_id)
    .bind(workspace_id)
    .bind(format!("connector-{connector_id}"))
    .bind("openai")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO connections (id, connector_id, workspace_id, principal_id, name) VALUES ($1, $2, $3, $4, $5)")
        .bind(connection_id).bind(connector_id).bind(workspace_id).bind(principal_id).bind("auth-required").execute(&pool).await.unwrap();
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
        .bind(guard_id)
        .bind(workspace_id)
        .bind(connection_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_slot($1, $2, $3, $4, $5)")
        .bind(slot_id)
        .bind(workspace_id)
        .bind(connection_id)
        .bind("provider")
        .bind("primary")
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_create_connection_revision_and_advance_head($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
        .bind(revision_id).bind(workspace_id).bind(connection_id).bind(guard_id)
        .bind("open_ai_chat_completions_v1").bind("https://logical.example/v1").bind("https://runtime.example/v1")
        .bind("openai/v1").bind("remote_https").bind("bearer").bind(slot_id).bind(0_i64)
        .execute(&mut *transaction).await.unwrap();
    let result = sqlx::query("SELECT vestrace_create_no_auth_binding_revision($1, $2, $3, $4)")
        .bind(Uuid::now_v7())
        .bind(workspace_id)
        .bind(connection_id)
        .bind(revision_id)
        .execute(&mut *transaction)
        .await;
    let error = result.expect_err("an auth-required revision must reject a no-auth binding");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    transaction.rollback().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn connection_revision_is_immutable_and_head_uses_expected_version(pool: PgPool) {
    let context = context();
    let command = command(&context);
    seed_context(&pool, &context, &command).await;
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .create_governed(context.clone(), command.clone())
        .await
        .unwrap();
    let mut revision = command.clone();
    revision.revision_id = ConnectionRevisionId::new();
    revision.expected_head_version = 1;
    revision.audit.id = AuditEventId::new();
    revision.idempotency.as_mut().unwrap().idempotency_key =
        format!("connection-{}", Uuid::now_v7());
    revision.outbox = vec![OutboxMessage::new(
        context.workspace_id,
        "connection.revision.created",
        serde_json::json!({"connection_id": revision.connection.id}),
        now(),
    )];
    let mut competing = revision.clone();
    competing.revision_id = ConnectionRevisionId::new();
    competing.audit.id = AuditEventId::new();
    competing.idempotency.as_mut().unwrap().idempotency_key =
        format!("connection-{}", Uuid::now_v7());
    competing.outbox = vec![OutboxMessage::new(
        context.workspace_id,
        "connection.revision.created",
        serde_json::json!({"connection_id": competing.connection.id}),
        now(),
    )];
    let left_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let right_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let left = PgConnectionRevisionRepository::new(PgStore::from_pool(left_pool.clone()));
    let right = PgConnectionRevisionRepository::new(PgStore::from_pool(right_pool.clone()));
    let (left_result, right_result) = tokio::join!(
        left.revise_governed(context.clone(), revision.clone()),
        right.revise_governed(context.clone(), competing.clone()),
    );
    let winner = match (left_result, right_result) {
        (Ok(_), Err(ApplicationError::Conflict(code))) if code == "CONNECTION_VERSION_CONFLICT" => {
            revision.revision_id
        }
        (Err(ApplicationError::Conflict(code)), Ok(_)) if code == "CONNECTION_VERSION_CONFLICT" => {
            competing.revision_id
        }
        (left, right) => panic!(
            "independent-pool CAS race must yield one commit and one typed conflict: {left:?}, {right:?}"
        ),
    };
    left_pool.close().await;
    right_pool.close().await;
    let revisions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM connection_revisions WHERE connection_id = $1")
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
    let heads: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM connection_revision_heads WHERE connection_id = $1",
    )
    .bind(command.connection.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let current_revision_id: Uuid = sqlx::query_scalar(
        "SELECT current_revision_id FROM connection_revision_heads WHERE connection_id = $1",
    )
    .bind(command.connection.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revisions, 2);
    assert_eq!(
        (guards, heads, current_revision_id),
        (1, 1, winner.as_uuid())
    );

    let original_logical_base_url: String =
        sqlx::query_scalar("SELECT logical_base_url FROM connection_revisions WHERE id = $1")
            .bind(command.revision_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        original_logical_base_url, command.logical_base_url,
        "advancing a head must preserve the prior immutable revision"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn raw_catalog_row_is_not_executable_without_a_guarded_head(pool: PgPool) {
    let runtime = runtime_pool(&pool).await;
    let context = context();
    let command = command(&context);
    seed_context(&pool, &context, &command).await;
    sqlx::query("INSERT INTO connections (id, connector_id, workspace_id, principal_id, name) VALUES ($1, $2, $3, $4, $5)")
        .bind(command.connection.id.as_uuid()).bind(command.connection.connector_id.as_uuid())
        .bind(context.workspace_id.as_uuid()).bind(context.principal_id.as_uuid()).bind(&command.connection.name)
        .execute(&pool).await.unwrap();
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let result = sqlx::query(
        "SELECT vestrace_create_connection_revision_and_advance_head(\
         $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(command.revision_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(command.connection.id.as_uuid())
    .bind(command.execution_guard_id)
    .bind("lm_studio_local")
    .bind(&command.logical_base_url)
    .bind(&command.runtime_base_url)
    .bind(&command.adapter_profile_revision)
    .bind("loopback_only")
    .bind("none")
    .bind(Option::<Uuid>::None)
    .bind(0_i64)
    .execute(&mut *transaction)
    .await;
    let error =
        result.expect_err("a raw catalog row without its permanent guard must not be executable");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    transaction.rollback().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn creating_a_connection_on_a_fresh_workspace_materializes_its_connector(pool: PgPool) {
    let context = context();
    let cmd = command(&context);
    // Seed workspace and principal only, NOT the connector.
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("connection-{}", context.workspace_id))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(context.principal_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(format!("principal-{}", context.principal_id))
        .execute(&pool)
        .await
        .unwrap();

    // Create the connection; this should auto-materialize the connector.
    let repository = PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .create_governed(context.clone(), cmd.clone())
        .await
        .unwrap();

    // Verify the connector row was created with the correct provider_type.
    let provider_type: String =
        sqlx::query_scalar("SELECT provider_type FROM connectors WHERE id = $1")
            .bind(cmd.connection.connector_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(provider_type, "local");
}
