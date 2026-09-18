use std::sync::Arc;

use chrono::Duration;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;
use vestrace_application::{
    ApplicationError, ConnectionRevisionRepository, CreateConnectionRevision, CreateModelRevision,
    IdempotencyRecord, LEGACY_RUN_MODEL_DEFAULT_PURPOSE, ModelRecord, ModelRevisionRepository,
    OutboxMessage, RequestContext, RunCommandExecutor, RunCommandService, SetWorkspaceModelDefault,
};
use vestrace_domain::{
    AuditEvent, ConnectionAuthMode, ConnectionId, ConnectionKind, ConnectionRevisionId,
    ConnectionTransportPolicy, ConnectorId, ModelId, ModelKind, ModelObservation, ModelRevision,
    ModelRevisionId, PrincipalId, ProviderId, WorkspaceId,
    connection::{Connection, ConnectionStatus},
    id::{AgentRunId, AuditEventId, CorrelationId, OperationId},
    run::{RunActor, RunCommand, RunCommandEnvelope, RunVersion},
    time::now,
};
use vestrace_infrastructure::{
    PgConnectionRevisionRepository, PgModelRevisionRepository, PgRunCommandCommitter,
    PgRunEventStore, PgStore,
};

const RUNTIME_DATABASE_URL_ENV: &str = "VESTRACE_RUNTIME_DATABASE_URL";

type NoAuthSnapshotColumns = (
    String,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Option<i64>,
    Option<Uuid>,
);

#[derive(Clone)]
pub(crate) struct Fixture {
    pub(crate) context: RequestContext,
    pub(crate) connection_id: ConnectionId,
    pub(crate) connection_revision_id: ConnectionRevisionId,
    pub(crate) execution_guard_id: Uuid,
    pub(crate) provider_id: ProviderId,
}

#[derive(Clone)]
pub(crate) struct CredentialBindingFixture {
    pub(crate) fixture: Fixture,
    pub(crate) slot_id: Uuid,
    pub(crate) credential_revision_id: Uuid,
    pub(crate) activation_guard_id: Uuid,
    pub(crate) credential_intent_id: Uuid,
    pub(crate) candidate_credential_revision_id: Uuid,
    pub(crate) candidate_credential_intent_id: Uuid,
}

#[derive(Clone, Copy)]
pub(crate) enum QualificationRefreshMode {
    Valid,
    Expired,
    DiscoveryOnly,
    CredentialCandidate,
}

fn context() -> RequestContext {
    RequestContext::new(WorkspaceId::new(), PrincipalId::new())
}

fn connection_command(context: &RequestContext) -> CreateConnectionRevision {
    let connection = Connection {
        id: ConnectionId::new(),
        connector_id: ConnectorId::new(),
        workspace_id: context.workspace_id,
        principal_id: context.principal_id,
        name: format!("model-revision-{}", Uuid::now_v7()),
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
            idempotency_key: format!("model-connection-{}", Uuid::now_v7()),
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

async fn fixture(pool: &PgPool) -> Fixture {
    let context = context();
    let connection = connection_command(&context);
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("model-{}", context.workspace_id))
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
    .bind(connection.connection.connector_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(format!("connector-{}", connection.connection.connector_id))
    .bind("local")
    .execute(pool)
    .await
    .unwrap();
    let provider_id = ProviderId::new();
    sqlx::query(
        "INSERT INTO providers (id, workspace_id, name, locality) VALUES ($1, $2, $3, 'local')",
    )
    .bind(provider_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(format!("provider-{provider_id}"))
    .execute(pool)
    .await
    .unwrap();
    PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()))
        .create_governed(context.clone(), connection.clone())
        .await
        .unwrap();
    Fixture {
        context,
        connection_id: connection.connection.id,
        connection_revision_id: connection.revision_id,
        execution_guard_id: connection.execution_guard_id,
        provider_id,
    }
}

async fn credential_binding_fixture(pool: &PgPool) -> CredentialBindingFixture {
    let context = context();
    let mut connection = connection_command(&context);
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("credential-model-{}", context.workspace_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(context.principal_id.as_uuid())
        .bind(context.workspace_id.as_uuid())
        .bind(format!("credential-principal-{}", context.principal_id))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(connection.connection.connector_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(format!(
        "credential-connector-{}",
        connection.connection.connector_id
    ))
    .bind("local")
    .execute(pool)
    .await
    .unwrap();
    let provider_id = ProviderId::new();
    sqlx::query(
        "INSERT INTO providers (id, workspace_id, name, locality) VALUES ($1, $2, $3, 'local')",
    )
    .bind(provider_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(format!("credential-provider-{provider_id}"))
    .execute(pool)
    .await
    .unwrap();

    PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()))
        .create_governed(context.clone(), connection.clone())
        .await
        .unwrap();

    let slot_id = Uuid::now_v7();
    let activation_guard_id = Uuid::now_v7();
    let mut setup = pool.begin().await.unwrap();
    for (name, value) in [
        ("vestrace.workspace_id", context.workspace_id.to_string()),
        ("vestrace.principal_id", context.principal_id.to_string()),
    ] {
        sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
            .bind(name)
            .bind(value)
            .fetch_one(&mut *setup)
            .await
            .unwrap();
    }
    sqlx::query("SELECT vestrace_ensure_connection_execution_guard($1, $2, $3)")
        .bind(connection.execution_guard_id)
        .bind(context.workspace_id.as_uuid())
        .bind(connection.connection.id.as_uuid())
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_reserve_credential_slot($1, $2, $3, 'provider', 'primary')")
        .bind(slot_id)
        .bind(context.workspace_id.as_uuid())
        .bind(connection.connection.id.as_uuid())
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("SELECT vestrace_ensure_credential_activation_guard($1, $2, $3, $4)")
        .bind(activation_guard_id)
        .bind(context.workspace_id.as_uuid())
        .bind(connection.connection.id.as_uuid())
        .bind(slot_id)
        .execute(&mut *setup)
        .await
        .unwrap();
    connection.revision_id = ConnectionRevisionId::new();
    connection.expected_head_version = 1;
    connection.auth_mode = ConnectionAuthMode::Bearer;
    connection.credential_slot_id = Some(vestrace_domain::CredentialSlotId::from_uuid(slot_id));
    sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_connection_revision_and_advance_head(
             $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(connection.revision_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(connection.connection.id.as_uuid())
    .bind(connection.execution_guard_id)
    .bind("lm_studio_local")
    .bind("http://127.0.0.1:1234/v1")
    .bind("http://127.0.0.1:1234/v1")
    .bind("lm-studio-local/v1")
    .bind("loopback_only")
    .bind("bearer")
    .bind(slot_id)
    .bind(1_i64)
    .fetch_one(&mut *setup)
    .await
    .unwrap();
    setup.commit().await.unwrap();

    let credential_revision_id = Uuid::now_v7();
    let credential_intent_id = Uuid::now_v7();
    let occupancy_id = Uuid::now_v7();
    let credential_material_key_id = Uuid::now_v7();
    let candidate_credential_revision_id = Uuid::now_v7();
    let candidate_intent_id = Uuid::now_v7();
    let candidate_occupancy_id = Uuid::now_v7();
    let candidate_material_key_id = Uuid::now_v7();
    let mut seeded = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(context.workspace_id.to_string())
        .fetch_one(&mut *seeded)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *seeded)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO credential_revisions (id, workspace_id, credential_slot_id, material_key_id, associated_data_profile)
         VALUES ($1, $2, $3, $4, 'credential_v2')",
    )
    .bind(credential_revision_id)
    .bind(context.workspace_id.as_uuid())
    .bind(slot_id)
    .bind(credential_material_key_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO credential_guard_occupancies
         (id, workspace_id, connection_id, credential_slot_id, activation_guard_id, state)
         VALUES ($1, $2, $3, $4, $5, 'activated')",
    )
    .bind(occupancy_id)
    .bind(context.workspace_id.as_uuid())
    .bind(connection.connection.id.as_uuid())
    .bind(slot_id)
    .bind(activation_guard_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO credential_key_creation_intents
         (id, workspace_id, connection_id, credential_slot_id, occupancy_id, credential_revision_id,
          material_key_id, nonce, state, vault_receipt, bound_receipt)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'active', $9, $10)",
    )
    .bind(credential_intent_id)
    .bind(context.workspace_id.as_uuid())
    .bind(connection.connection.id.as_uuid())
    .bind(slot_id)
    .bind(occupancy_id)
    .bind(credential_revision_id)
    .bind(credential_material_key_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO credential_prepared_materials
         (id, workspace_id, intent_id, credential_revision_id, ciphertext)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(context.workspace_id.as_uuid())
    .bind(credential_intent_id)
    .bind(credential_revision_id)
    .bind(vec![0xA5_u8; 32])
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO credential_lifecycle_events
         (id, workspace_id, intent_id, credential_revision_id, event_kind)
         VALUES ($1, $2, $3, $4, 'candidate')",
    )
    .bind(Uuid::now_v7())
    .bind(context.workspace_id.as_uuid())
    .bind(credential_intent_id)
    .bind(credential_revision_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query("UPDATE credential_guard_occupancies SET intent_id = $2 WHERE id = $1")
        .bind(occupancy_id)
        .bind(credential_intent_id)
        .execute(&mut *seeded)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE credential_slots
            SET current_revision_id = $2, current_revision_version = 1
          WHERE workspace_id = $3 AND id = $1",
    )
    .bind(slot_id)
    .bind(credential_revision_id)
    .bind(context.workspace_id.as_uuid())
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO credential_revisions (id, workspace_id, credential_slot_id, material_key_id, associated_data_profile)
         VALUES ($1, $2, $3, $4, 'credential_v2')",
    )
    .bind(candidate_credential_revision_id)
    .bind(context.workspace_id.as_uuid())
    .bind(slot_id)
    .bind(candidate_material_key_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO credential_guard_occupancies
         (id, workspace_id, connection_id, credential_slot_id, activation_guard_id, state)
         VALUES ($1, $2, $3, $4, $5, 'candidate')",
    )
    .bind(candidate_occupancy_id)
    .bind(context.workspace_id.as_uuid())
    .bind(connection.connection.id.as_uuid())
    .bind(slot_id)
    .bind(activation_guard_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO credential_key_creation_intents
         (id, workspace_id, connection_id, credential_slot_id, occupancy_id, credential_revision_id,
          material_key_id, nonce, state, vault_receipt, bound_receipt)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'candidate', $9, $10)",
    )
    .bind(candidate_intent_id)
    .bind(context.workspace_id.as_uuid())
    .bind(connection.connection.id.as_uuid())
    .bind(slot_id)
    .bind(candidate_occupancy_id)
    .bind(candidate_credential_revision_id)
    .bind(candidate_material_key_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query("UPDATE credential_guard_occupancies SET intent_id = $2 WHERE id = $1")
        .bind(candidate_occupancy_id)
        .bind(candidate_intent_id)
        .execute(&mut *seeded)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO credential_prepared_materials
         (id, workspace_id, intent_id, credential_revision_id, ciphertext)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(context.workspace_id.as_uuid())
    .bind(candidate_intent_id)
    .bind(candidate_credential_revision_id)
    .bind(vec![0xA5_u8; 32])
    .execute(&mut *seeded)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO credential_lifecycle_events
         (id, workspace_id, intent_id, credential_revision_id, event_kind)
         VALUES ($1, $2, $3, $4, 'candidate')",
    )
    .bind(Uuid::now_v7())
    .bind(context.workspace_id.as_uuid())
    .bind(candidate_intent_id)
    .bind(candidate_credential_revision_id)
    .execute(&mut *seeded)
    .await
    .unwrap();
    seeded.commit().await.unwrap();

    CredentialBindingFixture {
        fixture: Fixture {
            context,
            connection_id: connection.connection.id,
            connection_revision_id: connection.revision_id,
            execution_guard_id: connection.execution_guard_id,
            provider_id,
        },
        slot_id,
        credential_revision_id,
        activation_guard_id,
        credential_intent_id,
        candidate_credential_revision_id,
        candidate_credential_intent_id: candidate_intent_id,
    }
}

fn model_command(
    fixture: &Fixture,
    model_id: ModelId,
    expected_head_version: u64,
) -> CreateModelRevision {
    let created_at = now();
    CreateModelRevision {
        model: ModelRecord {
            id: model_id,
            provider_id: fixture.provider_id,
            workspace_id: fixture.context.workspace_id,
            model_name: "test-model".to_owned(),
            context_window: 4096,
            input_cost_per_mtoken: 0.0,
            output_cost_per_mtoken: 0.0,
            created_at,
        },
        revision: ModelRevision::from_persisted(
            ModelRevisionId::new(),
            fixture.context.workspace_id,
            fixture.connection_revision_id,
            "test-model",
            ModelKind::Chat,
            ModelObservation::unknown(),
            ModelObservation::unknown(),
        )
        .unwrap(),
        connection_id: fixture.connection_id,
        execution_guard_id: fixture.execution_guard_id,
        expected_head_version,
        idempotency: Some(IdempotencyRecord {
            idempotency_key: format!("model-revision-{}", Uuid::now_v7()),
            workspace_id: fixture.context.workspace_id,
            request_hash: format!("hash-{}", Uuid::now_v7()),
            response_payload: None,
            status: "completed".to_owned(),
            created_at,
            expires_at: created_at + Duration::hours(1),
        }),
        outbox: vec![OutboxMessage::new(
            fixture.context.workspace_id,
            "model.revision.created",
            serde_json::json!({"model_id": model_id}),
            created_at,
        )],
        audit: AuditEvent::new(
            AuditEventId::new(),
            fixture.context.workspace_id,
            fixture.context.principal_id,
            "model.revision.created",
            "model",
            model_id.as_uuid(),
            serde_json::json!({"model_id": model_id}),
            created_at,
        )
        .unwrap(),
    }
}

fn default_command(
    fixture: &Fixture,
    model_id: ModelId,
    expected_version: u64,
) -> SetWorkspaceModelDefault {
    let created_at = now();
    SetWorkspaceModelDefault {
        default_id: Uuid::now_v7(),
        workspace_id: fixture.context.workspace_id,
        purpose: LEGACY_RUN_MODEL_DEFAULT_PURPOSE.to_owned(),
        model_id,
        required_capabilities: vec!["chat".to_owned()],
        expected_version,
        idempotency: Some(IdempotencyRecord {
            idempotency_key: format!("model-default-{}", Uuid::now_v7()),
            workspace_id: fixture.context.workspace_id,
            request_hash: format!("hash-{}", Uuid::now_v7()),
            response_payload: None,
            status: "completed".to_owned(),
            created_at,
            expires_at: created_at + Duration::hours(1),
        }),
        outbox: vec![OutboxMessage::new(
            fixture.context.workspace_id,
            "workspace.model-default.set",
            serde_json::json!({"purpose": LEGACY_RUN_MODEL_DEFAULT_PURPOSE, "model_id": model_id}),
            created_at,
        )],
        audit: AuditEvent::new(
            AuditEventId::new(),
            fixture.context.workspace_id,
            fixture.context.principal_id,
            "workspace.model-default.set",
            "workspace_model_default",
            model_id.as_uuid(),
            serde_json::json!({"purpose": LEGACY_RUN_MODEL_DEFAULT_PURPOSE, "model_id": model_id}),
            created_at,
        )
        .unwrap(),
    }
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
    let runtime = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            source
                .connect_options()
                .as_ref()
                .clone()
                .username(username)
                .password(password),
        )
        .await
        .unwrap();
    let role = sqlx::query(
        "SELECT current_user::text AS role, rolsuper, rolbypassrls \
         FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(role.get::<String, _>("role"), "vestrace");
    assert!(!role.get::<bool, _>("rolsuper"));
    assert!(!role.get::<bool, _>("rolbypassrls"));
    runtime
}

pub(crate) fn run_service(pool: &PgPool) -> RunCommandService {
    let store = PgStore::from_pool(pool.clone());
    RunCommandService::new(
        Arc::new(PgRunEventStore::new(store.clone())),
        Arc::new(PgRunCommandCommitter::new(store)),
    )
}

pub(crate) fn legacy_create_command(
    context: &RequestContext,
    run_id: AgentRunId,
) -> RunCommandEnvelope {
    RunCommandEnvelope {
        command_id: OperationId::new(),
        idempotency_key: None,
        workspace_id: context.workspace_id,
        run_id,
        actor: RunActor::Principal(context.principal_id),
        expected_version: RunVersion::ZERO,
        correlation_id: CorrelationId::new(),
        issued_at: now(),
        command: RunCommand::Create {
            principal_id: context.principal_id,
            title: "legacy model binding".to_owned(),
        },
    }
}

pub(crate) fn legacy_prepare_command(
    context: &RequestContext,
    run_id: AgentRunId,
) -> RunCommandEnvelope {
    RunCommandEnvelope {
        command_id: OperationId::new(),
        idempotency_key: None,
        workspace_id: context.workspace_id,
        run_id,
        actor: RunActor::Principal(context.principal_id),
        expected_version: RunVersion::INITIAL,
        correlation_id: CorrelationId::new(),
        issued_at: now(),
        command: RunCommand::Prepare,
    }
}

/// Task 7 owns qualification production. This deliberately uses the guarded
/// owner only to construct an otherwise valid refresh fixture, never a
/// production creator, so this creation-half test can prove the default is not
/// a qualification pointer.
async fn publish_test_only_qualification_refresh(
    pool: &PgPool,
    fixture: &Fixture,
    model_revision_id: Uuid,
    mode: QualificationRefreshMode,
) {
    let auth_mode: String = sqlx::query_scalar(
        "SELECT auth_mode FROM connection_revisions WHERE workspace_id = $1 AND id = $2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(fixture.connection_revision_id.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    let no_auth_binding_id: Option<Uuid> = if auth_mode == "none" {
        Some(
            sqlx::query_scalar(
                "SELECT id FROM no_auth_binding_revisions WHERE workspace_id = $1 AND connection_id = $2 \
                 AND connection_revision_id = $3",
            )
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.connection_id.as_uuid())
            .bind(fixture.connection_revision_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap(),
        )
    } else {
        None
    };
    let credential_target: Option<(Uuid, Uuid, Uuid, i64)> = if auth_mode == "none" {
        None
    } else {
        let revision_selector = if matches!(mode, QualificationRefreshMode::CredentialCandidate) {
            "(SELECT credential_revision_id FROM credential_key_creation_intents WHERE workspace_id = $1 AND connection_id = $2 AND credential_slot_id = slot.id AND state = 'candidate')"
        } else {
            "slot.current_revision_id"
        };
        Some(
            sqlx::query_as::<_, (Uuid, Uuid, Uuid, i64)>(&format!(
                "SELECT {revision_selector}, slot.id, activation.id,
                    slot.current_revision_version
               FROM credential_slots AS slot
               JOIN credential_activation_guards AS activation
                 ON activation.workspace_id = slot.workspace_id
                AND activation.connection_id = slot.connection_id
                AND activation.credential_slot_id = slot.id
              WHERE slot.workspace_id = $1 AND slot.connection_id = $2
                AND slot.id = (SELECT credential_slot_id FROM connection_revisions WHERE id = $3)",
            ))
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.connection_id.as_uuid())
            .bind(fixture.connection_revision_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap(),
        )
    };
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE vestrace_guarded_owner")
        .execute(&mut *transaction)
        .await
        .unwrap();

    let valid_until = match mode {
        QualificationRefreshMode::Valid
        | QualificationRefreshMode::DiscoveryOnly
        | QualificationRefreshMode::CredentialCandidate => "NOW() + INTERVAL '1 hour'",
        QualificationRefreshMode::Expired => "NOW() - INTERVAL '1 hour'",
    };

    for version in 1_i64..=2 {
        let job_id = Uuid::now_v7();
        let connection_qualification_id = Uuid::now_v7();
        let model_qualification_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO qualification_jobs \
             (id, workspace_id, connection_revision_id, profile_revision, state, completed_at) \
             VALUES ($1, $2, $3, $4, 'succeeded', NOW())",
        )
        .bind(job_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.connection_revision_id.as_uuid())
        .bind(format!("test-refresh/{version}"))
        .execute(&mut *transaction)
        .await
        .unwrap();
        if let Some(no_auth_binding_id) = no_auth_binding_id {
            sqlx::query(
                "INSERT INTO qualification_target_bindings \
                 (id, workspace_id, qualification_job_id, connection_id, connection_revision_id, branch, no_auth_binding_revision_id) \
                 VALUES ($1, $2, $3, $4, $5, 'no_auth', $6)",
            )
            .bind(Uuid::now_v7())
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(job_id)
            .bind(fixture.connection_id.as_uuid())
            .bind(fixture.connection_revision_id.as_uuid())
            .bind(no_auth_binding_id)
            .execute(&mut *transaction)
            .await
            .unwrap();
        } else {
            let (revision_id, slot_id, activation_guard_id, expected_slot_version) =
                credential_target.expect("credential fixture must have an active target");
            sqlx::query(
                "INSERT INTO qualification_target_bindings \
                 (id, workspace_id, qualification_job_id, connection_id, connection_revision_id, branch, credential_revision_id, credential_slot_id, credential_activation_guard_id, expected_slot_version) \
                 VALUES ($1, $2, $3, $4, $5, 'credential', $6, $7, $8, $9)",
            )
            .bind(Uuid::now_v7())
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(job_id)
            .bind(fixture.connection_id.as_uuid())
            .bind(fixture.connection_revision_id.as_uuid())
            .bind(revision_id)
            .bind(slot_id)
            .bind(activation_guard_id)
            .bind(expected_slot_version)
            .execute(&mut *transaction)
            .await
            .unwrap();
        }
        sqlx::query(&format!(
            "INSERT INTO connection_qualification_revisions \
             (id, workspace_id, connection_revision_id, qualification_job_id, profile_revision, valid_until, capabilities) \
             VALUES ($1, $2, $3, $4, $5, {valid_until}, ARRAY['chat'])"
        ))
        .bind(connection_qualification_id)
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(fixture.connection_revision_id.as_uuid())
        .bind(job_id)
        .bind(format!("test-refresh/{version}"))
        .execute(&mut *transaction)
        .await
        .unwrap();
        if !matches!(mode, QualificationRefreshMode::DiscoveryOnly) {
            sqlx::query(&format!(
                "INSERT INTO model_qualification_revisions \
                 (id, workspace_id, model_revision_id, connection_revision_id, connection_qualification_revision_id, qualification_job_id, capabilities, valid_until) \
                 VALUES ($1, $2, $3, $4, $5, $6, ARRAY['chat'], {valid_until})"
            ))
            .bind(model_qualification_id)
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(model_revision_id)
            .bind(fixture.connection_revision_id.as_uuid())
            .bind(connection_qualification_id)
            .bind(job_id)
            .execute(&mut *transaction)
            .await
            .unwrap();
        }
        if version == 1 {
            sqlx::query(
                "INSERT INTO connection_qualification_heads \
                 (workspace_id, connection_revision_id, current_qualification_revision_id, version) \
                 VALUES ($1, $2, $3, 1)",
            )
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.connection_revision_id.as_uuid())
            .bind(connection_qualification_id)
            .execute(&mut *transaction)
            .await
            .unwrap();
            if !matches!(mode, QualificationRefreshMode::DiscoveryOnly) {
                sqlx::query(
                    "INSERT INTO model_qualification_heads \
                     (workspace_id, model_revision_id, current_qualification_revision_id, version) \
                     VALUES ($1, $2, $3, 1)",
                )
                .bind(fixture.context.workspace_id.as_uuid())
                .bind(model_revision_id)
                .bind(model_qualification_id)
                .execute(&mut *transaction)
                .await
                .unwrap();
            }
        } else {
            sqlx::query(
                "UPDATE connection_qualification_heads \
                 SET current_qualification_revision_id = $3, version = version + 1 \
                 WHERE workspace_id = $1 AND connection_revision_id = $2",
            )
            .bind(fixture.context.workspace_id.as_uuid())
            .bind(fixture.connection_revision_id.as_uuid())
            .bind(connection_qualification_id)
            .execute(&mut *transaction)
            .await
            .unwrap();
            if !matches!(mode, QualificationRefreshMode::DiscoveryOnly) {
                sqlx::query(
                    "UPDATE model_qualification_heads \
                     SET current_qualification_revision_id = $3, version = version + 1 \
                     WHERE workspace_id = $1 AND model_revision_id = $2",
                )
                .bind(fixture.context.workspace_id.as_uuid())
                .bind(model_revision_id)
                .bind(model_qualification_id)
                .execute(&mut *transaction)
                .await
                .unwrap();
            }
        }
    }
    transaction.commit().await.unwrap();
}

pub(crate) async fn prepare_no_auth_binding(
    pool: &PgPool,
    mode: QualificationRefreshMode,
) -> (Fixture, ModelId, Uuid) {
    let fixture = fixture(pool).await;
    let repository = PgModelRevisionRepository::new(PgStore::from_pool(pool.clone()));
    let model_id = ModelId::new();
    let model = model_command(&fixture, model_id, 0);
    let model_revision_id = model.revision.id().as_uuid();
    repository
        .create_governed(fixture.context.clone(), model)
        .await
        .unwrap();
    repository
        .set_workspace_default_governed(
            fixture.context.clone(),
            default_command(&fixture, model_id, 0),
        )
        .await
        .unwrap();
    publish_test_only_qualification_refresh(pool, &fixture, model_revision_id, mode).await;
    (fixture, model_id, model_revision_id)
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn safe_model_and_provider_projections_require_current_qualified_revision_tuples(
    pool: PgPool,
) {
    let (fixture, model_id, model_revision_id) =
        prepare_no_auth_binding(&pool, QualificationRefreshMode::Valid).await;

    // Compatibility catalog rows without heads or qualifications remain
    // readable to migration owners, but must never appear as a governed
    // executable/qualified Model or Provider projection.
    let legacy_provider_id = ProviderId::new();
    let legacy_model_id = ModelId::new();
    sqlx::query(
        "INSERT INTO providers (id,workspace_id,name,locality) VALUES ($1,$2,'legacy-only','local')",
    )
    .bind(legacy_provider_id.as_uuid())
    .bind(fixture.context.workspace_id.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO models \
         (id,provider_id,workspace_id,model_name,context_window,input_cost_per_mtoken,output_cost_per_mtoken) \
         VALUES ($1,$2,$3,'legacy-only',4096,0,0)",
    )
    .bind(legacy_model_id.as_uuid())
    .bind(legacy_provider_id.as_uuid())
    .bind(fixture.context.workspace_id.as_uuid())
    .execute(&pool)
    .await
    .unwrap();

    let runtime = runtime_pool(&pool).await;
    let repository = PgModelRevisionRepository::new(PgStore::from_pool(runtime.clone()));
    let models = repository
        .list_safe_models(&fixture.context)
        .await
        .expect("restricted runtime lists only governed model projections");
    assert_eq!(models.len(), 2);
    let governed = models
        .iter()
        .find(|model| model.id == model_id)
        .expect("the qualified governed Model is projected");
    assert_eq!(
        governed
            .revision_id
            .expect("the governed Model has its opaque revision id")
            .as_uuid(),
        model_revision_id
    );
    assert_eq!(governed.state, "qualified");
    assert_eq!(governed.qualification_state, "qualified");
    assert!(governed.blockers.is_empty());
    let legacy = models
        .iter()
        .find(|model| model.id == legacy_model_id)
        .expect("the legacy Model is visibly non-executable, not silently qualified");
    assert_eq!(legacy.revision_id, None);
    assert_eq!(legacy.state, "legacy");
    assert_eq!(legacy.qualification_state, "missing");
    assert_eq!(legacy.blockers, ["missing_governed_model_revision"]);

    let providers = repository
        .list_safe_providers(&fixture.context)
        .await
        .expect("restricted runtime derives providers only from governed tuples");
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].id, fixture.provider_id);
    assert_eq!(providers[0].state, "qualified");
    assert!(providers[0].blockers.is_empty());
    assert!(
        providers
            .iter()
            .all(|provider| provider.id != legacy_provider_id)
    );
    runtime.close().await;
}

pub(crate) async fn prepare_credential_binding(pool: &PgPool) -> (CredentialBindingFixture, Uuid) {
    let credential = credential_binding_fixture(pool).await;
    let repository = PgModelRevisionRepository::new(PgStore::from_pool(pool.clone()));
    let model_id = ModelId::new();
    let model = model_command(&credential.fixture, model_id, 0);
    let model_revision_id = model.revision.id().as_uuid();
    repository
        .create_governed(credential.fixture.context.clone(), model)
        .await
        .unwrap();
    repository
        .set_workspace_default_governed(
            credential.fixture.context.clone(),
            default_command(&credential.fixture, model_id, 0),
        )
        .await
        .unwrap();
    publish_test_only_qualification_refresh(
        pool,
        &credential.fixture,
        model_revision_id,
        QualificationRefreshMode::Valid,
    )
    .await;
    (credential, model_revision_id)
}

async fn prepare_candidate_credential_binding(pool: &PgPool) -> CredentialBindingFixture {
    let credential = credential_binding_fixture(pool).await;
    let repository = PgModelRevisionRepository::new(PgStore::from_pool(pool.clone()));
    let model_id = ModelId::new();
    let model = model_command(&credential.fixture, model_id, 0);
    let model_revision_id = model.revision.id().as_uuid();
    repository
        .create_governed(credential.fixture.context.clone(), model)
        .await
        .unwrap();
    repository
        .set_workspace_default_governed(
            credential.fixture.context.clone(),
            default_command(&credential.fixture, model_id, 0),
        )
        .await
        .unwrap();
    publish_test_only_qualification_refresh(
        pool,
        &credential.fixture,
        model_revision_id,
        QualificationRefreshMode::CredentialCandidate,
    )
    .await;
    credential
}

async fn assert_runtime_binding_refusal(
    pool: &PgPool,
    fixture: &Fixture,
    expected_constraint: &str,
) {
    let runtime = runtime_pool(pool).await;
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let error = sqlx::query_scalar::<_, Uuid>(
        "SELECT vestrace_create_run_model_binding_snapshot($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(AgentRunId::new().as_uuid())
    .bind(LEGACY_RUN_MODEL_DEFAULT_PURPOSE)
    .fetch_one(&mut *transaction)
    .await
    .expect_err("the resolver predicate must refuse before it creates a snapshot");
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("23514")
    );
    assert_eq!(
        error
            .as_database_error()
            .and_then(|database| database.constraint()),
        Some(expected_constraint)
    );
    transaction.rollback().await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn model_revision_head_uses_cas_and_preserves_prior_immutable_row(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let repository = PgModelRevisionRepository::new(PgStore::from_pool(pool.clone()));
    let model_id = ModelId::new();
    let first = model_command(&fixture, model_id, 0);
    repository
        .create_governed(fixture.context.clone(), first.clone())
        .await
        .unwrap();
    let first_head_version: i64 = sqlx::query_scalar(
        "SELECT version FROM model_revision_heads WHERE workspace_id = $1 AND model_id = $2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(model_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(first_head_version, 1);
    let first_row: (Uuid, String, String) =
        sqlx::query_as("SELECT id, wire_model_id, kind FROM model_revisions WHERE id = $1")
            .bind(first.revision.id().as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let second = model_command(&fixture, model_id, 1);
    repository
        .create_governed(fixture.context.clone(), second.clone())
        .await
        .unwrap();
    let head: (Uuid, i64) = sqlx::query_as(
        "SELECT current_revision_id, version FROM model_revision_heads WHERE workspace_id = $1 AND model_id = $2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(model_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(head, (second.revision.id().as_uuid(), 2));
    assert_eq!(
        sqlx::query_as::<_, (Uuid, String, String)>(
            "SELECT id, wire_model_id, kind FROM model_revisions WHERE id = $1",
        )
        .bind(first.revision.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        first_row,
        "advancing a model head must leave the previous revision byte-identical",
    );

    let stale = model_command(&fixture, model_id, 1);
    let result = repository
        .create_governed(fixture.context.clone(), stale.clone())
        .await;
    assert!(matches!(
        result,
        Err(ApplicationError::Conflict(code)) if code == "MODEL_VERSION_CONFLICT"
    ));
    let stale_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM model_revisions WHERE id = $1")
        .bind(stale.revision.id().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        stale_rows, 0,
        "a stale revision CAS must leave no revision behind"
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn workspace_default_has_its_own_cas_and_qualification_refresh_cannot_rewrite_it(
    pool: PgPool,
) {
    let fixture = fixture(&pool).await;
    let repository = PgModelRevisionRepository::new(PgStore::from_pool(pool.clone()));
    let first_model = ModelId::new();
    repository
        .create_governed(
            fixture.context.clone(),
            model_command(&fixture, first_model, 0),
        )
        .await
        .unwrap();
    let first_default = default_command(&fixture, first_model, 0);
    repository
        .set_workspace_default_governed(fixture.context.clone(), first_default.clone())
        .await
        .unwrap();
    let second_model = ModelId::new();
    repository
        .create_governed(
            fixture.context.clone(),
            model_command(&fixture, second_model, 0),
        )
        .await
        .unwrap();
    let replacement = default_command(&fixture, second_model, 1);
    repository
        .set_workspace_default_governed(fixture.context.clone(), replacement.clone())
        .await
        .unwrap();
    let default_before_refresh: (Uuid, Vec<String>, i64) = sqlx::query_as(
        "SELECT model_id, required_capabilities, version FROM workspace_model_defaults \
         WHERE workspace_id = $1 AND purpose = $2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(LEGACY_RUN_MODEL_DEFAULT_PURPOSE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        default_before_refresh,
        (second_model.as_uuid(), vec!["chat".to_owned()], 2)
    );

    let stale = default_command(&fixture, first_model, 1);
    let result = repository
        .set_workspace_default_governed(fixture.context.clone(), stale)
        .await;
    assert!(matches!(
        result,
        Err(ApplicationError::Conflict(code)) if code == "WORKSPACE_MODEL_DEFAULT_VERSION_CONFLICT"
    ));

    let second_model_revision_id: Uuid = sqlx::query_scalar(
        "SELECT current_revision_id FROM model_revision_heads WHERE workspace_id = $1 AND model_id = $2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(second_model.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    publish_test_only_qualification_refresh(
        &pool,
        &fixture,
        second_model_revision_id,
        QualificationRefreshMode::Valid,
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT version FROM model_qualification_heads WHERE workspace_id = $1 AND model_revision_id = $2",
        )
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(second_model_revision_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        2,
        "the fixture must publish a real replacement qualification head",
    );

    // The refresh above leaves the default untouched. A restricted runtime
    // connection can read it but cannot bypass the dedicated default CAS.
    let runtime = runtime_pool(&pool).await;
    let mut transaction = runtime.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id', $1, true)")
        .bind(fixture.context.workspace_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let refresh_rewrite = sqlx::query(
        "UPDATE workspace_model_defaults SET model_id = $1 \
         WHERE workspace_id = $2 AND purpose = $3",
    )
    .bind(first_model.as_uuid())
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(LEGACY_RUN_MODEL_DEFAULT_PURPOSE)
    .execute(&mut *transaction)
    .await
    .expect_err("a qualification refresh cannot rewrite the workspace default");
    assert_eq!(
        refresh_rewrite
            .as_database_error()
            .and_then(|database| database.code())
            .as_deref(),
        Some("42501")
    );
    transaction.rollback().await.unwrap();
    runtime.close().await;
    assert_eq!(
        sqlx::query_as::<_, (Uuid, Vec<String>, i64)>(
            "SELECT model_id, required_capabilities, version FROM workspace_model_defaults \
             WHERE workspace_id = $1 AND purpose = $2",
        )
        .bind(fixture.context.workspace_id.as_uuid())
        .bind(LEGACY_RUN_MODEL_DEFAULT_PURPOSE)
        .fetch_one(&pool)
        .await
        .unwrap(),
        default_before_refresh,
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn runtime_role_cannot_insert_model_revisions_heads_or_defaults_directly(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let runtime = runtime_pool(&pool).await;
    for statement in [
        "INSERT INTO model_revisions (id, workspace_id, model_id, connection_revision_id, wire_model_id, kind) VALUES (gen_random_uuid(), current_setting('vestrace.workspace_id')::uuid, gen_random_uuid(), gen_random_uuid(), 'raw', 'chat')",
        "INSERT INTO model_revision_heads (workspace_id, model_id, current_revision_id, version) VALUES (current_setting('vestrace.workspace_id')::uuid, gen_random_uuid(), gen_random_uuid(), 1)",
        "INSERT INTO workspace_model_defaults (id, workspace_id, purpose, model_id, required_capabilities, version) VALUES (gen_random_uuid(), current_setting('vestrace.workspace_id')::uuid, 'raw', gen_random_uuid(), ARRAY['chat'], 1)",
    ] {
        let mut transaction = runtime.begin().await.unwrap();
        sqlx::query_scalar::<_, String>("SELECT set_config('vestrace.workspace_id', $1, true)")
            .bind(fixture.context.workspace_id.to_string())
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        let error = sqlx::query(statement)
            .execute(&mut *transaction)
            .await
            .expect_err("the runtime role must not mutate guarded model state directly");
        assert_eq!(
            error
                .as_database_error()
                .and_then(|database| database.code())
                .as_deref(),
            Some("42501")
        );
        transaction.rollback().await.unwrap();
    }
    runtime.close().await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn legacy_run_pins_the_no_auth_branch_and_link_in_its_acceptance_transaction(pool: PgPool) {
    let (fixture, _, _) = prepare_no_auth_binding(&pool, QualificationRefreshMode::Valid).await;
    let run_id = AgentRunId::new();
    let service = run_service(&pool);
    service
        .execute(
            &fixture.context,
            legacy_create_command(&fixture.context, run_id),
        )
        .await
        .expect("creation alone must not resolve a model binding");
    service
        .execute(
            &fixture.context,
            legacy_prepare_command(&fixture.context, run_id),
        )
        .await
        .expect("a complete no-auth binding must make legacy Run acceptance executable");

    let row: NoAuthSnapshotColumns = sqlx::query_as(
        "SELECT snapshot.branch, snapshot.credential_revision_id,
                    snapshot.credential_slot_id, snapshot.credential_activation_guard_id,
                    snapshot.expected_slot_version, snapshot.no_auth_binding_revision_id
               FROM run_model_binding_snapshots AS link
               JOIN model_binding_snapshots AS snapshot
                 ON snapshot.workspace_id = link.workspace_id
                AND snapshot.id = link.snapshot_id
              WHERE link.workspace_id = $1 AND link.run_id = $2",
    )
    .bind(fixture.context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "no_auth");
    assert_eq!((row.1, row.2, row.3, row.4), (None, None, None, None));
    assert!(row.5.is_some());
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn legacy_run_pins_the_credential_branch_exclusively(pool: PgPool) {
    let (credential, _) = prepare_credential_binding(&pool).await;
    let run_id = AgentRunId::new();
    let service = run_service(&pool);
    service
        .execute(
            &credential.fixture.context,
            legacy_create_command(&credential.fixture.context, run_id),
        )
        .await
        .expect("creation alone must not resolve a model binding");
    service
        .execute(
            &credential.fixture.context,
            legacy_prepare_command(&credential.fixture.context, run_id),
        )
        .await
        .expect("a complete active credential binding must make legacy Run acceptance executable");

    let row: (String, Uuid, Uuid, Uuid, i64, Option<Uuid>) = sqlx::query_as(
        "SELECT snapshot.branch, snapshot.credential_revision_id,
                snapshot.credential_slot_id, snapshot.credential_activation_guard_id,
                snapshot.expected_slot_version, snapshot.no_auth_binding_revision_id
           FROM run_model_binding_snapshots AS link
           JOIN model_binding_snapshots AS snapshot
             ON snapshot.workspace_id = link.workspace_id
            AND snapshot.id = link.snapshot_id
          WHERE link.workspace_id = $1 AND link.run_id = $2",
    )
    .bind(credential.fixture.context.workspace_id.as_uuid())
    .bind(run_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "credential");
    assert_eq!(row.1, credential.credential_revision_id);
    assert_eq!(row.2, credential.slot_id);
    assert_eq!(row.3, credential.activation_guard_id);
    assert_eq!(row.4, 1);
    assert_eq!(row.5, None);
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn candidate_credential_outside_its_guarded_activation_path_cannot_produce_a_snapshot(
    pool: PgPool,
) {
    let credential = prepare_candidate_credential_binding(&pool).await;
    assert_ne!(
        credential.candidate_credential_revision_id,
        credential.credential_revision_id
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM credential_key_creation_intents WHERE id = $1",
        )
        .bind(credential.credential_intent_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "active"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM credential_key_creation_intents WHERE id = $1",
        )
        .bind(credential.candidate_credential_intent_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "candidate"
    );
    assert_runtime_binding_refusal(
        &pool,
        &credential.fixture,
        "model_binding_auth_branch_refused",
    )
    .await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn incompatible_qualification_cannot_produce_a_snapshot(pool: PgPool) {
    let (fixture, model_id, _) =
        prepare_no_auth_binding(&pool, QualificationRefreshMode::Valid).await;
    let repository = PgModelRevisionRepository::new(PgStore::from_pool(pool.clone()));
    let mut replacement = default_command(&fixture, model_id, 1);
    replacement.required_capabilities = vec!["chat".to_owned(), "tools".to_owned()];
    repository
        .set_workspace_default_governed(fixture.context.clone(), replacement)
        .await
        .unwrap();
    assert_runtime_binding_refusal(&pool, &fixture, "model_binding_qualification_incompatible")
        .await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn expired_qualification_cannot_produce_a_snapshot(pool: PgPool) {
    let (fixture, _, _) = prepare_no_auth_binding(&pool, QualificationRefreshMode::Expired).await;
    assert_runtime_binding_refusal(&pool, &fixture, "model_binding_qualification_expired").await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn discovery_only_evidence_cannot_produce_a_snapshot(pool: PgPool) {
    let (fixture, _, _) =
        prepare_no_auth_binding(&pool, QualificationRefreshMode::DiscoveryOnly).await;
    assert_runtime_binding_refusal(&pool, &fixture, "model_binding_qualification_incompatible")
        .await;
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn raw_legacy_catalog_rows_cannot_produce_a_snapshot(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let model_id = ModelId::new();
    sqlx::query(
        "INSERT INTO models (id, provider_id, workspace_id, model_name, context_window,
                             input_cost_per_mtoken, output_cost_per_mtoken)
         VALUES ($1, $2, $3, 'legacy-raw', 1024, 0, 0)",
    )
    .bind(model_id.as_uuid())
    .bind(fixture.provider_id.as_uuid())
    .bind(fixture.context.workspace_id.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    PgModelRevisionRepository::new(PgStore::from_pool(pool.clone()))
        .set_workspace_default_governed(
            fixture.context.clone(),
            default_command(&fixture, model_id, 0),
        )
        .await
        .unwrap();
    assert_runtime_binding_refusal(&pool, &fixture, "legacy_run_chat_default_not_current_chat")
        .await;
}

#[test]
fn no_auth_resolution_explicitly_rejects_every_credential_reference() {
    let migration =
        include_str!("../../../migrations/0178_model_revisions_and_binding_snapshots.sql");
    for predicate in [
        "target_binding_row.credential_revision_id IS NOT NULL",
        "target_binding_row.credential_slot_id IS NOT NULL",
        "target_binding_row.credential_activation_guard_id IS NOT NULL",
        "target_binding_row.expected_slot_version IS NOT NULL",
    ] {
        assert!(
            migration.contains(predicate),
            "the no-auth resolver must explicitly reject {predicate}",
        );
    }
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn created_only_run_commits_without_a_model_binding_snapshot(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let run_id = AgentRunId::new();
    run_service(&pool)
        .execute(
            &fixture.context,
            legacy_create_command(&fixture.context, run_id),
        )
        .await
        .expect("a Run that remains Created must not need a chat model default");

    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM agent_runs WHERE id = $1")
            .bind(run_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
        "created",
        "a create-only Run must remain non-executable",
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM run_model_binding_snapshots WHERE run_id = $1",
        )
        .bind(run_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "a create-only Run must not have a binding link",
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM model_binding_snapshots")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
        "a create-only Run must not create a binding snapshot",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn executable_run_without_chat_default_is_refused_without_binding_or_work_item(pool: PgPool) {
    let fixture = fixture(&pool).await;
    let run_id = AgentRunId::new();
    let service = run_service(&pool);
    service
        .execute(
            &fixture.context,
            legacy_create_command(&fixture.context, run_id),
        )
        .await
        .expect("creation alone must not resolve a model binding");
    let error = service
        .execute(
            &fixture.context,
            legacy_prepare_command(&fixture.context, run_id),
        )
        .await
        .expect_err("a Run cannot become executable without the chat default binding");

    assert!(
        matches!(
            error,
            ApplicationError::Policy(ref code) if code == "MODEL_BINDING_LEGACY_CHAT_DEFAULT_ABSENT"
        ),
        "legacy default refusal must be attributable, got {error:?}"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM agent_runs WHERE id = $1")
            .bind(run_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
        "created",
        "a refused acceptance must leave no executable Run",
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM run_model_binding_snapshots WHERE run_id = $1",
        )
        .bind(run_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "a refused acceptance must leave no binding link",
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM run_work_items WHERE run_id = $1")
            .bind(run_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
        "a refused acceptance must leave no work item",
    );
}

#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn creating_a_model_on_a_fresh_workspace_materializes_its_provider(pool: PgPool) {
    let context = context();
    let connection_cmd = connection_command(&context);
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(context.workspace_id.as_uuid())
        .bind(format!("model-{}", context.workspace_id))
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
    sqlx::query(
        "INSERT INTO connectors (id, workspace_id, name, provider_type) VALUES ($1, $2, $3, $4)",
    )
    .bind(connection_cmd.connection.connector_id.as_uuid())
    .bind(context.workspace_id.as_uuid())
    .bind(format!(
        "connector-{}",
        connection_cmd.connection.connector_id
    ))
    .bind("local")
    .execute(&pool)
    .await
    .unwrap();

    PgConnectionRevisionRepository::new(PgStore::from_pool(pool.clone()))
        .create_governed(context.clone(), connection_cmd.clone())
        .await
        .unwrap();

    let model_id = ModelId::new();
    let provider_id = ProviderId::new();
    let created_at = now();
    let model_cmd = CreateModelRevision {
        model: ModelRecord {
            id: model_id,
            provider_id,
            workspace_id: context.workspace_id,
            model_name: "test-model".to_owned(),
            context_window: 4096,
            input_cost_per_mtoken: 0.0,
            output_cost_per_mtoken: 0.0,
            created_at,
        },
        revision: ModelRevision::from_persisted(
            ModelRevisionId::new(),
            context.workspace_id,
            connection_cmd.revision_id,
            "test-model",
            ModelKind::Chat,
            ModelObservation::unknown(),
            ModelObservation::unknown(),
        )
        .unwrap(),
        connection_id: connection_cmd.connection.id,
        execution_guard_id: connection_cmd.execution_guard_id,
        expected_head_version: 0,
        idempotency: Some(IdempotencyRecord {
            idempotency_key: format!("model-revision-{}", Uuid::now_v7()),
            workspace_id: context.workspace_id,
            request_hash: format!("hash-{}", Uuid::now_v7()),
            response_payload: None,
            status: "completed".to_owned(),
            created_at,
            expires_at: created_at + Duration::hours(1),
        }),
        outbox: vec![OutboxMessage::new(
            context.workspace_id,
            "model.revision.created",
            serde_json::json!({"model_id": model_id}),
            created_at,
        )],
        audit: AuditEvent::new(
            AuditEventId::new(),
            context.workspace_id,
            context.principal_id,
            "model.revision.created",
            "model",
            model_id.as_uuid(),
            serde_json::json!({"model_id": model_id}),
            created_at,
        )
        .unwrap(),
    };

    let repository = PgModelRevisionRepository::new(PgStore::from_pool(pool.clone()));
    repository
        .create_governed(context.clone(), model_cmd.clone())
        .await
        .unwrap();

    let locality: String = sqlx::query_scalar("SELECT locality FROM providers WHERE id = $1")
        .bind(provider_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(locality, "governed");
}
